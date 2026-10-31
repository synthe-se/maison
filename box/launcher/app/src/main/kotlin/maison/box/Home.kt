package maison.box

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.provider.Settings
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.viewModels
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.compose.runtime.getValue
import maison.box.ui.HomeScreen
import maison.box.ui.HomeActions

/** The box's only home. */
class Home : ComponentActivity() {
	private val model: HomeModel by viewModels()

	override fun onCreate(savedInstanceState: Bundle?) {
		super.onCreate(savedInstanceState)
		val actions = HomeActions(
			open = { startActivity(it.opens) },
			settings = { model.settings?.let(::startActivity) },
			details = { startActivity(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, packageUri(it))) },
			uninstall = { startActivity(Intent(Intent.ACTION_DELETE, packageUri(it))) },
			favor = model::favor,
		)
		setContent {
			val state by model.state.collectAsStateWithLifecycle()
			HomeScreen(state, actions)
		}
	}

	override fun onResume() {
		super.onResume()
		model.refresh()
	}

	private fun packageUri(app: BoxApp) = Uri.fromParts("package", app.packageName, null)
}
