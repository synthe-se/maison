package maison.box

import android.content.Context
import android.content.Intent
import android.content.pm.ApplicationInfo
import android.content.pm.PackageManager
import android.os.Build
import android.provider.Settings
import android.util.DisplayMetrics
import androidx.compose.runtime.Immutable
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.core.graphics.drawable.toBitmap
import java.util.Locale

/** An app of the box, as its tiles show it. */
@Immutable
internal data class BoxApp(
	val packageName: String,
	val name: String,
	val picture: ImageBitmap,
	/** Part of the system: it cannot go. */
	val system: Boolean,
	val opens: Intent,
	val updatedAt: Long,
)

/** The box's TV apps, read from the system; a picture already made is kept while the app is the same. */
internal class BoxApps(private val context: Context) {
	private val packages = context.packageManager
	private val made = mutableMapOf<String, BoxApp>()

	/** The settings' name, and the way there. */
	val settings: Pair<String, Intent>? by lazy {
		val intent = Intent(Settings.ACTION_SETTINGS)
		intent.resolveActivity(packages)?.let { packages.applicationName(it.packageName) to intent }
	}

	fun read(): List<BoxApp> {
		val tv = Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LEANBACK_LAUNCHER)
		val settings = Intent(Settings.ACTION_SETTINGS).resolveActivity(packages)?.packageName
		return packages.queryActivities(tv)
			.map { it.activityInfo.applicationInfo }
			.distinctBy { it.packageName }
			.filter { it.packageName != context.packageName && it.packageName != settings }
			.mapNotNull(::app)
			.sortedBy { it.name.lowercase(Locale.getDefault()) }
	}

	private fun app(info: ApplicationInfo): BoxApp? {
		val updatedAt = packages.lastUpdate(info.packageName)
		made[info.packageName]?.takeIf { it.updatedAt == updatedAt }?.let { return it }
		val opens = packages.getLeanbackLaunchIntentForPackage(info.packageName) ?: return null
		return BoxApp(
			info.packageName,
			packages.getApplicationLabel(info).toString(),
			picture(info),
			info.flags and ApplicationInfo.FLAG_SYSTEM != 0,
			opens,
			updatedAt,
		).also { made[info.packageName] = it }
	}

	/** Its banner at the largest density it has, else its icon. */
	private fun picture(info: ApplicationInfo): ImageBitmap {
		val banner = info.banner.takeIf { it != 0 }?.let {
			packages.getResourcesForApplication(info).getDrawableForDensity(it, DisplayMetrics.DENSITY_XXXHIGH, null)
		}
		return (banner ?: packages.getApplicationIcon(info)).toBitmap().asImageBitmap()
	}

	private fun PackageManager.applicationName(name: String) = getApplicationLabel(applicationInfo(name)).toString()
}

// The flag-less forms went deprecated with Android 13: one place keeps both.
private val TIRAMISU = Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU

@Suppress("DEPRECATION")
private fun PackageManager.queryActivities(intent: Intent) =
	if (TIRAMISU) queryIntentActivities(intent, PackageManager.ResolveInfoFlags.of(0)) else queryIntentActivities(intent, 0)

@Suppress("DEPRECATION")
private fun PackageManager.lastUpdate(name: String) =
	(if (TIRAMISU) getPackageInfo(name, PackageManager.PackageInfoFlags.of(0)) else getPackageInfo(name, 0)).lastUpdateTime

@Suppress("DEPRECATION")
private fun PackageManager.applicationInfo(name: String) =
	if (TIRAMISU) getApplicationInfo(name, PackageManager.ApplicationInfoFlags.of(0)) else getApplicationInfo(name, 0)
