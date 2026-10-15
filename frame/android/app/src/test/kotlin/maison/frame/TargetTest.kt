package maison.frame

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue

class TargetTest {
	@Test
	fun `only HTTPS without credentials is accepted`() {
		assertNotNull(Target.parse("https://maison.example"))
		assertNull(Target.parse("http://maison.example"))
		assertNull(Target.parse("https://user:pass@maison.example"))
		assertNull(Target.parse("https://"))
		assertNull(Target.parse("file:///sdcard/index.html"))
		assertNull(Target.parse(null))
		assertNull(Target.parse("not a url"))
	}

	@Test
	fun `nothing rides in the address it opens`() {
		assertEquals("https://maison.example/", Target.parse("https://maison.example/#token=old")!!.startUrl)
		assertEquals("https://maison.example/salon", Target.parse("https://maison.example/salon")!!.startUrl)
	}

	@Test
	fun `only Maison's own origin loads`() {
		val target = Target.parse("https://maison.example")!!
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
