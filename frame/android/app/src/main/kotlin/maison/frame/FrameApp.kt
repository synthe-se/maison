package maison.frame

import android.app.Application
import org.mozilla.geckoview.GeckoRuntime
import org.mozilla.geckoview.GeckoRuntimeSettings

class FrameApp : Application() {
	/** One Gecko per process, made on first use (Gecko's own child processes never touch it). */
	val runtime: GeckoRuntime by lazy {
		GeckoRuntime.create(
			this,
			GeckoRuntimeSettings.Builder()
				// about:debugging from desktop Firefox over ADB, on debug builds only.
				.remoteDebuggingEnabled(BuildConfig.DEBUG)
				.consoleOutput(BuildConfig.DEBUG)
				.aboutConfigEnabled(false)
				// The house speaks French: Maison picks its language from the browser's.
				.locales(arrayOf("fr-FR", "fr"))
				.build(),
		)
	}
}
