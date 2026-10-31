package maison.box.baselineprofile

import androidx.benchmark.macro.junit4.BaselineProfileRule
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.uiautomator.By
import androidx.test.uiautomator.Until
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/** Cold start, the row of favorites, the Applications page: what a box must not JIT on. */
@RunWith(AndroidJUnit4::class)
class BaselineProfileGenerator {
	@get:Rule
	val rule = BaselineProfileRule()

	@Test
	fun startupHomeAndApps() = rule.collect(packageName = PACKAGE) {
		pressHome()
		startActivityAndWait()
		device.wait(Until.hasObject(By.pkg(PACKAGE).depth(0)), WAIT_MS)
		device.waitForIdle()
		repeat(STEPS) {
			device.pressDPadRight()
			device.waitForIdle()
		}
		repeat(STEPS) { device.pressDPadLeft() }
		device.pressDPadUp()
		device.pressDPadRight()
		device.waitForIdle()
		device.pressDPadDown()
		repeat(STEPS) {
			device.pressDPadRight()
			device.waitForIdle()
		}
	}

	private companion object {
		const val PACKAGE = "maison.box"
		const val WAIT_MS = 10_000L
		const val STEPS = 4
	}
}
