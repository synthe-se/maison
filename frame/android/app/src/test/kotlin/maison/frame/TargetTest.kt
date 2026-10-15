package maison.frame

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue

class TargetTest {
	private val token = "maison_" + "a".repeat(43)

	@Test
	fun `only HTTPS without credentials is accepted`() {
		assertNotNull(Target.parse("https://maison.example", null))
		assertNull(Target.parse("http://maison.example", null))
		assertNull(Target.parse("https://user:pass@maison.example", null))
		assertNull(Target.parse("https://", null))
		assertNull(Target.parse("file:///sdcard/index.html", null))
		assertNull(Target.parse(null, token))
		assertNull(Target.parse("not a url", null))
	}

	@Test
	fun `a token must look like one the backend prints`() {
		assertNotNull(Target.parse("https://maison.example", token))
		assertNull(Target.parse("https://maison.example", "maison_short"))
		assertNull(Target.parse("https://maison.example", "a".repeat(50)))
		assertNull(Target.parse("https://maison.example", "$token&x"))
	}

	@Test
	fun `the token rides in the fragment, replacing any`() {
		assertEquals("https://maison.example/#token=$token", Target.parse("https://maison.example/#old", token)!!.startUrl)
		assertEquals("https://maison.example/salon", Target.parse("https://maison.example/salon", null)!!.startUrl)
	}

	@Test
	fun `only Maison's own origin loads`() {
		val target = Target.parse("https://maison.example", token)!!
		assertTrue(target.allows("https://maison.example/api/zigbee/lamps"))
		assertTrue(target.allows("https://MAISON.example:443/"))
		assertFalse(target.allows("http://maison.example/"))
		assertFalse(target.allows("https://maison.example:8443/"))
		assertFalse(target.allows("https://maison.example.evil.test/"))
		assertFalse(target.allows("https://evil.test/?https://maison.example"))
		assertFalse(target.allows("about:config"))
		assertFalse(target.allows("data:text/html,hi"))
		assertFalse(target.allows("%%%"))
	}
}
