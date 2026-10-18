package app.micnexus.android

import android.content.Intent
import android.os.Bundle
import com.facebook.react.ReactActivity
import com.facebook.react.ReactActivityDelegate
import com.facebook.react.defaults.DefaultNewArchitectureEntryPoint.fabricEnabled
import com.facebook.react.defaults.DefaultReactActivityDelegate

class MainActivity : ReactActivity() {

  override fun getMainComponentName(): String = "MicNexus"

  override fun createReactActivityDelegate(): ReactActivityDelegate =
      DefaultReactActivityDelegate(this, mainComponentName, fabricEnabled)

  override fun onCreate(savedInstanceState: Bundle?) {
    super.onCreate(null)
    handle(intent)
  }

  override fun onNewIntent(intent: Intent) {
    super.onNewIntent(intent)
    setIntent(intent)
    handle(intent)
  }

  override fun onResume() {
    super.onResume()
    Engine.setForeground(true)
  }

  override fun onPause() {
    Engine.setForeground(false)
    super.onPause()
  }

  /** The PC opens us over USB with `am start --ez usb true` when the cable is plugged in. */
  private fun handle(intent: Intent?) {
    if (intent?.getBooleanExtra("usb", false) == true) Engine.onUsbIntent()
  }
}
