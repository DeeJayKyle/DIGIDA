package io.github.deejaykyle.digida.ui

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.foundation.background
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.scale
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import coil.compose.AsyncImage
import kotlin.math.abs

/**
 * A surface that can be pressed: it springs in a touch as the finger lands and sinks into the
 * page while held, wherever the finish has relief to give.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
fun Pressable(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    depth: Depth = Depth.Low,
    fill: Color? = null,
    selected: Boolean = false,
    radius: Dp? = null,
    contentAlignment: Alignment = Alignment.Center,
    onLongClick: (() -> Unit)? = null,
    content: @Composable BoxScope.() -> Unit,
) {
    val palette = LocalPalette.current
    val haptics = LocalHapticFeedback.current
    val interaction = remember { MutableInteractionSource() }
    val held by interaction.collectIsPressedAsState()
    val scale by animateFloatAsState(
        if (held) 0.96f else 1f,
        spring(dampingRatio = 0.55f, stiffness = 600f),
        label = "press",
    )
    Box(
        modifier
            .scale(scale)
            .surface(
                palette,
                depth,
                pressed = held || selected,
                fill = fill ?: palette.surface,
                radius = radius ?: palette.radius,
            )
            .combinedClickable(
                interactionSource = interaction,
                indication = null,
                onLongClick = onLongClick?.let { long ->
                    {
                        haptics.performHapticFeedback(HapticFeedbackType.LongPress)
                        long()
                    }
                },
                onClick = onClick,
            ),
        contentAlignment = contentAlignment,
        content = content,
    )
}

/** A round button holding one icon, for the transport and the top bar. */
@Composable
fun IconPill(
    icon: ImageVector,
    description: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    size: Dp = 44.dp,
    accent: Boolean = false,
) {
    val palette = LocalPalette.current
    Pressable(
        onClick = onClick,
        modifier = modifier.size(size),
        fill = if (accent) palette.accent else palette.surface,
        radius = size / 2,
    ) {
        Icon(
            icon,
            contentDescription = description,
            tint = if (accent) palette.onAccent else palette.text,
            modifier = Modifier.size(size * 0.48f),
        )
    }
}

/** An album cover, or a gradient picked from the title when the album has none. */
@Composable
fun Cover(url: String?, seed: String, modifier: Modifier = Modifier, radius: Dp = 12.dp) {
    val shape = RoundedCornerShape(radius)
    val hue = (abs(seed.hashCode()) % 360).toFloat()
    val fallback = Brush.linearGradient(
        listOf(Color.hsl(hue, 0.65f, 0.55f), Color.hsl((hue + 50f) % 360f, 0.65f, 0.45f))
    )
    Box(modifier.clip(shape).background(fallback)) {
        if (url != null) {
            AsyncImage(
                model = url,
                contentDescription = null,
                contentScale = ContentScale.Crop,
                modifier = Modifier.matchParentSize(),
            )
        }
    }
}

/** Text in the palette's own colours, one line by default. */
@Composable
fun Label(
    text: String,
    modifier: Modifier = Modifier,
    size: TextUnit = 15.sp,
    weight: FontWeight = FontWeight.Normal,
    subtle: Boolean = false,
    lines: Int = 1,
) {
    val palette = LocalPalette.current
    Text(
        text,
        modifier = modifier,
        maxLines = lines,
        overflow = TextOverflow.Ellipsis,
        style = TextStyle(
            color = if (subtle) palette.subtle else palette.text,
            fontSize = size,
            fontWeight = weight,
        ),
    )
}

/** `ms` as minutes and seconds, the way a track's length reads. */
fun clock(ms: Long): String {
    val seconds = (ms / 1000).coerceAtLeast(0)
    return "%d:%02d".format(seconds / 60, seconds % 60)
}
