package app.micnexus.android

import android.Manifest
import android.annotation.SuppressLint
import android.bluetooth.BluetoothAdapter
import android.bluetooth.BluetoothClass
import android.bluetooth.BluetoothManager
import android.bluetooth.BluetoothSocket
import android.content.Context
import android.content.SharedPreferences
import android.content.pm.PackageManager
import android.net.wifi.WifiManager
import android.os.Build
import android.os.Handler
import android.os.HandlerThread
import android.os.SystemClock
import android.provider.Settings
import android.util.Log
import androidx.core.content.ContextCompat
import org.json.JSONObject
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetSocketAddress
import java.net.ServerSocket
import java.net.Socket
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.CopyOnWriteArrayList

/**
 * Everything the phone does, independent of the UI (works with the app in the
 * background, driven by the notification and the Quick Settings tile):
 *
 * - answers the PC's Wi‑Fi discovery (`want` = the PC it wants now);
 * - listens on TCP for the PC (Wi‑Fi, or USB through `adb forward`);
 * - connects to paired PCs over Bluetooth RFCOMM;
 * - runs the [Session], the [MicCapture] and the [MicService] notification;
 * - reconnects for up to 60 s when the link drops.
 */
@SuppressLint("StaticFieldLeak")
object Engine : Session.Events {
    enum class Status { IDLE, CONNECTING, PENDING, CONNECTED, RECONNECTING }

    data class Target(val pcId: String?, val name: String, val transport: String, val address: String)

    class Pc(val id: String, @Volatile var name: String) {
        @Volatile var wifiAddress: String? = null
        @Volatile var wifiSeen = 0L
        @Volatile var busy = false
        val wifiAlive get() = SystemClock.elapsedRealtime() - wifiSeen < PC_TTL_MS
    }

    interface Listener {
        fun onState() {}
        fun onPcs() {}
        fun onLevel(level: Float) {}
    }

    private const val TAG = "Engine"
    private const val PC_TTL_MS = 5000L
    private const val RECONNECT_MS = 60_000L
    private const val CONNECT_TIMEOUT_MS = 10_000L

    lateinit var app: Context
        private set
    private lateinit var prefs: SharedPreferences
    private lateinit var handler: Handler
    private var started = false

    lateinit var deviceId: String
        private set
    val deviceName: String
        get() = (Settings.Global.getString(app.contentResolver, Settings.Global.DEVICE_NAME) ?: Build.MODEL).trim()

    private val listeners = CopyOnWriteArrayList<Listener>()
    private val pcs = ConcurrentHashMap<String, Pc>()

    // ---- state (guarded by `this`)
    @Volatile var status = Status.IDLE
        private set
    @Volatile var target: Target? = null
        private set
    @Volatile var muted = false
        private set
    @Volatile var codec: String? = null
        private set
    @Volatile var rttMs = -1
        private set
    @Volatile var bufferMs = -1
        private set
    @Volatile var error: String? = null
        private set
    @Volatile var connectedAt = 0L
        private set
    @Volatile var foreground = false
        private set
    @Volatile var level = 0f
        private set

    private var session: Session? = null
    private var capture: MicCapture? = null
    private var userDisconnected = false
    private var usbIntentAt = 0L
    private var reconnectUntil = 0L
    private var multicastLock: WifiManager.MulticastLock? = null

    // ---------------------------------------------------------- settings --

    var autoConnect: Boolean
        get() = prefs.getBoolean("autoConnect", true)
        set(v) = prefs.edit().putBoolean("autoConnect", v).apply()
    var noiseSuppression: Boolean
        get() = prefs.getBoolean("noiseSuppression", true)
        set(v) = prefs.edit().putBoolean("noiseSuppression", v).apply()
    var gain: Float
        get() = prefs.getFloat("gain", 1f)
        set(v) = prefs.edit().putFloat("gain", v.coerceIn(0.5f, 4f)).apply()
    val lastPcId: String? get() = prefs.getString("lastPcId", null)
    private val lastTransport: String? get() = prefs.getString("lastTransport", null)

    /** Bluetooth address of each PC seen over Bluetooth (pcId → MAC). */
    private fun knownBluetooth(): JSONObject = JSONObject(prefs.getString("knownBt", "{}") ?: "{}")

    // -------------------------------------------------------------- init --

    @Synchronized
    fun init(context: Context) {
        if (started) return
        started = true
        app = context.applicationContext
        prefs = app.getSharedPreferences("micnexus", Context.MODE_PRIVATE)
        deviceId = prefs.getString("deviceId", null) ?: UUID.randomUUID().toString().also {
            prefs.edit().putString("deviceId", it).apply()
        }
        handler = Handler(HandlerThread("engine").apply { start() }.looper)
        Thread({ discoveryLoop() }, "discovery").start()
        Thread({ serverLoop() }, "server").start()
        handler.postDelayed(object : Runnable {
            override fun run() {
                prunePcs()
                handler.postDelayed(this, 2000)
            }
        }, 2000)
    }

    fun addListener(l: Listener) = listeners.add(l)
    fun removeListener(l: Listener) = listeners.remove(l)

    private fun notifyState() {
        listeners.forEach { it.onState() }
        MicService.refresh(app)
        MicTileService.refresh(app)
    }

    private fun notifyPcs() = listeners.forEach { it.onPcs() }

    // -------------------------------------------------------- foreground --

    /** The activity came to the front (true) or left it (false). */
    fun setForeground(value: Boolean) {
        foreground = value
        if (value) {
            acquireMulticast()
            synchronized(this) {
                if (status == Status.IDLE && autoConnect && !userDisconnected && lastTransport == "bluetooth") {
                    val addr = lastPcId?.let { knownBluetooth().optString(it, "") }.orEmpty()
                    if (addr.isNotEmpty()) connectBluetooth(addr, prefs.getString("lastPcName", "PC") ?: "PC", lastPcId)
                }
            }
        } else if (status == Status.IDLE) {
            releaseMulticast()
        }
        notifyState()
    }

    /** MainActivity was opened by the PC over USB (`am start --ez usb true`). */
    fun onUsbIntent() {
        usbIntentAt = SystemClock.elapsedRealtime()
        userDisconnected = false
    }

    private fun acquireMulticast() {
        if (multicastLock?.isHeld == true) return
        try {
            val wifi = app.getSystemService(WifiManager::class.java)
            multicastLock = wifi.createMulticastLock("micnexus-discovery").apply {
                setReferenceCounted(false)
                acquire()
            }
        } catch (e: Exception) {
            Log.w(TAG, "multicast lock", e)
        }
    }

    private fun releaseMulticast() {
        try { multicastLock?.release() } catch (_: Exception) {}
    }

    // ------------------------------------------------------- discovery --

    /** The PC this phone wants to talk to right now (announced in discovery replies). */
    @Synchronized
    fun currentWant(): String? = when (status) {
        Status.CONNECTING, Status.RECONNECTING -> target?.takeIf { it.transport == "wifi" || status == Status.RECONNECTING }?.pcId
        Status.IDLE -> lastPcId.takeIf { foreground && autoConnect && !userDisconnected && hasMicPermission() }
        else -> null
    }

    private fun discoveryLoop() {
        while (true) {
            try {
                DatagramSocket(null).use { socket ->
                    socket.reuseAddress = true
                    socket.broadcast = true
                    socket.bind(InetSocketAddress(Protocol.DISCOVERY_PORT))
                    val buf = ByteArray(2048)
                    while (true) {
                        val packet = DatagramPacket(buf, buf.size)
                        socket.receive(packet)
                        val data = packet.data.copyOfRange(packet.offset, packet.offset + packet.length)
                        if (data.size < 4 || !data.copyOfRange(0, 4).contentEquals(Protocol.DISCOVER_REQUEST)) continue
                        val info = try { JSONObject(String(data, 4, data.size - 4)) } catch (_: Exception) { continue }
                        if (info.optInt("v") != Protocol.VERSION) continue
                        val id = info.optString("id")
                        if (id.isEmpty()) continue
                        val pc = pcs.getOrPut(id) { Pc(id, info.optString("name", "PC")) }
                        val wasAlive = pc.wifiAlive
                        val changed = pc.name != info.optString("name", pc.name) ||
                            pc.wifiAddress != packet.address.hostAddress || pc.busy != info.optBoolean("busy")
                        pc.name = info.optString("name", pc.name)
                        pc.wifiAddress = packet.address.hostAddress
                        pc.busy = info.optBoolean("busy")
                        pc.wifiSeen = SystemClock.elapsedRealtime()
                        if (!wasAlive || changed) notifyPcs()
                        val reply = JSONObject()
                            .put("v", Protocol.VERSION)
                            .put("id", deviceId)
                            .put("name", deviceName)
                            .put("model", Build.MODEL)
                            .put("port", Protocol.PHONE_PORT)
                            .put("want", currentWant() ?: JSONObject.NULL)
                        val bytes = Protocol.DISCOVER_REPLY + reply.toString().toByteArray()
                        socket.send(DatagramPacket(bytes, bytes.size, packet.socketAddress))
                    }
                }
            } catch (e: Exception) {
                Log.w(TAG, "discovery", e)
                Thread.sleep(2000)
            }
        }
    }

    private fun prunePcs() {
        if (pcs.values.any { it.wifiAddress != null && !it.wifiAlive }) {
            pcs.values.filter { !it.wifiAlive }.forEach { it.wifiAddress = null }
            notifyPcs()
        }
    }

    /** PCs to show: found on Wi‑Fi and/or paired over Bluetooth. */
    @SuppressLint("MissingPermission")
    fun pcList(): List<Map<String, Any?>> {
        val out = LinkedHashMap<String, MutableMap<String, Any?>>()
        for (pc in pcs.values) {
            if (!pc.wifiAlive) continue
            out[pc.id] = mutableMapOf("key" to pc.id, "pcId" to pc.id, "name" to pc.name, "wifi" to pc.wifiAddress, "bluetooth" to null, "busy" to pc.busy)
        }
        val known = knownBluetooth()
        val byAddress = known.keys().asSequence().associateBy { known.optString(it) }
        for (device in bondedComputers()) {
            val pcId = byAddress[device.first]
            val entry = pcId?.let { out[it] }
            if (entry != null) {
                entry["bluetooth"] = device.first
            } else {
                val key = pcId ?: "bt:${device.first}"
                out[key] = mutableMapOf("key" to key, "pcId" to pcId, "name" to device.second, "wifi" to null, "bluetooth" to device.first, "busy" to false)
            }
        }
        val last = lastPcId
        return out.values.map { it.apply { put("last", it["pcId"] != null && it["pcId"] == last) } }
            .sortedWith(compareByDescending<Map<String, Any?>> { it["last"] == true }.thenByDescending { it["wifi"] != null })
    }

    private fun bluetoothAdapter(): BluetoothAdapter? =
        app.getSystemService(BluetoothManager::class.java)?.adapter

    fun hasBluetoothPermission(): Boolean =
        Build.VERSION.SDK_INT < Build.VERSION_CODES.S ||
            ContextCompat.checkSelfPermission(app, Manifest.permission.BLUETOOTH_CONNECT) == PackageManager.PERMISSION_GRANTED

    fun bluetoothOn(): Boolean = try { bluetoothAdapter()?.isEnabled == true } catch (_: SecurityException) { false }

    /** (MAC, name) of paired computers, plus any device we already used as a PC. */
    @SuppressLint("MissingPermission")
    private fun bondedComputers(): List<Pair<String, String>> {
        if (!hasBluetoothPermission()) return emptyList()
        val adapter = bluetoothAdapter() ?: return emptyList()
        return try {
            if (!adapter.isEnabled) return emptyList()
            val known = knownBluetooth().let { k -> k.keys().asSequence().map { k.optString(it) }.toSet() }
            adapter.bondedDevices.orEmpty()
                .filter { it.bluetoothClass?.majorDeviceClass == BluetoothClass.Device.Major.COMPUTER || it.address in known }
                .map { it.address to (it.name ?: it.address) }
        } catch (_: SecurityException) {
            emptyList()
        }
    }

    fun hasMicPermission(): Boolean =
        ContextCompat.checkSelfPermission(app, Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED

    // ---------------------------------------------------------- connect --

    /** User tapped a PC. [transport]: "wifi" or "bluetooth". */
    @Synchronized
    fun connect(key: String, transport: String) {
        val entry = pcList().firstOrNull { it["key"] == key } ?: return
        userDisconnected = false
        error = null
        muted = false
        val pcId = entry["pcId"] as String?
        val name = entry["name"] as String
        if (status == Status.CONNECTED || status == Status.PENDING) session?.bye("user")
        when (transport) {
            "bluetooth" -> connectBluetooth(entry["bluetooth"] as String? ?: return, name, pcId)
            else -> {
                val address = entry["wifi"] as String? ?: return
                setStatus(Status.CONNECTING, Target(pcId, name, "wifi", address))
                val expected = target
                handler.postDelayed({
                    synchronized(this) {
                        if (status == Status.CONNECTING && target == expected) {
                            error = "O PC não respondeu. Confira se o MicNexus está aberto nele."
                            setStatus(Status.IDLE, null)
                        }
                    }
                }, CONNECT_TIMEOUT_MS)
            }
        }
    }

    @SuppressLint("MissingPermission")
    private fun connectBluetooth(address: String, name: String, pcId: String?) {
        setStatus(Status.CONNECTING, Target(pcId, name, "bluetooth", address))
        val expected = target
        Thread({
            var socket: BluetoothSocket? = null
            try {
                val adapter = bluetoothAdapter() ?: throw IllegalStateException("sem Bluetooth")
                adapter.cancelDiscovery()
                socket = adapter.getRemoteDevice(address).createRfcommSocketToServiceRecord(Protocol.BLUETOOTH_UUID)
                socket.connect()
                val s = socket
                val input = s.inputStream
                val info = JSONObject(String(Protocol.read(input).also { require(it.kind == Protocol.INFO) }.payload))
                synchronized(this) {
                    if (target != expected) {
                        s.close()
                        return@Thread
                    }
                }
                startSession(input, s.outputStream, { s.close() }, "bluetooth", info, address)
            } catch (e: Exception) {
                Log.i(TAG, "bluetooth connect failed: ${e.message}")
                try { socket?.close() } catch (_: Exception) {}
                synchronized(this) {
                    if (target != expected) return@Thread
                    if (status == Status.RECONNECTING) return@Thread // retried by the reconnect timer
                    error = "Não foi possível conectar pelo Bluetooth. O MicNexus está aberto no PC?"
                    setStatus(Status.IDLE, null)
                }
            }
        }, "bt-connect").start()
    }

    // ------------------------------------------------------ PC → phone --

    private fun serverLoop() {
        while (true) {
            try {
                ServerSocket().use { server ->
                    server.reuseAddress = true
                    server.bind(InetSocketAddress(Protocol.PHONE_PORT))
                    while (true) {
                        val socket = server.accept()
                        Thread({ handleIncoming(socket) }, "incoming").start()
                    }
                }
            } catch (e: Exception) {
                Log.w(TAG, "server", e)
                Thread.sleep(2000)
            }
        }
    }

    private fun handleIncoming(socket: Socket) {
        try {
            socket.tcpNoDelay = true
            socket.soTimeout = 5000
            val transport = if (socket.inetAddress.isLoopbackAddress) "usb" else "wifi"
            val input = socket.getInputStream()
            val first = Protocol.read(input)
            if (first.kind != Protocol.INFO) return socket.close()
            val info = JSONObject(String(first.payload))
            val pcId = info.optString("id")
            val refuse = synchronized(this) { decideIncoming(pcId, transport) }
            if (refuse != null) {
                socket.getOutputStream().write(Protocol.encode(Protocol.BYE, JSONObject().put("reason", refuse).toString().toByteArray()))
                socket.close()
                return
            }
            socket.soTimeout = (Session.TIMEOUT_MS + 1000).toInt()
            val address = if (transport == "usb") "USB" else socket.inetAddress.hostAddress ?: ""
            startSession(input, socket.getOutputStream(), { socket.close() }, transport, info, address)
        } catch (e: Exception) {
            Log.i(TAG, "incoming: ${e.message}")
            try { socket.close() } catch (_: Exception) {}
        }
    }

    /** Null to accept the PC's connection, or the BYE reason. */
    private fun decideIncoming(pcId: String, transport: String): String? {
        if (!hasMicPermission()) {
            error = "Permita o uso do microfone para conectar."
            notifyState()
            return "idle"
        }
        val current = session
        return when (status) {
            Status.CONNECTED, Status.PENDING -> when {
                // Same PC over the cable: move to it (lower latency).
                current != null && current.pcId == pcId && transport == "usb" && current.transport != "usb" -> null
                else -> "busy"
            }
            Status.RECONNECTING -> if (target?.pcId == pcId) null else "busy"
            Status.CONNECTING -> if (target?.pcId == pcId && (target?.transport == transport || transport == "usb")) null else "busy"
            Status.IDLE -> when (transport) {
                "usb" -> if (!userDisconnected && (foreground || SystemClock.elapsedRealtime() - usbIntentAt < 15_000)) null else "idle"
                else -> if (currentWant() == pcId) null else "idle"
            }
        }
    }

    // ---------------------------------------------------------- session --

    private fun startSession(
        input: java.io.InputStream,
        output: java.io.OutputStream,
        close: () -> Unit,
        transport: String,
        info: JSONObject,
        address: String,
    ) {
        val pcId = info.optString("id")
        val name = info.optString("name", "PC")
        val s = Session(input, output, close, transport, pcId, name, address, this)
        synchronized(this) {
            session?.let { old ->
                old.replaced = true
                stopCapture()
                old.bye("replaced")
            }
            session = s
            if (status != Status.RECONNECTING && status != Status.CONNECTED) {
                setStatus(Status.CONNECTING, Target(pcId, name, transport, address))
            } else {
                target = Target(pcId, name, transport, address)
            }
        }
        s.start(Session.hello(deviceId, deviceName, Build.MODEL, appVersion(), muted))
    }

    override fun onPending(s: Session) {
        synchronized(this) {
            if (session !== s) return
            setStatus(Status.PENDING, target)
        }
    }

    override fun onWelcome(s: Session, codec: String, rate: Int, frameMs: Int) {
        synchronized(this) {
            if (session !== s) return
            this.codec = codec
            this.rate = rate
            this.frameMs = frameMs
            error = null
            reconnectUntil = 0
            if (connectedAt == 0L) connectedAt = System.currentTimeMillis()
            prefs.edit()
                .putString("lastPcId", s.pcId)
                .putString("lastPcName", s.pcName)
                .putString("lastTransport", s.transport)
                .apply()
            if (s.transport == "bluetooth") {
                prefs.edit().putString("knownBt", knownBluetooth().put(s.pcId, s.address).toString()).apply()
            }
            pcs[s.pcId]?.let { it.name = s.pcName }
            if (!MicService.start(app)) {
                error = "Abra o MicNexus para conectar (o Android não deixa ligar o microfone em segundo plano)."
                s.bye("idle")
                return
            }
            setStatus(Status.CONNECTED, Target(s.pcId, s.pcName, s.transport, s.address))
            if (!muted) startCapture(s)
        }
    }

    private var rate = 48000
    private var frameMs = 10

    private fun startCapture(s: Session) {
        stopCapture()
        capture = MicCapture(
            codec = codec ?: "pcm16",
            rate = rate,
            frameMs = frameMs,
            noiseSuppression = noiseSuppression,
            gain = gain,
            send = { payload -> s.send(Protocol.AUDIO, payload) },
            onLevel = { l ->
                level = l
                listeners.forEach { it.onLevel(l) }
            },
            onError = { msg ->
                synchronized(this) {
                    error = msg
                    notifyState()
                }
            },
        ).also { it.start() }
    }

    private fun stopCapture() {
        capture?.stop()
        capture = null
        level = 0f
        listeners.forEach { it.onLevel(0f) }
    }

    override fun onMuteRequest(s: Session, muted: Boolean) {
        if (session === s) setMuted(muted)
    }

    override fun onStats(s: Session, bufferMs: Int, rttMs: Int) {
        if (session !== s) return
        this.bufferMs = bufferMs
        this.rttMs = rttMs
        listeners.forEach { it.onState() }
    }

    override fun onEnded(s: Session, reason: String) {
        synchronized(this) {
            if (session !== s || s.replaced) return
            session = null
            stopCapture()
            rttMs = -1
            bufferMs = -1
            val wasConnected = status == Status.CONNECTED || status == Status.RECONNECTING
            val dropped = reason == "network" || reason == "timeout"
            if (dropped && wasConnected && !userDisconnected) {
                if (reconnectUntil == 0L) reconnectUntil = SystemClock.elapsedRealtime() + RECONNECT_MS
                setStatus(Status.RECONNECTING, target)
                scheduleReconnect()
                return
            }
            error = when (reason) {
                "rejected" -> "O PC recusou a conexão."
                "busy" -> "O PC já está usando outro celular."
                "version" -> "Atualize o MicNexus no PC e no celular para a mesma versão."
                "user" -> if (userDisconnected) null else "O PC desconectou."
                "local", "replaced", "idle" -> error
                else -> if (wasConnected) "A conexão caiu." else "Não foi possível conectar."
            }
            if (reason == "user" && !userDisconnected) userDisconnected = true // PC chose to stop: don't auto-reconnect
            reconnectUntil = 0
            setStatus(Status.IDLE, null)
        }
    }

    private fun scheduleReconnect() {
        handler.postDelayed({
            synchronized(this) {
                if (status != Status.RECONNECTING) return@postDelayed
                if (SystemClock.elapsedRealtime() > reconnectUntil) {
                    error = "Conexão perdida com ${target?.name ?: "o PC"}."
                    reconnectUntil = 0
                    setStatus(Status.IDLE, null)
                    return@postDelayed
                }
                val t = target
                // Wi‑Fi and USB: the PC comes back to us (want / adb). Bluetooth: we dial again.
                if (t != null && t.transport == "bluetooth" && session == null) {
                    connectBluetoothRetry(t)
                }
                scheduleReconnect()
            }
        }, 3000)
    }

    private var btRetrying = false

    @SuppressLint("MissingPermission")
    private fun connectBluetoothRetry(t: Target) {
        if (btRetrying) return
        btRetrying = true
        Thread({
            try {
                val adapter = bluetoothAdapter() ?: return@Thread
                val socket = adapter.getRemoteDevice(t.address).createRfcommSocketToServiceRecord(Protocol.BLUETOOTH_UUID)
                socket.connect()
                val info = JSONObject(String(Protocol.read(socket.inputStream).payload))
                synchronized(this) {
                    if (status != Status.RECONNECTING) {
                        socket.close()
                        return@Thread
                    }
                }
                startSession(socket.inputStream, socket.outputStream, { socket.close() }, "bluetooth", info, t.address)
            } catch (_: Exception) {
            } finally {
                btRetrying = false
            }
        }, "bt-reconnect").start()
    }

    // ------------------------------------------------------- user actions --

    @Synchronized
    fun disconnect() {
        userDisconnected = true
        reconnectUntil = 0
        val s = session
        session = null
        stopCapture()
        s?.bye("user")
        error = null
        setStatus(Status.IDLE, null)
    }

    @Synchronized
    fun setMuted(value: Boolean) {
        if (muted == value) {
            session?.sendMute(value) // confirm the PC's request
            return
        }
        muted = value
        val s = session
        if (s != null && status == Status.CONNECTED) {
            if (value) stopCapture() else startCapture(s)
        }
        s?.sendMute(value)
        notifyState()
    }

    @Synchronized
    fun clearError() {
        error = null
        notifyState()
    }

    private fun setStatus(next: Status, t: Target?) {
        val old = status
        status = next
        target = t
        if (next == Status.IDLE) {
            codec = null
            connectedAt = 0
            MicService.stop(app)
            if (!foreground) releaseMulticast()
        }
        if (old != next || next == Status.IDLE) Log.i(TAG, "status $old → $next ${t?.name ?: ""}")
        notifyState()
    }

    fun appVersion(): String = try {
        app.packageManager.getPackageInfo(app.packageName, 0).versionName ?: "0"
    } catch (_: Exception) {
        "0"
    }
}
