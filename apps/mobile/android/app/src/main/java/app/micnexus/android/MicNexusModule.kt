package app.micnexus.android

import android.content.Intent
import android.net.Uri
import android.provider.Settings
import com.facebook.react.bridge.Arguments
import com.facebook.react.bridge.Promise
import com.facebook.react.bridge.ReactApplicationContext
import com.facebook.react.bridge.ReactContextBaseJavaModule
import com.facebook.react.bridge.ReactMethod
import com.facebook.react.bridge.ReadableMap
import com.facebook.react.bridge.WritableMap
import com.facebook.react.modules.core.DeviceEventManagerModule

/** JS bridge to the [Engine] (src/native/MicNexus.ts). */
class MicNexusModule(private val ctx: ReactApplicationContext) : ReactContextBaseJavaModule(ctx), Engine.Listener {
    private val installer = ApkInstaller(ctx)

    override fun getName() = "MicNexus"

    override fun initialize() {
        super.initialize()
        Engine.init(ctx)
        Engine.addListener(this)
    }

    override fun invalidate() {
        Engine.removeListener(this)
        super.invalidate()
    }

    private fun emit(event: String, data: Any?) {
        if (!ctx.hasActiveReactInstance()) return
        ctx.getJSModule(DeviceEventManagerModule.RCTDeviceEventEmitter::class.java).emit(event, data)
    }

    override fun onState() = emit("MicNexusState", state())
    override fun onPcs() = emit("MicNexusPcs", pcs())

    override fun onLevel(level: Float) {
        if (!Engine.foreground) return
        emit("MicNexusLevel", level.toDouble())
    }

    private fun state(): WritableMap = Arguments.createMap().apply {
        putString("status", Engine.status.name.lowercase())
        val t = Engine.target
        if (t == null) {
            putNull("target")
        } else {
            putMap("target", Arguments.createMap().apply {
                putString("pcId", t.pcId)
                putString("name", t.name)
                putString("transport", t.transport)
                putString("address", t.address)
            })
        }
        putBoolean("muted", Engine.muted)
        putString("codec", Engine.codec)
        if (Engine.rttMs >= 0) putInt("rttMs", Engine.rttMs) else putNull("rttMs")
        if (Engine.bufferMs >= 0) putInt("bufferMs", Engine.bufferMs) else putNull("bufferMs")
        putString("error", Engine.error)
        putDouble("connectedAt", Engine.connectedAt.toDouble())
        putBoolean("bluetoothOn", Engine.bluetoothOn())
        putBoolean("bluetoothPermission", Engine.hasBluetoothPermission())
        putBoolean("micPermission", Engine.hasMicPermission())
        putString("deviceName", Engine.deviceName)
    }

    private fun pcs() = Arguments.createArray().apply {
        for (pc in Engine.pcList()) {
            pushMap(Arguments.createMap().apply {
                putString("key", pc["key"] as String)
                putString("pcId", pc["pcId"] as String?)
                putString("name", pc["name"] as String)
                putString("wifi", pc["wifi"] as String?)
                putString("bluetooth", pc["bluetooth"] as String?)
                putBoolean("busy", pc["busy"] == true)
                putBoolean("last", pc["last"] == true)
            })
        }
    }

    @ReactMethod
    fun getState(promise: Promise) = promise.resolve(state())

    @ReactMethod
    fun getPcs(promise: Promise) = promise.resolve(pcs())

    @ReactMethod
    fun getSettings(promise: Promise) = promise.resolve(Arguments.createMap().apply {
        putBoolean("autoConnect", Engine.autoConnect)
        putBoolean("noiseSuppression", Engine.noiseSuppression)
        putDouble("gain", Engine.gain.toDouble())
        putString("lastPcId", Engine.lastPcId)
    })

    @ReactMethod
    fun setSettings(patch: ReadableMap, promise: Promise) {
        if (patch.hasKey("autoConnect")) Engine.autoConnect = patch.getBoolean("autoConnect")
        if (patch.hasKey("noiseSuppression")) Engine.noiseSuppression = patch.getBoolean("noiseSuppression")
        if (patch.hasKey("gain")) Engine.gain = patch.getDouble("gain").toFloat()
        getSettings(promise)
    }

    @ReactMethod
    fun setForeground(value: Boolean) = Engine.setForeground(value)

    @ReactMethod
    fun connect(key: String, transport: String) = Engine.connect(key, transport)

    @ReactMethod
    fun disconnect() = Engine.disconnect()

    @ReactMethod
    fun setMuted(muted: Boolean) = Engine.setMuted(muted)

    @ReactMethod
    fun clearError() = Engine.clearError()

    /** Re-reads paired devices and permissions (after a permission prompt). */
    @ReactMethod
    fun refresh() {
        onState()
        onPcs()
    }

    @ReactMethod
    fun openBluetoothSettings() {
        ctx.startActivity(Intent(Settings.ACTION_BLUETOOTH_SETTINGS).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
    }

    @ReactMethod
    fun openAppSettings() {
        ctx.startActivity(
            Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:${ctx.packageName}"))
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
        )
    }

    @ReactMethod
    fun getAppVersion(promise: Promise) = promise.resolve(Engine.appVersion())

    /** Resolves "permission" when the user must first allow installing unknown apps. */
    @ReactMethod
    fun installUpdate(url: String, version: String, promise: Promise) {
        if (!installer.canInstall()) {
            installer.openInstallPermissionSettings()
            promise.resolve("permission")
            return
        }
        installer.downloadAndInstall(url, version) { error ->
            emit("MicNexusUpdateError", Arguments.createMap().apply { putString("message", error) })
        }
        promise.resolve("downloading")
    }

    // NativeEventEmitter bookkeeping.
    @ReactMethod
    fun addListener(@Suppress("UNUSED_PARAMETER") event: String) {}

    @ReactMethod
    fun removeListeners(@Suppress("UNUSED_PARAMETER") count: Int) {}
}
