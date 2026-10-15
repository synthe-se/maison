package maison.frame

import android.app.Activity
import android.app.admin.DeviceAdminReceiver
import android.app.admin.DevicePolicyManager
import android.content.ComponentName
import android.content.Context

/** The device-admin receiver `dpm set-device-owner maison.frame/.Admin` names. */
class Admin : DeviceAdminReceiver()

/** What the device owner locks: lock task without a prompt, no status bar, no keyguard. A frame not
 * made device owner still works, as an ordinary home app. */
object Kiosk {
	fun enter(activity: Activity) {
		val dpm = activity.getSystemService(DevicePolicyManager::class.java)
		if (!dpm.isDeviceOwnerApp(activity.packageName)) return
		val admin = ComponentName(activity, Admin::class.java)
		dpm.setLockTaskPackages(admin, arrayOf(activity.packageName))
		dpm.setStatusBarDisabled(admin, true)
		dpm.setKeyguardDisabled(admin, true)
		activity.startLockTask()
	}

	/** Gives the frame back: the way out of the kiosk, over ADB (ProvisionReceiver). */
	fun release(context: Context) {
		val dpm = context.getSystemService(DevicePolicyManager::class.java)
		if (!dpm.isDeviceOwnerApp(context.packageName)) return
		val admin = ComponentName(context, Admin::class.java)
		dpm.setStatusBarDisabled(admin, false)
		dpm.setKeyguardDisabled(admin, false)
		dpm.setLockTaskPackages(admin, emptyArray())
		@Suppress("DEPRECATION") // still the only way for an owner to step down below Android 8
		dpm.clearDeviceOwnerApp(context.packageName)
	}
}
