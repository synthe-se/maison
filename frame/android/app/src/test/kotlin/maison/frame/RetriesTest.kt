package maison.frame

import kotlin.test.Test
import kotlin.test.assertEquals

class RetriesTest {
	private val scheduled = mutableListOf<Long>()
	private val retries = Retries(Backoff(first = 2_000, cap = 60_000)) { scheduled += it }

	@Test
	fun `a failure waits longer each time, a network back tries at once`() {
		retries.failed()
		retries.failed()
		retries.failed()
		assertEquals(listOf(2_000L, 4_000L, 8_000L), scheduled)
		retries.networkAvailable()
		assertEquals(0L, scheduled.last())
		// the waits start over after that try
		retries.failed()
		assertEquals(2_000L, scheduled.last())
	}

	@Test
	fun `a network that comes while Maison shows changes nothing`() {
		retries.networkAvailable()
		retries.failed()
		retries.loaded()
		retries.networkAvailable()
		assertEquals(listOf(2_000L), scheduled)
		// and after a load, the next failure waits the first wait again
		retries.failed()
		assertEquals(2_000L, scheduled.last())
	}
}
