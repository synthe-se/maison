package maison.box

import android.app.Application
import androidx.compose.runtime.Immutable
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.datastore.preferences.core.edit
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.time.LocalDate

/** What the home shows. */
@Immutable
internal data class HomeState(
	val apps: List<BoxApp> = emptyList(),
	val favorites: List<String> = emptyList(),
	val house: House? = null,
	val wallpaper: ImageBitmap? = null,
	val credit: String? = null,
	val settingsName: String = "",
)

internal class HomeModel(application: Application) : AndroidViewModel(application) {
	private val context get() = getApplication<Application>()
	private val boxApps = BoxApps(application)
	private val store = application.boxStore
	private val apps = MutableStateFlow(emptyList<BoxApp>())
	private val wallpaper = MutableStateFlow<ImageBitmap?>(null)

	val state: StateFlow<HomeState> = combine(apps, store.data, wallpaper) { apps, saved, picture ->
		HomeState(
			apps,
			favoritesOf(saved[Keys.FAVORITES], apps),
			fresh(saved.house(), System.currentTimeMillis()),
			picture,
			saved[Keys.PICTURE_CREDIT],
			boxApps.settings?.first.orEmpty(),
		)
	}.stateIn(viewModelScope, SharingStarted.Eagerly, HomeState())

	val settings get() = boxApps.settings?.second

	init {
		viewModelScope.launch {
			store.data.map { it[Keys.PICTURE_DAY] }.distinctUntilChanged().collect {
				wallpaper.value = keptPicture(context)?.asImageBitmap()
			}
		}
	}

	/** Each return home: what was installed or removed, Maison's silence, the day's picture. */
	fun refresh() {
		viewModelScope.launch {
			apps.value = withContext(Dispatchers.Default) { boxApps.read() }
			if (store.data.first()[Keys.PICTURE_DAY] != LocalDate.now().toString()) askPicture(context)
		}
	}

	fun favor(chosen: List<String>) {
		viewModelScope.launch { store.edit { it[Keys.FAVORITES] = chosen.saved() } }
	}
}
