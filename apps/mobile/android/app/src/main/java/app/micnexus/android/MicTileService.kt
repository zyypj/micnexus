package app.micnexus.android

import android.app.PendingIntent
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.graphics.drawable.Icon
import android.os.Build
import android.service.quicksettings.Tile
import android.service.quicksettings.TileService

/**
 * Quick Settings tile: mute/unmute while connected (no need to open the app),
 * opens MicNexus otherwise.
 */
class MicTileService : TileService() {
    override fun onStartListening() {
        update()
    }

    override fun onClick() {
        if (Engine.status == Engine.Status.CONNECTED) {
            Engine.setMuted(!Engine.muted)
            update()
            return
        }
        val intent = Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            startActivityAndCollapse(PendingIntent.getActivity(this, 0, intent, PendingIntent.FLAG_IMMUTABLE))
        } else {
            @Suppress("DEPRECATION")
            startActivityAndCollapse(intent)
        }
    }

    private fun update() {
        val tile = qsTile ?: return
        val connected = Engine.status == Engine.Status.CONNECTED
        tile.state = when {
            !connected -> Tile.STATE_INACTIVE
            Engine.muted -> Tile.STATE_INACTIVE
            else -> Tile.STATE_ACTIVE
        }
        tile.label = "MicNexus"
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            tile.subtitle = when {
                !connected -> "Desconectado"
                Engine.muted -> "Mutado"
                else -> "Ao vivo"
            }
        }
        tile.icon = Icon.createWithResource(this, if (connected && Engine.muted) R.drawable.ic_stat_mic_off else R.drawable.ic_stat_mic)
        tile.updateTile()
    }

    companion object {
        fun refresh(context: Context) {
            try {
                requestListeningState(context, ComponentName(context, MicTileService::class.java))
            } catch (_: Exception) {
            }
        }
    }
}
