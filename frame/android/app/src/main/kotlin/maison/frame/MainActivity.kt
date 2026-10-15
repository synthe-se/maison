package maison.frame

import android.annotation.SuppressLint
import android.app.Activity
import android.content.Intent
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.view.KeyEvent
import android.view.View
import android.view.WindowManager
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import org.mozilla.geckoview.AllowOrDeny
import org.mozilla.geckoview.GeckoResult
import org.mozilla.geckoview.GeckoSession
import org.mozilla.geckoview.GeckoSession.NavigationDelegate.LoadRequest
import org.mozilla.geckoview.GeckoSessionSettings
import org.mozilla.geckoview.GeckoView
import org.mozilla.geckoview.WebRequestError

/** Maison, full screen, and nothing else: no native UI, the web app is the whole interface. */
class MainActivity : Activity() {
	private lateinit var view: GeckoView
	private var session: GeckoSession? = null
	private var target: Target? = null
	private var canGoBack = false
	private val handler = Handler(Looper.getMainLooper())
	private val backoff = Backoff()
	private val retry = Runnable { target?.let { session?.loadUri(it.startUrl) } }

	override fun onCreate(savedInstanceState: Bundle?) {
		super.onCreate(savedInstanceState)
		window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
		view = GeckoView(this)
		setContentView(view)
		open()
	}

	override fun onNewIntent(intent: Intent) {
		super.onNewIntent(intent)
		setIntent(intent)
		when (intent.action) {
			ACTION_RELOAD -> open()
			ACTION_RELEASE -> {
				stopLockTask()
				Kiosk.release(this)
			}
		}
	}

	override fun onResume() {
		super.onResume()
		hideSystemBars()
		Kiosk.enter(this)
	}

	override fun onWindowFocusChanged(hasFocus: Boolean) {
		super.onWindowFocusChanged(hasFocus)
		if (hasFocus) hideSystemBars()
	}

	// Android 6 has no predictive back: the key walks back inside Maison, never out of it.
	@SuppressLint("GestureBackNavigation")
	override fun onKeyDown(keyCode: Int, event: KeyEvent): Boolean {
		if (keyCode != KeyEvent.KEYCODE_BACK) return super.onKeyDown(keyCode, event)
		if (canGoBack) session?.goBack()
		return true
	}

	override fun onDestroy() {
		handler.removeCallbacks(retry)
		session?.close()
		super.onDestroy()
	}

	/** (Re)opens Maison from the stored target. Until a page has loaded (unprovisioned, or no
	 * network yet: at boot the clock says 1970 until NTP), the frame shows the brand colour, the
	 * window's background: an empty GeckoView paints white. */
	private fun open() {
		handler.removeCallbacks(retry)
		session?.close()
		session = null
		view.visibility = View.INVISIBLE
		val target = TargetStore(this).load() ?: return
		this.target = target
		val session = GeckoSession(GeckoSessionSettings.Builder().usePrivateMode(false).build())
		session.navigationDelegate = Navigation(target)
		session.progressDelegate = Progress()
		session.contentDelegate = Content()
		session.open((application as FrameApp).runtime)
		view.setSession(session)
		this.session = session
		session.loadUri(target.startUrl)
	}

	private fun hideSystemBars() {
		WindowCompat.getInsetsController(window, window.decorView).run {
			systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
			hide(WindowInsetsCompat.Type.systemBars())
		}
	}

	private fun retryLater() {
		handler.removeCallbacks(retry)
		handler.postDelayed(retry, backoff.next())
	}

	private inner class Navigation(private val target: Target) : GeckoSession.NavigationDelegate {
		override fun onLoadRequest(session: GeckoSession, request: LoadRequest): GeckoResult<AllowOrDeny> =
			GeckoResult.fromValue(if (target.allows(request.uri)) AllowOrDeny.ALLOW else AllowOrDeny.DENY)

		// No popups, no new windows: a link elsewhere simply does nothing.
		override fun onNewSession(session: GeckoSession, uri: String): GeckoResult<GeckoSession>? = null

		override fun onCanGoBack(session: GeckoSession, canGoBack: Boolean) {
			this@MainActivity.canGoBack = canGoBack
		}

		override fun onLoadError(session: GeckoSession, uri: String?, error: WebRequestError): GeckoResult<String>? {
			view.visibility = View.INVISIBLE
			retryLater()
			return null
		}
	}

	private inner class Progress : GeckoSession.ProgressDelegate {
		override fun onPageStop(session: GeckoSession, success: Boolean) {
			if (!success) return
			backoff.reset()
			view.visibility = View.VISIBLE
		}
	}

	// A content process that crashed or was killed for memory: start over with a fresh session.
	private inner class Content : GeckoSession.ContentDelegate {
		override fun onCrash(session: GeckoSession) {
			handler.post { open() }
		}

		override fun onKill(session: GeckoSession) {
			handler.post { open() }
		}
	}

	companion object {
		const val ACTION_RELOAD = "maison.frame.RELOAD"
		const val ACTION_RELEASE = "maison.frame.RELEASE_KIOSK"
	}
}
