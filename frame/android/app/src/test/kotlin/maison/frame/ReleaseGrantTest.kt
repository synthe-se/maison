package maison.frame

import kotlin.test.Test
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class ReleaseGrantTest {
	@Test
	fun `only the secret last issued lifts the kiosk, once`() {
		assertFalse(ReleaseGrant.redeem(null))
		val older = ReleaseGrant.issue()
		val secret = ReleaseGrant.issue()
		assertFalse(ReleaseGrant.redeem(older), "an older secret no longer counts")
		assertFalse(ReleaseGrant.redeem("not hex"))
		assertFalse(ReleaseGrant.redeem(null), "another sender says nothing")
		assertTrue(ReleaseGrant.redeem(secret))
		assertFalse(ReleaseGrant.redeem(secret), "once only")
	}
}
