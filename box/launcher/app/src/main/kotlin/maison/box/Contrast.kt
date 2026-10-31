package maison.box

import kotlin.math.pow

/** The ground, as Iris's palette says it (ui/Tokens.kt draws with it). */
internal const val GROUND_RGB = 0x151A21

/** How much of the ground lies over the picture behind each text: see [contrastOver]. */
internal const val SCRIM_LEFT = 0.86f
internal const val SCRIM_BOTTOM = 0.92f
internal const val SCRIM_NIGHT = 0.35f
internal const val SCRIM_APPS = 0.9f

private const val SRGB_KNEE = 0.03928
private const val SRGB_LINEAR = 12.92
private const val SRGB_OFFSET = 0.055
private const val SRGB_SCALE = 1.055
private const val SRGB_GAMMA = 2.4
private const val WEIGHT_R = 0.2126
private const val WEIGHT_G = 0.7152
private const val WEIGHT_B = 0.0722
private const val FLARE = 0.05

private fun channel(rgb: Int, shift: Int) = (rgb shr shift and 0xFF) / 255.0

private fun linear(value: Double) =
	if (value <= SRGB_KNEE) value / SRGB_LINEAR else ((value + SRGB_OFFSET) / SRGB_SCALE).pow(SRGB_GAMMA)

/** WCAG 2.2's relative luminance. */
internal fun luminance(rgb: Int) =
	WEIGHT_R * linear(channel(rgb, 16)) + WEIGHT_G * linear(channel(rgb, 8)) + WEIGHT_B * linear(channel(rgb, 0))

/** WCAG 2.2's contrast ratio between two colors. */
internal fun contrast(a: Int, b: Int): Double {
	val (light, dark) = listOf(luminance(a), luminance(b)).sortedDescending()
	return (light + FLARE) / (dark + FLARE)
}

/** A soft edge between 1 at [hold] and 0 at [end]: a straight fall shows as a line on a picture. */
internal fun fall(at: Float, hold: Float, end: Float): Float {
	val t = ((at - hold) / (end - hold)).coerceIn(0f, 1f)
	return 1 - t * t * (3 - 2 * t)
}

/** [scrim] of the ground over [picture]. */
internal fun under(scrim: Float, picture: Int): Int {
	fun mix(shift: Int) =
		(channel(GROUND_RGB, shift) * scrim + channel(picture, shift) * (1 - scrim)).times(255).toInt() shl shift
	return mix(16) or mix(8) or mix(0)
}

/** [ink]'s contrast over the worst picture there is, all white, under [scrim]. */
internal fun contrastOver(ink: Int, scrim: Float) = contrast(ink, under(scrim, 0xFFFFFF))
