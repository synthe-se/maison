package maison.box

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull

class StoreTest {
	private val said = House(1_000, 2_000, 3_000, false, "BLUE", null)

	@Test
	fun `what the house said just now shows`() = assertEquals(said, fresh(said, 1_000 + HOUSE_FRESH_MS - 1))

	@Test
	fun `silent for too long, the house is not online`() = assertNull(fresh(said, 1_000 + HOUSE_FRESH_MS))

	@Test
	fun `nothing said, nothing shown`() = assertNull(fresh(null, 1_000))

	@Test
	fun `a clock set back shows nothing rather than an old word`() = assertNull(fresh(said, 999))
}
