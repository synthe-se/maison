package maison.frame

import android.content.Context
import androidx.core.content.edit

/** The provisioned target, in the app's private storage (the token never reaches the log). */
class TargetStore(context: Context) {
	private val prefs = context.getSharedPreferences("target", Context.MODE_PRIVATE)

	fun load(): Target? = Target.parse(prefs.getString(URL, null), prefs.getString(TOKEN, null))

	fun save(target: Target) = prefs.edit(commit = true) {
		putString(URL, target.url.toString())
		putString(TOKEN, target.token)
	}

	private companion object {
		const val URL = "url"
		const val TOKEN = "token"
	}
}
