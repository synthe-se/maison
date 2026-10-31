package maison.box

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import androidx.datastore.preferences.core.edit
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingWorkPolicy
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import java.time.LocalDate
import java.time.format.DateTimeFormatter
import java.util.Locale

private const val FEED = "https://api.wikimedia.org/feed/v1/wikipedia/%s/featured/"
/** Wikimedia answers 403 to a client that does not say who it is. */
private const val USER_AGENT = "MaisonBox/0.1 (https://github.com/uplg/maison) launcher"
private const val TIMEOUT_MS = 15_000
private const val DAYS_BACK = 7L
private const val PICTURE = "wallpaper.jpg"
private const val WORK = "wallpaper"
private val FEED_DAY: DateTimeFormatter = DateTimeFormatter.ofPattern("yyyy/MM/dd")

private fun picture(context: Context) = File(context.filesDir, PICTURE)

/** The kept picture, or none yet: the plain ground then. */
internal suspend fun keptPicture(context: Context): Bitmap? = withContext(Dispatchers.IO) {
	picture(context).takeIf(File::isFile)?.let { BitmapFactory.decodeFile(it.path) }
}

/** Asks for [today]'s picture: when the network is there, retried by WorkManager, once. */
internal fun askPicture(context: Context) {
	val request = OneTimeWorkRequestBuilder<WallpaperWorker>()
		.setConstraints(Constraints(requiredNetworkType = NetworkType.CONNECTED))
		.build()
	WorkManager.getInstance(context).enqueueUniqueWork(WORK, ExistingWorkPolicy.KEEP, request)
}

/** Fetches the latest fitting picture and keeps it; the last one stays when this fails. */
class WallpaperWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
	override suspend fun doWork(): Result = withContext(Dispatchers.IO) {
		runCatching { fetch() }.fold({ Result.success() }, { Result.retry() })
	}

	private suspend fun fetch() {
		val today = LocalDate.now()
		val feed = FEED.format(Locale.getDefault().language)
		val chosen = (0..DAYS_BACK).asSequence()
			.map { today.minusDays(it) }
			.firstNotNullOf { day -> dayPicture(get(feed + day.format(FEED_DAY)).decodeToString()) }
		val kept = picture(applicationContext)
		val fresh = File(kept.path + ".new")
		fresh.writeBytes(get(chosen.url))
		check(fresh.renameTo(kept)) { "the picture was not kept" }
		applicationContext.boxStore.edit {
			it[Keys.PICTURE_DAY] = today.toString()
			it[Keys.PICTURE_CREDIT] = chosen.credit
		}
	}

	private fun get(url: String): ByteArray {
		val connection = URL(url).openConnection() as HttpURLConnection
		connection.connectTimeout = TIMEOUT_MS
		connection.readTimeout = TIMEOUT_MS
		connection.setRequestProperty("User-Agent", USER_AGENT)
		try {
			return connection.inputStream.use { it.readBytes() }
		} finally {
			connection.disconnect()
		}
	}
}
