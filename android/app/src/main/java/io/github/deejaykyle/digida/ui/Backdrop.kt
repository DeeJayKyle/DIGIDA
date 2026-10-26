package io.github.deejaykyle.digida.ui

import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.DrawScope
import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.min
import kotlin.math.sin
import kotlin.random.Random

/** One blob of the aurora: where it rests, how big it is, and how it drifts. */
private data class Blob(
    val x: Float,
    val y: Float,
    val size: Float,
    val hue: Float,
    val phase: Float,
    val reach: Float,
)

/** The desktop app's five blobs, in the same places and hues. */
private val BLOBS = listOf(
    Blob(0.22f, 0.30f, 1.10f, 268f, 0.0f, 0.13f),
    Blob(0.80f, 0.24f, 1.00f, 216f, 1.7f, 0.11f),
    Blob(0.52f, 0.68f, 1.20f, 187f, 3.4f, 0.14f),
    Blob(0.12f, 0.78f, 0.90f, 324f, 5.1f, 0.10f),
    Blob(0.85f, 0.72f, 0.72f, 238f, 2.5f, 0.12f),
)

/**
 * The page under everything: the page colour, the drifting aurora that glass frosts over, and a
 * paper grain under skeuomorphism. The aurora drifts on a slow loop so it never asks much of
 * the GPU.
 */
@Composable
fun Backdrop(palette: Palette, modifier: Modifier = Modifier) {
    val drift = rememberInfiniteTransition(label = "aurora")
    val time by drift.animateFloat(
        initialValue = 0f,
        targetValue = (2 * PI).toFloat(),
        animationSpec = infiniteRepeatable(tween(40_000, easing = LinearEasing), RepeatMode.Restart),
        label = "time",
    )
    val grain = remember { grainDots() }

    Canvas(modifier.fillMaxSize()) {
        drawRect(palette.page)
        if (palette.finish == Finish.Glass) aurora(palette, time)
        if (palette.finish == Finish.Skeuo) {
            val tint = if (palette.dark) Color.White.copy(alpha = 0.035f) else Color.Black.copy(alpha = 0.05f)
            grain.forEach { (x, y) -> drawCircle(tint, 0.9f, Offset(x * size.width, y * size.height)) }
        }
    }
}

private fun DrawScope.aurora(palette: Palette, time: Float) {
    val side = min(size.width, size.height)
    BLOBS.forEach { blob ->
        val x = (blob.x + blob.reach * sin(time + blob.phase)) * size.width
        val y = (blob.y + blob.reach * cos(time * 0.83f + blob.phase * 1.7f)) * size.height
        val radius = side * blob.size * 0.55f * (1f + 0.12f * sin(time * 0.6f + blob.phase * 2.3f))
        val core = Color.hsl(
            blob.hue,
            0.7f,
            if (palette.dark) 0.42f else 0.74f,
            if (palette.dark) 0.55f else 0.7f,
        )
        drawCircle(
            Brush.radialGradient(listOf(core, core.copy(alpha = 0f)), Offset(x, y), radius),
            radius,
            Offset(x, y),
        )
    }
}

/** Scattered points for the paper grain, the same on every launch. */
private fun grainDots(): List<Pair<Float, Float>> {
    val random = Random(7)
    return List(2400) { random.nextFloat() to random.nextFloat() }
}
