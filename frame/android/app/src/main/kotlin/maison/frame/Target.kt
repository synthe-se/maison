package maison.frame

import java.net.URI

/** Where the frame points: Maison's address, and the service token it drives the house with. */
class Target private constructor(val url: URI, val token: String?) {
	/** The page to open. The token rides in the fragment, which the browser never sends anywhere. */
	val startUrl: String
		get() {
			val page = url.toString().substringBefore('#')
			return if (token == null) page else "$page#token=$token"
		}

	/** Only Maison's own origin loads: same scheme, host and port. */
	fun allows(uri: String): Boolean {
		val other = runCatching { URI(uri) }.getOrNull() ?: return false
		return other.scheme.equals(url.scheme, ignoreCase = true) &&
			other.host.equals(url.host, ignoreCase = true) &&
			port(other) == port(url)
	}

	companion object {
		/** A service token as `maison-backend token create` prints it (backend/src/auth.rs). */
		private val TOKEN = Regex("maison_[A-Za-z0-9_-]{43}")

		/** A target from what provisioning gave, or null: HTTPS only, no credentials in the URL. */
		fun parse(url: String?, token: String?): Target? {
			val uri = url?.let { runCatching { URI(it) }.getOrNull() } ?: return null
			if (!uri.scheme.equals("https", ignoreCase = true) || uri.host.isNullOrEmpty() || uri.userInfo != null) return null
			if (token != null && !TOKEN.matches(token)) return null
			return Target(uri, token)
		}

		private fun port(uri: URI): Int = if (uri.port != -1) uri.port else if (uri.scheme.equals("https", ignoreCase = true)) 443 else 80
	}
}
