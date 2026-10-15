package maison.frame

import java.net.URI

/** Where the frame points: Maison's address. The frame is in as a paired wall screen, by the
 * HttpOnly cookie Maison set once an admin paired it (docs/passkeys.md « Wall screens »): no
 * token here, nothing to provision but the address. */
class Target private constructor(val url: URI) {
	/** The page to open, without a fragment: nothing rides in the address. */
	val startUrl: String
		get() = url.toString().substringBefore('#')

	/** Only Maison's own origin loads: same scheme, host and port. */
	fun allows(uri: String): Boolean {
		val other = runCatching { URI(uri) }.getOrNull() ?: return false
		return other.scheme.equals(url.scheme, ignoreCase = true) &&
			other.host.equals(url.host, ignoreCase = true) &&
			port(other) == port(url)
	}

	companion object {
		/** A target from what provisioning gave, or null: HTTPS only, no credentials in the URL. */
		fun parse(url: String?): Target? {
			val uri = url?.let { runCatching { URI(it) }.getOrNull() } ?: return null
			if (!uri.scheme.equals("https", ignoreCase = true) || uri.host.isNullOrEmpty() || uri.userInfo != null) return null
			return Target(uri)
		}

		private fun port(uri: URI): Int = if (uri.port != -1) uri.port else if (uri.scheme.equals("https", ignoreCase = true)) 443 else 80
	}
}
