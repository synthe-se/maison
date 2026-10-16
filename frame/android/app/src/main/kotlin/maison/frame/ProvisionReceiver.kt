package maison.frame

import android.app.Activity
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.util.Log

/**
 * Over ADB only (the receiver requires DUMP, which only the shell holds):
 *
 * ```
 * adb shell am broadcast -a maison.frame.PROVISION -n maison.frame/.ProvisionReceiver \
 *   --es url https://maison.example
 * adb shell am broadcast -a maison.frame.RELEASE -n maison.frame/.ProvisionReceiver
 * ```
 */
class ProvisionReceiver : BroadcastReceiver() {
	override fun onReceive(context: Context, intent: Intent) {
		when (intent.action) {
			ACTION_PROVISION -> {
				val target = Target.parse(intent.getStringExtra("url"))
				if (target == null) {
					Log.w(TAG, "provisioning refused: an HTTPS url")
					resultCode = Activity.RESULT_CANCELED
					return
				}
				TargetStore(context).save(target)
				Log.i(TAG, "provisioned: ${target.url}")
				resultCode = Activity.RESULT_OK
				show(context, MainActivity.ACTION_RELOAD)
			}
			// the activity lifts the kiosk only with the secret granted here
			ACTION_RELEASE -> show(context, MainActivity.ACTION_RELEASE) { putExtra(MainActivity.EXTRA_GRANT, ReleaseGrant.issue()) }
		}
	}

	private fun show(context: Context, action: String, extras: Intent.() -> Unit = {}) =
		context.startActivity(
			Intent(context, MainActivity::class.java).setAction(action).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK).apply(extras),
		)

	private companion object {
		const val TAG = "maison.frame"
		const val ACTION_PROVISION = "maison.frame.PROVISION"
		const val ACTION_RELEASE = "maison.frame.RELEASE"
	}
}
