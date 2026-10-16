package maison.frame

import kotlin.test.Test
import kotlin.test.assertEquals

class BackoffTest {
	@Test
	fun `doubles up to the cap, then starts over after a success`() {
		val backoff = Backoff(first = 2_000, cap = 10_000)
		assertEquals(listOf(2_000L, 4_000L, 8_000L, 10_000L, 10_000L), List(5) { backoff.next() })
		backoff.reset()
		assertEquals(2_000L, backoff.next())
	}
}
