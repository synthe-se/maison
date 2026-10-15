package maison.frame

/** Waits between tries while Maison cannot be reached: doubling from [first], capped at [cap]. */
class Backoff(private val first: Long = 2_000, private val cap: Long = 60_000) {
	private var next = first

	fun next(): Long = next.also { next = (it * 2).coerceAtMost(cap) }

	fun reset() {
		next = first
	}
}
