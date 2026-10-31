package maison.box.ui

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.Crossfade
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.focusRestorer
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.repeatOnLifecycle
import androidx.tv.material3.Text
import kotlinx.coroutines.delay
import maison.box.BoxApp
import maison.box.HomeState
import maison.box.House
import maison.box.R
import maison.box.SCRIM_APPS
import maison.box.SCRIM_BOTTOM
import maison.box.SCRIM_LEFT
import maison.box.SCRIM_NIGHT
import maison.box.fall
import maison.box.moved
import maison.box.with
import java.time.Instant
import java.time.LocalDateTime
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import android.text.format.DateFormat as AndroidDateFormat

private const val MS_PER_MINUTE = 60_000L
private const val SCRIM_STEPS = 12
private const val SCRIM_LEFT_HOLD = 0.34f
private const val SCRIM_LEFT_END = 0.86f
private const val SCRIM_BOTTOM_HOLD = 0.34f
private const val SCRIM_BOTTOM_END = 0.78f
private const val PAGE_RISE = 24
internal const val PLUS_TAG = "plus"

private enum class Page { HOME, APPS }

/** What leaves the screen: the system's, Activity's to do. */
internal class HomeActions(
	val open: (BoxApp) -> Unit,
	val settings: () -> Unit,
	val details: (BoxApp) -> Unit,
	val uninstall: (BoxApp) -> Unit,
	val favor: (List<String>) -> Unit,
)

/** Where the focus is asked to go: a new one each time, even for the same tile. */
private class Wish(val packageName: String?)

/**
 * The home, Iris's language over Google's mechanics: a home of favorites and an Applications page
 * of every TV app, a long OK on any of them for its options. On the home, the day's picture, the
 * time and what Maison says of the house while it is online. Each text sits on a scrim that holds
 * its contrast over any picture (Contrast.kt, tested). No feed, no recommendation, no service.
 */
@Composable
internal fun HomeScreen(state: HomeState, actions: HomeActions) {
	var page by remember { mutableStateOf(Page.HOME) }
	var sheet by remember { mutableStateOf<BoxApp?>(null) }
	var moving by remember { mutableStateOf<String?>(null) }
	var wish by remember { mutableStateOf(Wish(null)) }
	var hinted by remember { mutableStateOf(true) }
	var resting by remember { mutableStateOf(false) }
	var wakingKey by remember { mutableStateOf(false) }
	var lastKey by remember { mutableLongStateOf(0L) }
	val homeTab = remember { FocusRequester() }
	val appsTab = remember { FocusRequester() }
	val tab = if (page == Page.HOME) homeTab else appsTab

	// The first tile takes the focus once the apps are known; after that, it stays where it is.
	val firstTile = state.favorites.firstOrNull()
	LaunchedEffect(firstTile) {
		if (wish.packageName == null && firstTile != null) wish = Wish(firstTile)
	}

	// Rest comes to the home alone, nothing open, after a while without a key.
	LaunchedEffect(lastKey, page, sheet, moving) {
		if (page == Page.HOME && sheet == null && moving == null) {
			delay(Measure.REST_AFTER_MS)
			resting = true
		}
	}
	val chrome by animateFloatAsState(if (resting) 0f else 1f, tween(Measure.REST_FADE_MS), label = "chrome")

	// Back undoes what is open; on the home itself it does nothing: there is nowhere to go.
	BackHandler {
		when {
			sheet != null -> {
				wish = Wish(sheet?.packageName)
				sheet = null
			}
			moving != null -> moving = null
			page == Page.APPS -> homeTab.requestFocus()
		}
	}

	Box(
		Modifier
			.fillMaxSize()
			.background(Palette.ground)
			.onPreviewKeyEvent { event ->
				// the key that wakes only wakes, down and up: nothing opens behind the time
				val down = event.type == KeyEventType.KeyDown
				if (down) lastKey++
				val swallowed = resting || wakingKey
				if (down && resting) {
					resting = false
					wakingKey = true
				} else if (!down) {
					wakingKey = false
				}
				swallowed
			},
	) {
		Backdrop(state, page, chrome)
		// Rows and grids run edge to edge, their content padded to the margin: inside the margin
		// they would cut the focus ring and the grown card of their first tile.
		Column(Modifier.fillMaxSize().padding(vertical = Measure.safeY)) {
			Head(state.settingsName, page, chrome, homeTab, appsTab, onTab = {
				page = it
				hinted = false
			}, onSettings = actions.settings, onLeaveTiles = { hinted = false })
			AnimatedContent(
				targetState = page,
				modifier = Modifier.weight(1f).fillMaxWidth(),
				transitionSpec = {
					(fadeIn(tween(Measure.PAGE_MS)) + slideInVertically(tween(Measure.PAGE_MS)) { it / PAGE_RISE }) togetherWith
						fadeOut(tween(Measure.PAGE_MS / 2))
				},
				label = "page",
			) { shown ->
				val tile: @Composable (BoxApp, Modifier) -> Unit = { app, modifier ->
					Tile(
						app = app,
						wish = wish,
						isMoving = moving == app.packageName,
						up = tab,
						modifier = modifier,
						onFocus = { hinted = true },
						onOpen = {
							if (moving != null) moving = null else actions.open(app)
						},
						onOptions = { if (moving == null) sheet = app },
						onMove = { by -> actions.favor(state.favorites.moved(app.packageName, by)) },
					)
				}
				when (shown) {
					Page.HOME -> HomePage(state, wish, tab, resting, chrome, tile, onPlus = { appsTab.requestFocus() }, onPlusFocus = { hinted = false })
					Page.APPS -> AppsPage(state.apps, tile)
				}
			}
			Foot(hinted, moving != null, state.credit.takeIf { page == Page.HOME }, chrome)
		}
		AnimatedVisibility(
			visible = sheet != null,
			modifier = Modifier.align(Alignment.CenterEnd),
			enter = slideInHorizontally(tween(Measure.SHEET_MS)) { it } + fadeIn(tween(Measure.SHEET_MS)),
			exit = slideOutHorizontally(tween(Measure.SHEET_MS)) { it } + fadeOut(tween(Measure.SHEET_MS)),
		) {
			sheet?.let { app ->
				Sheet(app, app.packageName in state.favorites, close = { sheet = null }, actions = actions, onMove = {
					page = Page.HOME
					moving = app.packageName
					wish = Wish(app.packageName)
				}, onChoose = { chosen -> actions.favor(chosen(state.favorites)) }, onReturn = { wish = Wish(app.packageName) })
			}
		}
	}
}

/** The picture, night's veil, the scrims; the plain ground before any picture came. */
@Composable
private fun Backdrop(state: HomeState, page: Page, chrome: Float) {
	val shade by animateFloatAsState(if (page == Page.APPS) SCRIM_APPS else 0f, tween(Measure.PAGE_MS), label = "shade")
	Crossfade(state.wallpaper, animationSpec = tween(Measure.PAGE_MS), label = "wallpaper") { picture: ImageBitmap? ->
		if (picture != null) Image(picture, null, Modifier.fillMaxSize(), contentScale = ContentScale.Crop)
	}
	if (state.house?.night == true) Box(Modifier.fillMaxSize().background(Palette.ground.copy(alpha = SCRIM_NIGHT)))
	Box(Modifier.fillMaxSize().background(Brush.horizontalGradient(*scrim(SCRIM_LEFT, SCRIM_LEFT_HOLD, SCRIM_LEFT_END))))
	// nothing left to read down there at rest: the picture takes the room back
	Box(
		Modifier
			.fillMaxSize()
			.graphicsLayer { alpha = chrome }
			.background(Brush.verticalGradient(*scrim(SCRIM_BOTTOM, SCRIM_BOTTOM_HOLD, SCRIM_BOTTOM_END).reversedStops())),
	)
	Box(Modifier.fillMaxSize().graphicsLayer { alpha = shade }.background(Palette.ground))
}

/** The ground held then falling away softly: no line shows on any picture. */
private fun scrim(alpha: Float, hold: Float, end: Float) =
	Array(SCRIM_STEPS + 1) { step ->
		val at = step.toFloat() / SCRIM_STEPS
		at to Palette.ground.copy(alpha = alpha * fall(at, hold, end))
	}

private fun <T> Array<Pair<Float, T>>.reversedStops() = map { (at, color) -> 1 - at to color }.reversed().toTypedArray()

@Composable
private fun Head(
	settingsName: String,
	page: Page,
	chrome: Float,
	homeTab: FocusRequester,
	appsTab: FocusRequester,
	onTab: (Page) -> Unit,
	onSettings: () -> Unit,
	onLeaveTiles: () -> Unit,
) {
	Row(
		verticalAlignment = Alignment.CenterVertically,
		modifier = Modifier.fillMaxWidth().padding(horizontal = Measure.safeX).graphicsLayer { alpha = chrome },
	) {
		PageTab(stringResource(R.string.tab_home), page == Page.HOME, homeTab) { onTab(Page.HOME) }
		PageTab(stringResource(R.string.tab_apps), page == Page.APPS, appsTab) { onTab(Page.APPS) }
		Spacer(Modifier.weight(1f))
		FocusSurface(onClick = onSettings, shape = CircleShape, rest = Palette.pill, modifier = Modifier.onFocusChanged { if (it.isFocused) onLeaveTiles() }) { focused -> PillWords(settingsName, if (focused) Palette.ground else Palette.ink) }
	}
}

/** A tab: its page shows as the focus lands on it, as Google's and Iris's do. */
@Composable
private fun PageTab(words: String, current: Boolean, requester: FocusRequester, onFocus: () -> Unit) {
	FocusSurface(
		onClick = onFocus,
		shape = CircleShape,
		rest = if (current) Palette.wash else androidx.compose.ui.graphics.Color.Transparent,
		modifier = Modifier
			.padding(end = Measure.s)
			.focusRequester(requester)
			.onFocusChanged { if (it.isFocused) onFocus() },
	) { focused -> PillWords(words, if (focused) Palette.ground else if (current) Palette.accent else Palette.ink) }
}

@Composable
private fun PillWords(words: String, color: androidx.compose.ui.graphics.Color) {
	Box(Modifier.heightIn(min = Measure.pill).padding(horizontal = Measure.l), contentAlignment = Alignment.Center) {
		Text(words, style = Type.control, color = color)
	}
}

@Composable
private fun HomePage(
	state: HomeState,
	wish: Wish,
	up: FocusRequester,
	resting: Boolean,
	chrome: Float,
	tile: @Composable (BoxApp, Modifier) -> Unit,
	onPlus: () -> Unit,
	onPlusFocus: () -> Unit,
) {
	val apps = state.favorites.mapNotNull { name -> state.apps.find { it.packageName == name } }
	val list = rememberLazyListState()
	LaunchedEffect(wish, apps.size) {
		apps.indexOfFirst { it.packageName == wish.packageName }.takeIf { it >= 0 }?.let { list.animateScrollToItem(it) }
	}
	Column(Modifier.fillMaxSize()) {
		Spacer(Modifier.weight(1f))
		Column(Modifier.padding(horizontal = Measure.safeX).offset(y = restingRise(resting))) {
			Clock()
			state.house?.let { house -> Column(Modifier.graphicsLayer { alpha = chrome }) { HouseLines(house) } }
		}
		Spacer(Modifier.weight(1f))
		LazyRow(
			state = list,
			horizontalArrangement = Arrangement.spacedBy(Measure.row),
			contentPadding = PaddingValues(horizontal = Measure.safeX, vertical = Measure.m),
			verticalAlignment = Alignment.Top,
			modifier = Modifier.fillMaxWidth().focusRestorer().graphicsLayer { alpha = chrome },
		) {
			items(apps, key = { it.packageName }) { app -> tile(app, Modifier.animateItem()) }
			if (state.apps.size > apps.size) item(key = PLUS_TAG) { Plus(up, onPlus, onPlusFocus, Modifier.animateItem()) }
		}
	}
}

/** How far up the time sits: it rises as the rest comes, then floats, slowly, while nobody is there. */
@Composable
private fun restingRise(resting: Boolean): Dp {
	val rise by animateDpAsState(
		if (resting) -Measure.restRise else 0.dp,
		tween(if (resting) Measure.REST_RISE_MS else Measure.REST_FADE_MS),
		label = "rise",
	)
	if (!resting) return rise
	val float by rememberInfiniteTransition(label = "float").animateFloat(
		1f,
		-1f,
		infiniteRepeatable(tween(Measure.REST_FLOAT_MS), RepeatMode.Reverse),
		label = "float",
	)
	return rise + Measure.restFloat * float
}

/** The way to Applications: a small soft +, beside the stills, not one of them. */
@Composable
private fun Plus(up: FocusRequester, onClick: () -> Unit, onFocus: () -> Unit, modifier: Modifier) {
	val words = stringResource(R.string.add_app)
	Box(modifier.height(Measure.stillH), contentAlignment = Alignment.Center) {
		FocusSurface(
			onClick = onClick,
			shape = CircleShape,
			modifier = Modifier
				.size(Measure.plus)
				.testTag(PLUS_TAG)
				.semantics { contentDescription = words }
				.focusProperties { this.up = up }
				.onFocusChanged { if (it.isFocused) onFocus() },
		) { focused ->
			Image(
				painterResource(R.drawable.ic_plus),
				null,
				Modifier.size(Measure.xl),
				colorFilter = ColorFilter.tint(if (focused) Palette.ground else Palette.muted),
			)
		}
	}
}

@Composable
private fun AppsPage(apps: List<BoxApp>, tile: @Composable (BoxApp, Modifier) -> Unit) {
	LazyVerticalGrid(
		columns = GridCells.Fixed(Measure.COLUMNS),
		horizontalArrangement = Arrangement.spacedBy(Measure.row),
		verticalArrangement = Arrangement.spacedBy(Measure.row),
		contentPadding = PaddingValues(horizontal = Measure.safeX, vertical = Measure.xl),
		modifier = Modifier.fillMaxSize().focusRestorer(),
	) {
		items(apps, key = { it.packageName }) { app -> tile(app, Modifier.animateItem()) }
	}
}

/** A still and its name. Moving, left and right carry it; OK or Back put it down. */
@Composable
private fun Tile(
	app: BoxApp,
	wish: Wish,
	isMoving: Boolean,
	up: FocusRequester,
	modifier: Modifier,
	onFocus: () -> Unit,
	onOpen: () -> Unit,
	onOptions: () -> Unit,
	onMove: (Int) -> Unit,
) {
	val requester = remember { FocusRequester() }
	LaunchedEffect(wish) {
		if (wish.packageName == app.packageName) requester.requestFocus()
	}
	Column(modifier.width(Measure.stillW)) {
		FocusSurface(
			onClick = onOpen,
			onLongClick = onOptions,
			shape = RoundedCornerShape(Measure.radiusCard),
			rest = Palette.raised,
			focused = Palette.raised,
			scale = Measure.CARD_SCALE,
			modifier = Modifier
				.fillMaxWidth()
				.height(Measure.stillH)
				.focusRequester(requester)
				.focusProperties { this.up = up }
				.onFocusChanged { if (it.isFocused) onFocus() }
				.onPreviewKeyEvent { event ->
					val by = when (event.key) {
						Key.DirectionLeft -> -1
						Key.DirectionRight -> 1
						Key.DirectionUp, Key.DirectionDown -> 0
						else -> null
					}
					when {
						!isMoving || by == null -> false
						else -> {
							if (event.type == KeyEventType.KeyDown && by != 0) onMove(by)
							true
						}
					}
				},
		) {
			Image(
				app.picture,
				app.name,
				Modifier
					.fillMaxSize()
					.border(Measure.hairline, Palette.line, RoundedCornerShape(Measure.radiusCard)),
				contentScale = ContentScale.Crop,
			)
		}
		Text(
			app.name,
			style = Type.name,
			color = Palette.ink,
			maxLines = 1,
			overflow = TextOverflow.Ellipsis,
			modifier = Modifier.padding(top = Measure.l),
		)
	}
}

/** The time, as long as someone looks. */
@Composable
private fun Clock() {
	val lifecycle = LocalLifecycleOwner.current.lifecycle
	var now by remember { mutableStateOf(LocalDateTime.now()) }
	LaunchedEffect(lifecycle) {
		lifecycle.repeatOnLifecycle(Lifecycle.State.RESUMED) {
			while (true) {
				now = LocalDateTime.now()
				delay(MS_PER_MINUTE - System.currentTimeMillis() % MS_PER_MINUTE)
			}
		}
	}
	val locale = LocalConfiguration.current.locales[0]
	val hours = hoursFormat()
	val date = remember(locale) { DateTimeFormatter.ofPattern(AndroidDateFormat.getBestDateTimePattern(locale, "EEEEdMMMM"), locale) }
	Text(now.format(hours), style = Type.clock, color = Palette.ink)
	Text(now.format(date), style = Type.date, color = Palette.ink)
}

/** The time as the box is set to say it, 12 or 24 hours. */
@Composable
private fun hoursFormat(): DateTimeFormatter {
	val context = LocalContext.current
	val locale = LocalConfiguration.current.locales[0]
	val pattern = AndroidDateFormat.getBestDateTimePattern(locale, if (AndroidDateFormat.is24HourFormat(context)) "Hm" else "hm")
	return remember(pattern) { DateTimeFormatter.ofPattern(pattern, locale) }
}

/** What Maison says: gone with it. */
@Composable
private fun HouseLines(house: House) {
	val hours = hoursFormat()
	val time = { ms: Long -> Instant.ofEpochMilli(ms).atZone(ZoneId.systemDefault()).format(hours) }
	Row(Modifier.padding(top = Measure.l), horizontalArrangement = Arrangement.spacedBy(Measure.xl)) {
		house.sunriseMs?.let { SunFact(R.drawable.ic_sunrise, R.string.sunrise, time(it)) }
		house.sunsetMs?.let { SunFact(R.drawable.ic_sunset, R.string.sunset, time(it)) }
	}
	Row(Modifier.padding(top = Measure.s), horizontalArrangement = Arrangement.spacedBy(Measure.xl)) {
		house.tempoToday?.let { TempoFact(R.string.tempo_today, it) }
		house.tempoTomorrow?.let { TempoFact(R.string.tempo_tomorrow, it) }
	}
}

/** Its icon and the time; the word is for whoever cannot see the icon. */
@Composable
private fun SunFact(icon: Int, words: Int, time: String) {
	val said = stringResource(words, time)
	Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.semantics(mergeDescendants = true) { contentDescription = said }) {
		Image(painterResource(icon), null, Modifier.size(Measure.sunIcon), colorFilter = ColorFilter.tint(Palette.ink))
		Text(time, style = Type.house, color = Palette.ink, modifier = Modifier.padding(start = Measure.s))
	}
}

/** A Tempo color, in its word and its dot: never the color alone. */
@Composable
private fun TempoFact(line: Int, color: String) {
	val (word, dot) = when (color) {
		"BLUE" -> R.string.tempo_blue to Palette.tempoBlue
		"WHITE" -> R.string.tempo_white to Palette.tempoWhite
		"RED" -> R.string.tempo_red to Palette.tempoRed
		else -> return
	}
	Row(verticalAlignment = Alignment.CenterVertically) {
		Box(Modifier.size(Measure.dot).background(dot, CircleShape))
		Text(stringResource(line, stringResource(word)), style = Type.house, color = Palette.ink, modifier = Modifier.padding(start = Measure.s))
	}
}

@Composable
private fun Foot(hinted: Boolean, moving: Boolean, credit: String?, chrome: Float) {
	Row(
		Modifier.fillMaxWidth().padding(horizontal = Measure.safeX).padding(top = Measure.xs).heightIn(min = Measure.xl).graphicsLayer { alpha = chrome },
		verticalAlignment = Alignment.CenterVertically,
	) {
		when {
			moving -> Text(stringResource(R.string.hint_move), style = Type.control, color = Palette.ink)
			hinted -> KeyHint()
		}
		Spacer(Modifier.weight(1f))
		credit?.let { Text(it, style = Type.control, color = Palette.ink, maxLines = 1, overflow = TextOverflow.Ellipsis) }
	}
}

/** Iris's key cap and its words: a long OK is found by no one who is not told. */
@Composable
private fun KeyHint() {
	Text(
		stringResource(R.string.hint_hold),
		style = Type.key,
		color = Palette.ink,
		modifier = Modifier
			.border(Measure.hairline, Palette.ink, RoundedCornerShape(Measure.radiusKey))
			.padding(horizontal = Measure.s, vertical = Measure.xs),
	)
	Text(stringResource(R.string.hint_options), style = Type.control, color = Palette.ink, modifier = Modifier.padding(start = Measure.s))
}

/** Google's long OK: a side panel of what can be done with this app; the focus stays in it. */
@Composable
private fun Sheet(
	app: BoxApp,
	favorite: Boolean,
	close: () -> Unit,
	actions: HomeActions,
	onMove: () -> Unit,
	onChoose: ((List<String>) -> List<String>) -> Unit,
	onReturn: () -> Unit,
) {
	val first = remember { FocusRequester() }
	LaunchedEffect(Unit) { first.requestFocus() }
	val act = { then: () -> Unit ->
		close()
		then()
	}
	Column(
		Modifier
			.width(Measure.sheet)
			.fillMaxHeight()
			.background(Palette.ground)
			.padding(horizontal = Measure.xl, vertical = Measure.safeY)
			.focusProperties { onExit = { cancelFocusChange() } }
			.focusGroup(),
		verticalArrangement = Arrangement.spacedBy(Measure.s),
	) {
		Text(app.name, style = Type.sheet, color = Palette.ink, modifier = Modifier.padding(bottom = Measure.l))
		Option(R.string.sheet_open, Modifier.focusRequester(first)) { act { actions.open(app) } }
		if (favorite) {
			Option(R.string.sheet_move) { act(onMove) }
			Option(R.string.sheet_remove) {
				act {
					onChoose { it - app.packageName }
					onReturn()
				}
			}
		} else {
			Option(R.string.sheet_add) {
				act {
					onChoose { it.with(app.packageName) }
					onReturn()
				}
			}
		}
		Option(R.string.sheet_info) { act { actions.details(app) } }
		if (!app.system) Option(R.string.sheet_uninstall) { act { actions.uninstall(app) } }
	}
}

@Composable
private fun Option(words: Int, modifier: Modifier = Modifier, onClick: () -> Unit) {
	FocusSurface(onClick = onClick, shape = RoundedCornerShape(Measure.radiusCard), modifier = modifier.fillMaxWidth()) { focused ->
		Box(Modifier.fillMaxWidth().heightIn(min = Measure.option).padding(horizontal = Measure.l), contentAlignment = Alignment.CenterStart) {
			Text(stringResource(words), style = Type.control, color = if (focused) Palette.ground else Palette.ink)
		}
	}
}
