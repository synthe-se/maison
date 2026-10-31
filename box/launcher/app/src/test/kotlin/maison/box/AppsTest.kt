package maison.box

import android.content.Intent
import androidx.compose.ui.graphics.ImageBitmap
import kotlin.test.Test
import kotlin.test.assertEquals
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.GraphicsMode

@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class AppsTest {
	private fun app(name: String, system: Boolean = false) = BoxApp(name, name, ImageBitmap(1, 1), system, Intent(), 0)

	@Test
	fun `before any choice, the home starts with what the person installed`() {
		val apps = listOf(app("store", system = true)) + (1..7).map { app("app$it") }
		assertEquals((1..5).map { "app$it" }, favoritesOf(null, apps))
	}

	@Test
	fun `a saved choice is kept, even an empty one`() {
		val apps = listOf(app("a"), app("b"))
		assertEquals(listOf("b"), favoritesOf("b", apps))
		assertEquals(emptyList(), favoritesOf("", apps))
	}

	@Test
	fun `a removed app leaves no hole`() {
		assertEquals(listOf("a"), favoritesOf("gone,a", listOf(app("a"))))
	}

	@Test
	fun `favorites survive being saved`() {
		assertEquals(listOf("a", "b"), favoritesOf(listOf("a", "b").saved(), listOf(app("a"), app("b"))))
	}

	@Test
	fun `an app is added once, at the end`() {
		assertEquals(listOf("a", "b"), listOf("a").with("b"))
		assertEquals(listOf("a"), listOf("a").with("a"))
	}

	@Test
	fun `moving stops at the ends`() {
		val row = listOf("a", "b", "c")
		assertEquals(listOf("b", "a", "c"), row.moved("a", 1))
		assertEquals(listOf("a", "c", "b"), row.moved("c", -1))
		assertEquals(row, row.moved("a", -1))
		assertEquals(row, row.moved("c", 1))
		assertEquals(row, row.moved("missing", 1))
	}
}
