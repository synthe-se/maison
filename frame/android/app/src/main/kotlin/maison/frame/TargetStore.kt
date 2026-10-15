package maison.frame

import android.content.Context
import androidx.core.content.edit

/** The provisioned target, in the app's private storage. A service token kept by an older
 * kiosk (before screens were paired) is not read, and goes at the next save. */
class TargetStore(context: Context) {
	private val prefs = context.getSharedPreferences("target", Context.MODE_PRIVATE)

	fun load(): Target? = Target.parse(prefs.getString(URL, null))

	fun save(target: Target) = prefs.edit(commit = true) {
		putString(URL, target.url.toString())
		remove(OLD_TOKEN)
	}

	private companion object {
		const val URL = "url"
		const val OLD_TOKEN = "token"
	}
}
