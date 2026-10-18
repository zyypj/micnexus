package app.micnexus.android

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.wifi.WifiManager
import android.os.Build
import android.os.IBinder
import android.os.PowerManager
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat

/**
 * Foreground service (type microphone) that exists only while a PC is
 * connected: keeps the microphone allowed with the app in the background,
 * holds the CPU/Wi‑Fi awake for low latency and shows the ongoing
 * notification with "Mutar" and "Desconectar".
 */
class MicService : Service() {
    private var wakeLock: PowerManager.WakeLock? = null
    private var wifiLock: WifiManager.WifiLock? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        running = true
        wakeLock = getSystemService(PowerManager::class.java)
            .newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "micnexus:mic").apply {
                setReferenceCounted(false)
                acquire()
            }
        val mode = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            WifiManager.WIFI_MODE_FULL_LOW_LATENCY
        } else {
            @Suppress("DEPRECATION")
            WifiManager.WIFI_MODE_FULL_HIGH_PERF
        }
        wifiLock = getSystemService(WifiManager::class.java).createWifiLock(mode, "micnexus:wifi").apply {
            setReferenceCounted(false)
            acquire()
        }
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_TOGGLE_MUTE -> Engine.setMuted(!Engine.muted)
            ACTION_DISCONNECT -> Engine.disconnect()
        }
        if (Engine.status == Engine.Status.IDLE) {
            stop()
            return START_NOT_STICKY
        }
        try {
            ServiceCompat.startForeground(
                this,
                NOTIFICATION_ID,
                build(this),
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE else 0,
            )
        } catch (e: Exception) {
            Log.w(TAG, "startForeground", e)
            stop()
        }
        return START_NOT_STICKY
    }

    private fun stop() {
        ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE)
        stopSelf()
    }

    override fun onDestroy() {
        running = false
        try { wakeLock?.release() } catch (_: Exception) {}
        try { wifiLock?.release() } catch (_: Exception) {}
        super.onDestroy()
    }

    /** Swiping the app away from recents keeps the microphone going (it is a service). */
    override fun onTaskRemoved(rootIntent: Intent?) {}

    companion object {
        private const val TAG = "MicService"
        private const val CHANNEL = "connection"
        private const val NOTIFICATION_ID = 1
        const val ACTION_TOGGLE_MUTE = "app.micnexus.android.TOGGLE_MUTE"
        const val ACTION_DISCONNECT = "app.micnexus.android.DISCONNECT"

        @Volatile private var running = false

        /** Starts the service; false when Android refuses (app in the background). */
        fun start(context: Context): Boolean {
            ensureChannel(context)
            return try {
                ContextCompat.startForegroundService(context, Intent(context, MicService::class.java))
                true
            } catch (e: Exception) {
                Log.w(TAG, "cannot start", e)
                false
            }
        }

        /** Updates the notification (state changed). */
        fun refresh(context: Context) {
            if (!running || Engine.status == Engine.Status.IDLE) return
            context.getSystemService(NotificationManager::class.java).notify(NOTIFICATION_ID, build(context))
        }

        fun stop(context: Context) {
            if (!running) return
            context.stopService(Intent(context, MicService::class.java))
        }

        private fun ensureChannel(context: Context) {
            if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
            val nm = context.getSystemService(NotificationManager::class.java)
            if (nm.getNotificationChannel(CHANNEL) != null) return
            nm.createNotificationChannel(
                NotificationChannel(CHANNEL, "Conexão com o PC", NotificationManager.IMPORTANCE_LOW).apply {
                    description = "Mostra que o microfone está sendo usado pelo PC, com mutar e desconectar."
                    setShowBadge(false)
                },
            )
        }

        private fun action(context: Context, action: String): PendingIntent =
            PendingIntent.getService(
                context,
                action.hashCode(),
                Intent(context, MicService::class.java).setAction(action),
                PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
            )

        private fun build(context: Context): Notification {
            ensureChannel(context)
            val t = Engine.target
            val pc = t?.name ?: "PC"
            val via = when (t?.transport) {
                "usb" -> "cabo USB"
                "bluetooth" -> "Bluetooth"
                else -> "Wi‑Fi"
            }
            val muted = Engine.muted
            val (title, text) = when (Engine.status) {
                Engine.Status.RECONNECTING -> "Reconectando…" to "Procurando $pc pelo $via"
                Engine.Status.PENDING -> "Aguardando o PC" to "Permita a conexão no MicNexus em $pc"
                Engine.Status.CONNECTING -> "Conectando…" to "$pc pelo $via"
                else -> (if (muted) "Microfone mutado" else "Microfone ativo") to "$pc · $via"
            }
            val open = PendingIntent.getActivity(
                context,
                0,
                Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
                PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
            )
            val b = NotificationCompat.Builder(context, CHANNEL)
                .setSmallIcon(if (muted) R.drawable.ic_stat_mic_off else R.drawable.ic_stat_mic)
                .setColor(ContextCompat.getColor(context, if (muted) R.color.nx_danger else R.color.nx_accent))
                .setContentTitle(title)
                .setContentText(text)
                .setContentIntent(open)
                .setOngoing(true)
                .setOnlyAlertOnce(true)
                .setSilent(true)
                .setCategory(NotificationCompat.CATEGORY_SERVICE)
                .setVisibility(NotificationCompat.VISIBILITY_PUBLIC)
                .setForegroundServiceBehavior(NotificationCompat.FOREGROUND_SERVICE_IMMEDIATE)
            if (Engine.status == Engine.Status.CONNECTED && Engine.connectedAt > 0) {
                b.setWhen(Engine.connectedAt).setShowWhen(true).setUsesChronometer(true)
            } else {
                b.setShowWhen(false)
            }
            if (Engine.status == Engine.Status.CONNECTED) {
                b.addAction(
                    if (muted) R.drawable.ic_stat_mic else R.drawable.ic_stat_mic_off,
                    if (muted) "Desmutar" else "Mutar",
                    action(context, ACTION_TOGGLE_MUTE),
                )
            }
            b.addAction(R.drawable.ic_stat_power, "Desconectar", action(context, ACTION_DISCONNECT))
            return b.build()
        }
    }
}
