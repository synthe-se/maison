package maison.frame

/**
 * When to try Maison again while it cannot be reached: after a failure, once [backoff]'s wait is
 * over; at once when a network comes up (the Wi-Fi back after a cut), not up to a minute later.
 * [schedule] (re)places the one pending try, `delay` ms from now.
 */
class Retries(private val backoff: Backoff, private val schedule: (delay: Long) -> Unit) {
	private var waiting = false

	/** A load failed: tried again after the next wait. */
	fun failed() {
		waiting = true
		schedule(backoff.next())
	}

	/** A page loaded: the waits start over. */
	fun loaded() {
		waiting = false
		backoff.reset()
	}

	/** A network is available: a load that failed is tried again now, the waits starting over. */
	fun networkAvailable() {
		if (!waiting) return
		backoff.reset()
		schedule(0)
	}
}
