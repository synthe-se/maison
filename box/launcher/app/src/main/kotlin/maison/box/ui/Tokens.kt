package maison.box.ui

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import maison.box.GROUND_RGB
import maison.box.R

/** The synthe.se palette, dark side, as Iris's TV app says it (ui/theme/Color.kt): flat. */
internal object Palette {
	val ground = Color(GROUND_RGB or (0xFF shl 24))
	val raised = Color(0xFF1E2530)
	val line = Color(0xFF2E3744)
	val ink = Color(0xFFEAE7E0)
	val muted = Color(0xFFA8AEB8)
	val accent = Color(0xFF63C0CB)
	val wash = Color(0xFF1D2A31)
	val pill = Color(0xDB0E1218)
	// Tempo's days, the rabbit's ears (web/static/brand.svg): always said in words beside.
	val tempoBlue = Color(0xFF7FA7E8)
	val tempoWhite = Color(0xFFEAE7E0)
	val tempoRed = Color(0xFFF08A7E)
}

/** Android TV's 960x540 dp canvas and margins; Iris's measures for the rest (ui/theme/Tokens.kt). */
internal object Measure {
	val safeX = 58.dp
	val safeY = 28.dp
	val stillW = 154.dp
	val stillH = 87.dp
	val plus = 40.dp
	val pill = 28.dp
	val option = 34.dp
	val sheet = 340.dp
	val dot = 9.dp
	val sunIcon = 22.dp
	val xs = 4.dp
	val s = 6.dp
	val m = 8.dp
	val l = 12.dp
	val row = 16.dp
	val xl = 22.dp
	val radiusKey = 4.dp
	val radiusCard = 6.dp
	val hairline = 1.dp
	val ring = 2.dp
	val ringOffset = 2.dp
	const val COLUMNS = 5
	const val CARD_SCALE = 1.06f
	const val FOCUS_MS = 150
	const val PAGE_MS = 280
	const val SHEET_MS = 220
	/** At rest: how long without a key, how slowly the rest goes, how far the time rises and floats. */
	const val REST_AFTER_MS = 180_000L
	const val REST_FADE_MS = 1_600
	const val REST_RISE_MS = 9_000
	const val REST_FLOAT_MS = 40_000
	val restRise = 48.dp
	val restFloat = 12.dp
}

private val Fraunces = FontFamily(Font(R.font.fraunces_opsz72_medium, FontWeight.Medium))
private val CalSans = FontFamily(Font(R.font.cal_sans_medium, FontWeight.Medium), Font(R.font.cal_sans_semibold, FontWeight.SemiBold))
private const val TABULAR = "tnum"

private fun text(size: Int, weight: FontWeight = FontWeight.Medium, tabular: Boolean = false) =
	TextStyle(fontFamily = CalSans, fontWeight = weight, fontSize = size.sp, fontFeatureSettings = TABULAR.takeIf { tabular })

internal object Type {
	val clock = TextStyle(fontFamily = Fraunces, fontWeight = FontWeight.Medium, fontSize = 86.sp, lineHeight = 90.sp)
	val sheet = clock.copy(fontSize = 26.sp, lineHeight = 30.sp)
	val date = text(17)
	val house = text(14, tabular = true)
	val name = text(14, FontWeight.SemiBold)
	val control = text(12)
	val key = text(10, FontWeight.SemiBold)
}
