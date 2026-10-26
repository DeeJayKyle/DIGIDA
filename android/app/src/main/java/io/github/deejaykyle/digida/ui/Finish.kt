package io.github.deejaykyle.digida.ui

import android.graphics.BlurMaskFilter
import androidx.annotation.StringRes
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.RoundRect
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.clipPath
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import io.github.deejaykyle.digida.R

/** The material every surface is drawn in, the same four the desktop app offers. */
enum class Finish(val id: String, @StringRes val label: Int) {
    Glass("glass", R.string.finish_glass),
    Clay("clay", R.string.finish_clay),
    Skeuo("skeuo", R.string.finish_skeuo),
    Neu("neu", R.string.finish_neu);

    companion object {
        fun from(id: String?) = entries.firstOrNull { it.id == id } ?: Glass
    }
}

/** Whether the app follows the system's light or dark choice or holds one of its own. */
enum class Mode(val id: String, @StringRes val label: Int) {
    System("system", R.string.theme_system),
    Dark("dark", R.string.theme_dark),
    Light("light", R.string.theme_light);

    companion object {
        fun from(id: String?) = entries.firstOrNull { it.id == id } ?: System
    }
}

/** How far a surface stands off the page: a control, a card, or something raised over both. */
enum class Depth(val near: Dp, val far: Dp) {
    Low(3.dp, 7.dp),
    Mid(6.dp, 14.dp),
    High(10.dp, 26.dp),
}

/** The colours and shape of one finish in one polarity. */
@Immutable
data class Palette(
    val finish: Finish,
    val dark: Boolean,
    val page: Color,
    val surface: Color,
    val raised: Color,
    val muted: Color,
    val border: Color,
    val text: Color,
    val subtle: Color,
    val accent: Color,
    val onAccent: Color,
    val radius: Dp,
)

/** The palette of `finish` on a dark or light page, matching the desktop app's. */
fun palette(finish: Finish, dark: Boolean): Palette = when (finish) {
    Finish.Glass -> if (dark) Palette(
        finish, true, Color(0xFF0B0B12), Color(0x14FFFFFF), Color(0xCC17171F), Color(0x1FFFFFFF),
        Color(0x29FFFFFF), Color(0xFFFAFAFA), Color(0xFFA0A0B0), Color(0xFFFAFAFA), Color(0xFF111118), 14.dp,
    ) else Palette(
        finish, false, Color(0xFFF4F5FB), Color(0x8CFFFFFF), Color(0xD9FFFFFF), Color(0xA6FFFFFF),
        Color(0xCCFFFFFF), Color(0xFF15151C), Color(0xFF5C5C6A), Color(0xFF15151C), Color(0xFFFAFAFA), 14.dp,
    )
    Finish.Clay -> if (dark) Palette(
        finish, true, Color(0xFF1C1A2B), Color(0xFF2A2740), Color(0xFF2D2A45), Color(0xFF35314F),
        Color(0x0FFFFFFF), Color(0xFFF4F1FF), Color(0xFFA9A3C7), Color(0xFFBFAEFF), Color(0xFF1C1A2B), 22.dp,
    ) else Palette(
        finish, false, Color(0xFFF0EBFD), Color(0xFFFDFCFF), Color(0xFFFFFFFF), Color(0xFFE4DCF8),
        Color(0x1F7C5CFF), Color(0xFF221E33), Color(0xFF6E6787), Color(0xFF7C5CFF), Color(0xFFFFFFFF), 22.dp,
    )
    Finish.Skeuo -> if (dark) Palette(
        finish, true, Color(0xFF1F1C19), Color(0xFF2F2B27), Color(0xFF2B2723), Color(0xFF3B3631),
        Color(0x99000000), Color(0xFFF3EEE6), Color(0xFFA89F93), Color(0xFFE0AA4A), Color(0xFF1F1C19), 10.dp,
    ) else Palette(
        finish, false, Color(0xFFE9E4DA), Color(0xFFF7F3EB), Color(0xFFFAF7F1), Color(0xFFD9D2C4),
        Color(0x5C5A4632), Color(0xFF2A241D), Color(0xFF6F6456), Color(0xFF8C5A2B), Color(0xFFFFF8EE), 10.dp,
    )
    Finish.Neu -> if (dark) Palette(
        finish, true, Color(0xFF24272D), Color(0xFF24272D), Color(0xFF272A31), Color(0xFF2C3037),
        Color(0x08FFFFFF), Color(0xFFE8EBF0), Color(0xFF9097A3), Color(0xFF7AA7FF), Color(0xFF14171C), 18.dp,
    ) else Palette(
        finish, false, Color(0xFFE3E8EF), Color(0xFFE3E8EF), Color(0xFFE8EDF3), Color(0xFFD9DFE7),
        Color(0x66FFFFFF), Color(0xFF2B3240), Color(0xFF6B7385), Color(0xFF4A7BEA), Color(0xFFFFFFFF), 18.dp,
    )
}

val LocalPalette = staticCompositionLocalOf { palette(Finish.Glass, true) }

/**
 * Provides the palette of `finish` and `mode`, easing each colour over when either changes, the
 * way the desktop app fades its theme.
 */
@Composable
fun DigidaTheme(finish: Finish, mode: Mode, content: @Composable () -> Unit) {
    val dark = when (mode) {
        Mode.System -> isSystemInDarkTheme()
        Mode.Dark -> true
        Mode.Light -> false
    }
    val target = palette(finish, dark)
    val spec = tween<Color>(durationMillis = 420)
    val page by animateColorAsState(target.page, spec, label = "page")
    val surface by animateColorAsState(target.surface, spec, label = "surface")
    val raised by animateColorAsState(target.raised, spec, label = "raised")
    val muted by animateColorAsState(target.muted, spec, label = "muted")
    val border by animateColorAsState(target.border, spec, label = "border")
    val text by animateColorAsState(target.text, spec, label = "text")
    val subtle by animateColorAsState(target.subtle, spec, label = "subtle")
    val accent by animateColorAsState(target.accent, spec, label = "accent")
    val onAccent by animateColorAsState(target.onAccent, spec, label = "onAccent")
    val eased = target.copy(
        page = page, surface = surface, raised = raised, muted = muted, border = border,
        text = text, subtle = subtle, accent = accent, onAccent = onAccent,
    )
    CompositionLocalProvider(LocalPalette provides eased, content = content)
}

/**
 * Stands a surface off the page in the active finish and fills it: frosted and lit along its
 * top on glass, puffed by an inner light and shade on clay, a lit gradient on a contact shadow
 * for skeuomorphism, and pushed out of the page between a light and a shade for neumorphism.
 * `pressed` sinks it into the page instead.
 */
fun Modifier.surface(
    palette: Palette,
    depth: Depth = Depth.Mid,
    pressed: Boolean = false,
    fill: Color = palette.surface,
    radius: Dp = palette.radius,
): Modifier {
    val shape = RoundedCornerShape(radius)
    val outer = when {
        pressed -> this
        else -> this.drawBehind { outerShadows(palette, depth, radius) }
    }
    val filled = when (palette.finish) {
        Finish.Skeuo -> outer.clip(shape).background(
            Brush.verticalGradient(listOf(lighten(fill, 0.05f), lighten(fill, -0.05f)))
        )
        else -> outer.clip(shape).background(fill)
    }
    val edged = when (palette.finish) {
        Finish.Glass -> filled.border(
            1.dp,
            Brush.verticalGradient(listOf(Color.White.copy(alpha = if (palette.dark) 0.22f else 0.9f), palette.border)),
            shape,
        )
        Finish.Skeuo -> filled.border(1.dp, palette.border, shape)
        else -> filled
    }
    return edged.drawWithContent {
        drawContent()
        innerShadows(palette, depth, radius, pressed)
    }
}

/** The shadows a raised surface casts around itself. */
private fun DrawScope.outerShadows(palette: Palette, depth: Depth, radius: Dp) {
    val near = depth.near.toPx()
    val far = depth.far.toPx()
    val dark = palette.dark
    when (palette.finish) {
        Finish.Glass -> if (depth != Depth.Low) {
            shadow(0f, near, far * 1.6f, Color.Black.copy(alpha = if (dark) 0.35f else 0.12f), radius)
        }
        Finish.Clay -> shadow(
            0f, near * 1.5f, far * 1.6f,
            if (dark) Color.Black.copy(alpha = 0.45f) else palette.accent.copy(alpha = 0.22f), radius,
        )
        Finish.Skeuo -> {
            shadow(0f, 1f, 2f, Color.Black.copy(alpha = if (dark) 0.7f else 0.25f), radius)
            shadow(0f, near, far, Color.Black.copy(alpha = if (dark) 0.4f else 0.14f), radius)
        }
        Finish.Neu -> {
            shadow(near, near, far, if (dark) Color.Black.copy(alpha = 0.5f) else Color(0xAAA3B1C6), radius)
            shadow(-near, -near, far, Color.White.copy(alpha = if (dark) 0.05f else 0.95f), radius)
        }
    }
}

/** The light and shade a surface catches inside its own edge. */
private fun DrawScope.innerShadows(palette: Palette, depth: Depth, radius: Dp, pressed: Boolean) {
    val near = depth.near.toPx()
    val far = depth.far.toPx()
    val dark = palette.dark
    when {
        pressed && palette.finish != Finish.Glass -> {
            inset(near, near, far, if (dark) Color.Black.copy(alpha = 0.5f) else Color(0x99A3B1C6), radius)
            inset(-near, -near, far, Color.White.copy(alpha = if (dark) 0.05f else 0.85f), radius)
        }
        palette.finish == Finish.Clay -> {
            inset(near, near, far, Color.White.copy(alpha = if (dark) 0.07f else 0.85f), radius)
            inset(-near, -near, far, Color.Black.copy(alpha = if (dark) 0.35f else 0.08f), radius)
        }
        palette.finish == Finish.Skeuo -> {
            val top = Color.White.copy(alpha = if (dark) 0.09f else 0.8f)
            drawLine(top, Offset(radius.toPx(), 1f), Offset(size.width - radius.toPx(), 1f), 1.5f)
        }
        else -> Unit
    }
}

/** A blurred copy of the surface's shape, offset by `x` and `y`, under the surface. */
private fun DrawScope.shadow(x: Float, y: Float, blur: Float, color: Color, radius: Dp) {
    drawIntoCanvas { canvas ->
        val paint = android.graphics.Paint().apply {
            isAntiAlias = true
            this.color = color.toArgb()
            maskFilter = BlurMaskFilter(blur.coerceAtLeast(0.5f), BlurMaskFilter.Blur.NORMAL)
        }
        val r = radius.toPx()
        canvas.nativeCanvas.drawRoundRect(x, y, size.width + x, size.height + y, r, r, paint)
    }
}

/**
 * A shadow cast inward: a thick blurred ring around the shape, offset by `x` and `y`, clipped
 * to the shape itself so only its soft inner edge shows.
 */
private fun DrawScope.inset(x: Float, y: Float, blur: Float, color: Color, radius: Dp) {
    val r = radius.toPx()
    val clip = Path().apply {
        addRoundRect(RoundRect(0f, 0f, size.width, size.height, CornerRadius(r, r)))
    }
    clipPath(clip) {
        drawIntoCanvas { canvas ->
            val paint = android.graphics.Paint().apply {
                isAntiAlias = true
                this.color = color.toArgb()
                style = android.graphics.Paint.Style.STROKE
                strokeWidth = blur * 2f
                maskFilter = BlurMaskFilter(blur.coerceAtLeast(0.5f), BlurMaskFilter.Blur.NORMAL)
            }
            canvas.nativeCanvas.drawRoundRect(
                x - blur, y - blur, size.width + x + blur, size.height + y + blur, r + blur, r + blur, paint,
            )
        }
    }
}

private fun lighten(color: Color, amount: Float): Color {
    val target = if (amount > 0) Color.White else Color.Black
    val t = kotlin.math.abs(amount)
    return Color(
        red = color.red + (target.red - color.red) * t,
        green = color.green + (target.green - color.green) * t,
        blue = color.blue + (target.blue - color.blue) * t,
        alpha = color.alpha,
    )
}
