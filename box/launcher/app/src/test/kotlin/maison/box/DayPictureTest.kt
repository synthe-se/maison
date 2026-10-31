package maison.box

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull

class DayPictureTest {
	private fun feed(width: Int = 7360, height: Int = 4600, artist: String = "Christian Ferrer", license: String = "CC BY 4.0") =
		"""{"image":{"title":"File:Vignes_du_Pic - October 2020.jpg",
		"image":{"width":$width,"height":$height},
		"thumbnail":{"source":"https://thumb.wikimedia.org/a/960px-Vignes.jpg?x=1"},
		"artist":{"text":"$artist"},"license":{"type":"$license"},"unread":1}}"""

	@Test
	fun `a landscape credited picture is taken at the screen's width`() {
		val picture = dayPicture(feed())!!
		assertEquals("https://thumb.wikimedia.org/a/1920px-Vignes.jpg?x=1", picture.url)
		assertEquals("Vignes du Pic - October 2020", picture.caption)
		assertEquals("Christian Ferrer", picture.author)
		assertEquals("CC BY 4.0", picture.license)
		assertEquals("Vignes du Pic - October 2020 · Christian Ferrer · CC BY 4.0", picture.credit)
	}

	@Test
	fun `a portrait is left for another day`() = assertNull(dayPicture(feed(width = 8100, height = 11708)))

	@Test
	fun `no clean credit, no picture`() {
		assertNull(dayPicture(feed(artist = "")))
		assertNull(dayPicture(feed(artist = "NOTE: ".repeat(20))))
		assertNull(dayPicture(feed(license = "")))
	}

	@Test
	fun `a day without a picture is none`() = assertNull(dayPicture("{}"))
}
