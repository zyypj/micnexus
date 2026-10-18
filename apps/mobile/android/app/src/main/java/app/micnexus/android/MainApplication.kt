package app.micnexus.android

import android.app.Application
import com.facebook.react.PackageList
import com.facebook.react.ReactApplication
import com.facebook.react.ReactHost
import com.facebook.react.ReactNativeApplicationEntryPoint.loadReactNative
import com.facebook.react.defaults.DefaultReactHost.getDefaultReactHost

class MainApplication : Application(), ReactApplication {

  override val reactHost: ReactHost by lazy {
    getDefaultReactHost(
      context = applicationContext,
      packageList =
        PackageList(this).packages.apply {
          add(MicNexusPackage())
        },
    )
  }

  override fun onCreate() {
    super.onCreate()
    // The engine (discovery, server, sessions) lives as long as the process,
    // independent of the UI.
    Engine.init(this)
    loadReactNative(this)
  }
}
