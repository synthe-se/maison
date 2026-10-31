package maison.box

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.longPreferencesKey
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch

/** Past this without a word from Maison, what it said is no longer shown: it is not online. */
internal const val HOUSE_FRESH_MS = 2L * 60 * 60 * 1000

private const val SUNRISE = "sunriseMs"
private const val SUNSET = "sunsetMs"
private const val SIDE = "side"
private const val TEMPO_TODAY = "tempoToday"
private const val TEMPO_TOMORROW = "tempoTomorrow"
private const val NIGHT = "dark"
private const val FAVORITES = "favorites"

internal val Context.boxStore: DataStore<Preferences> by preferencesDataStore("box")

internal object Keys {
	val FAVORITES = stringPreferencesKey("favorites")
	/** The kept picture's day (ISO), and its line. */
	val PICTURE_DAY = stringPreferencesKey("pictureDay")
	val PICTURE_CREDIT = stringPreferencesKey("pictureCredit")
	val HOUSE_AT = longPreferencesKey("houseAtMs")
	val SUNRISE_MS = longPreferencesKey(SUNRISE)
	val SUNSET_MS = longPreferencesKey(SUNSET)
	val HOUSE_SIDE = stringPreferencesKey(SIDE)
	val TEMPO_TODAY_COLOR = stringPreferencesKey(TEMPO_TODAY)
	val TEMPO_TOMORROW_COLOR = stringPreferencesKey(TEMPO_TOMORROW)
}

/** What Maison last said of the house. Each part may be missing: the line then goes. */
internal data class House(
	val atMs: Long,
	val sunriseMs: Long?,
	val sunsetMs: Long?,
	val night: Boolean,
	val tempoToday: String?,
	val tempoTomorrow: String?,
)

/** What is kept, if Maison spoke recently enough at [nowMs]. */
internal fun fresh(house: House?, nowMs: Long) = house?.takeIf { nowMs - it.atMs in 0 until HOUSE_FRESH_MS }

internal fun Preferences.house(): House? {
	val at = this[Keys.HOUSE_AT] ?: return null
	return House(
		at,
		this[Keys.SUNRISE_MS],
		this[Keys.SUNSET_MS],
		this[Keys.HOUSE_SIDE] == NIGHT,
		this[Keys.TEMPO_TODAY_COLOR],
		this[Keys.TEMPO_TOMORROW_COLOR],
	)
}

/**
 * What a house system says over ADB, by `am broadcast` (DUMP in the manifest: the shell's alone,
 * no app can say it instead): any of the sun's times, its side, Tempo's colors, and `favorites`,
 * the home's apps set in one line. Silent, the home simply shows none of it.
 */
class HouseReceiver : BroadcastReceiver() {
	override fun onReceive(context: Context, intent: Intent) {
		val done = goAsync()
		writes.launch {
			try {
				context.boxStore.edit { store ->
					intent.getStringExtra(FAVORITES)?.let { store[Keys.FAVORITES] = it }
					if (intent.getStringExtra(FAVORITES) != null && intent.extras?.size() == 1) return@edit
					store[Keys.HOUSE_AT] = System.currentTimeMillis()
					store.put(Keys.SUNRISE_MS, intent.takeIf { it.hasExtra(SUNRISE) }?.getLongExtra(SUNRISE, 0L))
					store.put(Keys.SUNSET_MS, intent.takeIf { it.hasExtra(SUNSET) }?.getLongExtra(SUNSET, 0L))
					store.put(Keys.HOUSE_SIDE, intent.getStringExtra(SIDE))
					store.put(Keys.TEMPO_TODAY_COLOR, intent.getStringExtra(TEMPO_TODAY))
					store.put(Keys.TEMPO_TOMORROW_COLOR, intent.getStringExtra(TEMPO_TOMORROW))
				}
			} finally {
				done.finish()
			}
		}
	}

	private fun <T> androidx.datastore.preferences.core.MutablePreferences.put(key: Preferences.Key<T>, value: T?) {
		if (value == null) remove(key) else this[key] = value
	}

	private companion object {
		val writes = CoroutineScope(SupervisorJob() + Dispatchers.IO)
	}
}
