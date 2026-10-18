package app.micnexus.android

import android.os.SystemClock
import android.util.Log
import org.json.JSONArray
import org.json.JSONObject
import java.io.BufferedInputStream
import java.io.InputStream
import java.io.OutputStream

/**
 * One connection to a PC, after its INFO frame: HELLO → (PENDING) → WELCOME,
 * then audio out, pings, mute both ways, BYE. The transport (TCP or RFCOMM)
 * is just a pair of streams.
 */
class Session(
    input: InputStream,
    private val output: OutputStream,
    private val closeTransport: () -> Unit,
    val transport: String,
    val pcId: String,
    val pcName: String,
    val address: String,
    private val events: Events,
) {
    interface Events {
        fun onPending(s: Session)
        fun onWelcome(s: Session, codec: String, rate: Int, frameMs: Int)
        fun onMuteRequest(s: Session, muted: Boolean)
        fun onStats(s: Session, bufferMs: Int, rttMs: Int)
        /** [reason]: the PC's BYE reason, or "network" / "timeout" / "local". */
        fun onEnded(s: Session, reason: String)
    }

    private val input = BufferedInputStream(input, 16 * 1024)
    @Volatile private var closed = false
    @Volatile private var lastRx = SystemClock.elapsedRealtime()
    @Volatile var rttMs = -1
        private set
    @Volatile var welcomed = false
        private set
    /** Set when another session to the same PC takes over (e.g. Wi‑Fi → cable). */
    @Volatile var replaced = false

    fun start(hello: JSONObject) {
        Thread({ readLoop(hello) }, "session-$transport").start()
    }

    private fun readLoop(hello: JSONObject) {
        var reason = "network"
        try {
            send(Protocol.HELLO, hello.toString().toByteArray())
            Thread({ pingLoop() }, "session-ping").start()
            while (!closed) {
                val f = Protocol.read(input)
                lastRx = SystemClock.elapsedRealtime()
                when (f.kind) {
                    Protocol.PENDING -> events.onPending(this)
                    Protocol.WELCOME -> {
                        val w = JSONObject(String(f.payload))
                        welcomed = true
                        events.onWelcome(this, w.optString("codec", "pcm16"), w.optInt("rate", 48000), w.optInt("frameMs", 10))
                    }
                    Protocol.MUTE -> if (f.payload.isNotEmpty()) events.onMuteRequest(this, f.payload[0].toInt() == 1)
                    Protocol.PONG -> if (f.payload.size >= 8) {
                        rttMs = (SystemClock.elapsedRealtime() - Protocol.readLe64(f.payload)).toInt().coerceAtLeast(0)
                    }
                    Protocol.STATS -> {
                        val st = JSONObject(String(f.payload))
                        events.onStats(this, st.optInt("bufferMs"), rttMs)
                    }
                    Protocol.BYE -> {
                        reason = try { JSONObject(String(f.payload)).optString("reason", "user") } catch (_: Exception) { "user" }
                        break
                    }
                }
            }
        } catch (e: Exception) {
            if (closed) reason = "local" else Log.i(TAG, "session ended: ${e.message}")
        }
        if (SystemClock.elapsedRealtime() - lastRx > TIMEOUT_MS && reason == "network") reason = "timeout"
        close()
        events.onEnded(this, reason)
    }

    private fun pingLoop() {
        while (!closed) {
            try {
                Thread.sleep(1000)
            } catch (_: InterruptedException) {
                break
            }
            if (SystemClock.elapsedRealtime() - lastRx > TIMEOUT_MS) {
                // RFCOMM has no read timeout: unblock the reader ourselves.
                close()
                break
            }
            send(Protocol.PING, Protocol.le64(SystemClock.elapsedRealtime()) + Protocol.le32(maxOf(rttMs, 0)))
        }
    }

    fun send(kind: Int, payload: ByteArray) {
        if (closed) return
        val bytes = Protocol.encode(kind, payload)
        try {
            synchronized(output) {
                output.write(bytes)
                output.flush()
            }
        } catch (e: Exception) {
            close()
        }
    }

    fun sendMute(muted: Boolean) = send(Protocol.MUTE, byteArrayOf(if (muted) 1 else 0))

    fun bye(reason: String) {
        send(Protocol.BYE, JSONObject().put("reason", reason).toString().toByteArray())
        close()
    }

    fun close() {
        if (closed) return
        closed = true
        try { closeTransport() } catch (_: Exception) {}
    }

    companion object {
        private const val TAG = "Session"
        const val TIMEOUT_MS = 6000L

        fun hello(id: String, name: String, model: String, version: String, muted: Boolean): JSONObject {
            val codecs = JSONArray()
            if (MicCapture.opusAvailable) codecs.put("opus")
            codecs.put("pcm16")
            return JSONObject()
                .put("v", Protocol.VERSION)
                .put("id", id)
                .put("name", name)
                .put("model", model)
                .put("version", version)
                .put("codecs", codecs)
                .put("muted", muted)
        }
    }
}
