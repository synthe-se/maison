package maison.box.ui

import android.content.Intent
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performKeyInput
import androidx.compose.ui.test.pressKey
import maison.box.BoxApp
import maison.box.HomeState
import maison.box.House
import kotlin.test.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

@OptIn(ExperimentalTestApi::class)
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(qualifiers = "fr-w960dp-h540dp-xhdpi")
class HomeScreenTest {
	@get:Rule
	val compose = createComposeRule()

	private val actions = HomeActions({}, {}, {}, {}, {})

	private fun app(name: String) = BoxApp(name, name, ImageBitmap(16, 9), false, Intent(), 0)

	private fun show(house: House? = null, others: Boolean = true) {
		val iris = app("Iris")
		compose.setContent {
			HomeScreen(HomeState(listOfNotNull(iris, app("Twitch").takeIf { others }), listOf("Iris"), house, null, "Credit", "Paramètres"), actions)
		}
		compose.waitForIdle()
	}

	@Test
	fun `favorites show, the rest waits behind the plus`() {
		show()
		compose.onNodeWithText("Iris").assertIsDisplayed()
		compose.onNodeWithText("Twitch").assertDoesNotExist()
		compose.onNodeWithTag(PLUS_TAG, useUnmergedTree = true).assertIsDisplayed()
	}

	@Test
	fun `nothing left to add, no plus`() {
		show(others = false)
		compose.onNodeWithTag(PLUS_TAG, useUnmergedTree = true).assertDoesNotExist()
	}

	@Test
	fun `no house system, no house line`() {
		show()
		compose.onNodeWithText("Tempo bleu").assertDoesNotExist()
	}

	@Test
	fun `what the house says shows, the sun for whoever cannot see the icon too`() {
		show(House(1, 3_600_000, null, false, "BLUE", "RED"))
		compose.onNodeWithText("Tempo bleu").assertIsDisplayed()
		compose.onNodeWithText("Demain rouge").assertIsDisplayed()
		compose.onNodeWithContentDescription("Lever", substring = true, useUnmergedTree = true).assertExists()
	}

	/** The key that wakes opens nothing: the next one does. */
	@Test
	fun `at rest, the first key only wakes`() {
		val opened = mutableListOf<String>()
		val iris = app("Iris")
		compose.setContent {
			HomeScreen(HomeState(listOf(iris), listOf("Iris")), HomeActions({ opened += it.name }, {}, {}, {}, {}))
		}
		compose.waitForIdle()
		compose.mainClock.autoAdvance = false
		compose.mainClock.advanceTimeBy(Measure.REST_AFTER_MS + Measure.REST_FADE_MS + 100)
		compose.onRoot().performKeyInput { pressKey(Key.DirectionCenter) }
		compose.mainClock.advanceTimeBy(100)
		assertEquals(emptyList(), opened)
		compose.onRoot().performKeyInput { pressKey(Key.DirectionCenter) }
		compose.mainClock.advanceTimeBy(100)
		assertEquals(listOf("Iris"), opened)
	}
}
