package maison.box.ui

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.drawOutline
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.zIndex
import androidx.tv.material3.ClickableSurfaceDefaults
import androidx.tv.material3.Surface

/** The accent ring, off the element and following [shape]. */
private fun Modifier.focusRing(visible: Boolean, shape: Shape) = drawWithContent {
	drawContent()
	if (!visible) return@drawWithContent
	val width = Measure.ring.toPx()
	val inset = Measure.ringOffset.toPx() + width / 2f
	val outline = shape.createOutline(Size(size.width + inset * 2, size.height + inset * 2), layoutDirection, this)
	translate(-inset, -inset) { drawOutline(outline, Palette.accent, style = Stroke(width)) }
}

/**
 * Iris's one focus treatment on tv-material's clickable Surface: the ring, an optional scale, the
 * focused fill, a long OK when there is one. [content] is told whether it has the focus.
 */
@Composable
internal fun FocusSurface(
	onClick: () -> Unit,
	shape: Shape,
	modifier: Modifier = Modifier,
	onLongClick: (() -> Unit)? = null,
	rest: Color = Color.Transparent,
	focused: Color = Palette.ink,
	scale: Float = 1f,
	content: @Composable (focused: Boolean) -> Unit,
) {
	val source = remember { MutableInteractionSource() }
	val isFocused by source.collectIsFocusedAsState()
	val grown by animateFloatAsState(if (isFocused) scale else 1f, tween(Measure.FOCUS_MS), label = "focus")
	Box(
		modifier
			.zIndex(if (isFocused) 1f else 0f)
			.graphicsLayer {
				scaleX = grown
				scaleY = grown
			}
			.focusRing(isFocused, shape),
		contentAlignment = Alignment.Center,
		propagateMinConstraints = true,
	) {
		Surface(
			onClick = onClick,
			onLongClick = onLongClick,
			shape = ClickableSurfaceDefaults.shape(shape),
			colors = ClickableSurfaceDefaults.colors(
				containerColor = rest,
				contentColor = Palette.ink,
				focusedContainerColor = focused,
				focusedContentColor = Palette.ground,
				pressedContainerColor = focused,
				pressedContentColor = Palette.ground,
			),
			scale = ClickableSurfaceDefaults.scale(focusedScale = 1f, pressedScale = 1f),
			interactionSource = source,
		) {
			Box(Modifier.align(Alignment.Center), contentAlignment = Alignment.Center) { content(isFocused) }
		}
	}
}
