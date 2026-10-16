package maison.frame

import java.security.MessageDigest
import java.security.SecureRandom

/**
 * The kiosk's way out, granted by [ProvisionReceiver] (which only the shell may call: DUMP) to the
 * activity it starts, as a one-time secret in the intent: the activity is exported (it is the
 * home), so its word alone would let any app lift the kiosk. Both run in this process: the secret
 * never leaves it.
 */
object ReleaseGrant {
	private val random = SecureRandom()
	private var pending: ByteArray? = null

	/** A new secret; any older one no longer counts. */
	@Synchronized
	fun issue(): String {
		val secret = ByteArray(16).also(random::nextBytes)
		pending = secret
		return secret.joinToString("") { "%02x".format(it) }
	}

	/** True once, for the secret last issued. */
	@Synchronized
	fun redeem(secret: String?): Boolean {
		val expected = pending ?: return false
		val given = secret?.let(::bytes) ?: return false
		if (!MessageDigest.isEqual(given, expected)) return false
		pending = null
		return true
	}

	private fun bytes(hex: String): ByteArray? =
		if (hex.length % 2 != 0) null else hex.chunked(2).map { it.toIntOrNull(16)?.toByte() ?: return null }.toByteArray()
}
