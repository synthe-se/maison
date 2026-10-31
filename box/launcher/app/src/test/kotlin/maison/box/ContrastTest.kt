package maison.box

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class ContrastTest {
	private val ink = 0xEAE7E0

	@Test
	fun `the formula reads WCAG's own extremes`() {
		assertEquals(21.0, contrast(0xFFFFFF, 0x000000), 0.001)
		assertEquals(1.0, contrast(ink, ink), 0.001)
	}

	/** WCAG 1.4.3: 4.5:1 for text, F83 counts a picture behind it. The worst picture is white. */
	@Test
	fun `every text holds 4,5 to 1 over a white picture`() {
		assertTrue(contrastOver(ink, SCRIM_LEFT) >= 4.5, "left: ${contrastOver(ink, SCRIM_LEFT)}")
		assertTrue(contrastOver(ink, SCRIM_BOTTOM) >= 4.5, "bottom: ${contrastOver(ink, SCRIM_BOTTOM)}")
	}

	/** WCAG 1.4.11: 3:1 for the focus ring, over the darkest row it can stand on. */
	@Test
	fun `the focus ring holds 3 to 1 on the scrim`() {
		assertTrue(contrast(0x63C0CB, GROUND_RGB) >= 3.0)
		assertTrue(contrastOver(0x63C0CB, SCRIM_BOTTOM) >= 3.0)
	}

	@Test
	fun `the scrim's edge falls smoothly, flat at both ends`() {
		assertEquals(1f, fall(0.1f, 0.3f, 0.8f))
		assertEquals(0f, fall(0.9f, 0.3f, 0.8f))
		assertEquals(0.5f, fall(0.55f, 0.3f, 0.8f), 0.001f)
		assertTrue(fall(0.32f, 0.3f, 0.8f) > 0.99f, "no line where the hold ends")
	}

	@Test
	fun `the applications page holds its text too`() {
		assertTrue(contrastOver(ink, SCRIM_APPS) >= 4.5)
	}
}
