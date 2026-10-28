package io.github.deejaykyle.digida.ui

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectVerticalDragGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.deejaykyle.digida.Library
import io.github.deejaykyle.digida.R
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import uniffi.digida_mobile.Headphone

/** How far a band slider reaches either side of flat, in decibels. */
private const val REACH = 12f
/** The bands' centres, for the labels under the sliders. */
private val BANDS = listOf("31", "62", "125", "250", "500", "1k", "2k", "4k", "8k", "16k")

/**
 * The equalizer: on or off, the built-in presets, ten band sliders under a drawn curve, and a
 * search of AutoEq's headphone measurements whose correction plays instead of the bands.
 */
@Composable
fun EqualizerScreen(library: Library, onBack: () -> Unit) {
    val palette = LocalPalette.current
    val sound by library.sound.collectAsStateWithLifecycle()
    val presets = remember { library.presets() }
    val scope = rememberCoroutineScope()
    var query by remember { mutableStateOf("") }
    var found by remember { mutableStateOf<List<Headphone>?>(null) }
    var searching by remember { mutableStateOf(false) }
    var problem by remember { mutableStateOf<String?>(null) }

    // a search waits for the typing to pause, so every key does not fetch
    LaunchedEffect(query) {
        if (query.trim().length < 2) {
            found = null
            return@LaunchedEffect
        }
        delay(350)
        searching = true
        found = runCatching { library.headphones(query) }
            .onFailure { problem = it.message }
            .getOrNull()
        searching = false
    }

    Column(Modifier.fillMaxSize()) {
        TopBar(stringResource(R.string.equalizer), onBack = onBack)
        LazyColumn(contentPadding = PaddingValues(horizontal = 18.dp, vertical = 8.dp)) {
            item {
                Toggle(stringResource(R.string.equalizer_on), stringResource(R.string.equalizer_detail), sound.enabled) {
                    library.setEqualizer(it)
                }
            }
            val headphone = sound.headphone
            if (headphone != null) {
                item {
                    Row(
                        Modifier.fillMaxWidth().padding(top = 12.dp)
                            .surface(palette, Depth.Low, fill = palette.muted).padding(14.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Label(stringResource(R.string.headphone_applied, headphone), Modifier.weight(1f), weight = FontWeight.SemiBold)
                        SmallButton(stringResource(R.string.reset)) { library.setPreset("flat") }
                    }
                }
            }
            item { Heading(stringResource(R.string.presets), null) }
            item {
                LazyRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    items(presets) { id ->
                        SmallButton(presetName(id), accent = sound.preset == id && sound.headphone == null) {
                            library.setPreset(id)
                        }
                    }
                }
            }
            item { Heading(stringResource(R.string.bands), null) }
            item {
                val dim by animateFloatAsState(if (sound.headphone == null) 1f else 0.4f, label = "bands")
                Column(
                    Modifier.fillMaxWidth().graphicsLayer { alpha = dim }
                        .surface(palette, Depth.Mid).padding(vertical = 14.dp, horizontal = 8.dp),
                ) {
                    Response(sound.gains, Modifier.fillMaxWidth().height(70.dp).padding(horizontal = 10.dp))
                    Row(Modifier.fillMaxWidth().height(200.dp).padding(top = 8.dp)) {
                        sound.gains.forEachIndexed { band, gain ->
                            Band(gain, BANDS[band], Modifier.weight(1f).fillMaxHeight()) { library.setBand(band, it) }
                        }
                    }
                }
            }
            item { Heading(stringResource(R.string.headphones), stringResource(R.string.headphones_detail)) }
            item { Field(query, { query = it }, stringResource(R.string.headphones_search), Modifier.fillMaxWidth()) }
            item {
                AnimatedVisibility(searching) {
                    Label(stringResource(R.string.loading), Modifier.padding(vertical = 10.dp), subtle = true)
                }
                problem?.let { Label(it, Modifier.padding(vertical = 10.dp), size = 13.sp, subtle = true, lines = 3) }
            }
            val shown = found
            if (shown != null && shown.isEmpty() && !searching) {
                item { Label(stringResource(R.string.headphones_none), Modifier.padding(vertical = 10.dp), subtle = true) }
            }
            items(shown.orEmpty(), key = { it.path }) { option ->
                Pressable(
                    onClick = {
                        scope.launch {
                            problem = library.applyHeadphone(option).exceptionOrNull()?.message
                        }
                    },
                    modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp),
                    selected = sound.headphone == option.name,
                    contentAlignment = Alignment.CenterStart,
                ) {
                    Column(Modifier.padding(14.dp)) {
                        Label(option.name, weight = FontWeight.SemiBold)
                        if (option.source.isNotEmpty()) {
                            Label(stringResource(R.string.headphone_by, option.source), size = 13.sp, subtle = true)
                        }
                    }
                }
            }
            item { Box(Modifier.height(120.dp)) }
        }
    }
}

/** The bands' gains drawn as one smooth curve over a flat midline. */
@Composable
private fun Response(gains: List<Float>, modifier: Modifier) {
    val palette = LocalPalette.current
    val animated = gains.mapIndexed { band, gain ->
        animateFloatAsState(gain, spring(dampingRatio = 0.75f), label = "curve$band").value
    }
    Canvas(modifier) {
        val mid = size.height / 2
        drawLine(palette.border, Offset(0f, mid), Offset(size.width, mid), strokeWidth = 1.dp.toPx())
        if (animated.isEmpty()) return@Canvas
        val step = size.width / (animated.size - 1).coerceAtLeast(1)
        val points = animated.mapIndexed { band, gain -> Offset(band * step, mid - gain / REACH * mid) }
        val path = Path().apply {
            moveTo(points.first().x, points.first().y)
            for (at in 1 until points.size) {
                val from = points[at - 1]
                val to = points[at]
                val bend = (to.x - from.x) / 2
                cubicTo(from.x + bend, from.y, to.x - bend, to.y, to.x, to.y)
            }
        }
        drawPath(path, palette.accent, style = Stroke(width = 3.dp.toPx(), cap = StrokeCap.Round))
    }
}

/** One band's vertical slider, sunk into the panel with a raised knob, and its frequency. */
@Composable
private fun Band(gain: Float, label: String, modifier: Modifier, onChange: (Float) -> Unit) {
    val palette = LocalPalette.current
    var height by remember { mutableStateOf(1f) }
    val knob = 22.dp
    val fraction = ((gain + REACH) / (2 * REACH)).coerceIn(0f, 1f)
    val shown by animateFloatAsState(fraction, spring(dampingRatio = 0.8f, stiffness = 900f), label = "band")
    // a knob's travel is its track less its own size, so it never leaves the track
    fun gainAt(y: Float, knobPx: Float): Float {
        val travel = (height - knobPx).coerceAtLeast(1f)
        val up = 1f - ((y - knobPx / 2) / travel).coerceIn(0f, 1f)
        return (up * 2 * REACH - REACH).let { Math.round(it * 2) / 2f }
    }
    Column(modifier, horizontalAlignment = Alignment.CenterHorizontally) {
        Label(
            if (gain > 0) "+${"%.1f".format(gain)}" else "%.1f".format(gain),
            size = 10.sp,
            subtle = true,
        )
        Box(
            Modifier
                .weight(1f)
                .width(36.dp)
                .padding(vertical = 6.dp)
                .onSizeChanged { height = it.height.toFloat() }
                .pointerInput(Unit) {
                    val knobPx = knob.toPx()
                    detectVerticalDragGestures(
                        onDragStart = { onChange(gainAt(it.y, knobPx)) },
                    ) { change, _ -> onChange(gainAt(change.position.y, knobPx)) }
                },
            contentAlignment = Alignment.TopCenter,
        ) {
            Box(
                Modifier.width(6.dp).fillMaxHeight()
                    .surface(palette, Depth.Low, pressed = true, fill = palette.muted, radius = 3.dp)
            )
            Box(
                Modifier.align(Alignment.Center).width(14.dp).height(2.dp)
                    .clip(RoundedCornerShape(1.dp)).background(palette.border)
            )
            Box(
                Modifier
                    .graphicsLayer { translationY = (1f - shown) * (height - knob.toPx()) }
                    .size(knob)
                    .surface(palette, Depth.Low, fill = palette.accent, radius = knob / 2)
            )
        }
        Label(label, size = 11.sp, subtle = true)
    }
}

/** A built-in preset's name in the app's language. */
@Composable
private fun presetName(id: String): String = when (id) {
    "flat" -> stringResource(R.string.preset_flat)
    "bass-boost" -> stringResource(R.string.preset_bass_boost)
    "bass-reducer" -> stringResource(R.string.preset_bass_reducer)
    "treble-boost" -> stringResource(R.string.preset_treble_boost)
    "vocal" -> stringResource(R.string.preset_vocal)
    "rock" -> stringResource(R.string.preset_rock)
    "pop" -> stringResource(R.string.preset_pop)
    "jazz" -> stringResource(R.string.preset_jazz)
    "classical" -> stringResource(R.string.preset_classical)
    "electronic" -> stringResource(R.string.preset_electronic)
    "acoustic" -> stringResource(R.string.preset_acoustic)
    "loudness" -> stringResource(R.string.preset_loudness)
    else -> id
}
