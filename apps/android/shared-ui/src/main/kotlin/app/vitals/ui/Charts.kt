package app.vitals.ui

import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

// The drawing behind every gauge, bar and chart, shared by the phone and the
// TV so a reading looks the same on both. Material-free: each surface passes
// its own track and grid colours from its own theme.

/** Long enough to read as motion, short enough that most of each one-second update is idle. */
const val LIVE_ANIMATION_MS = 200

/**
 * A 60-point sparkline. The path object is reused across frames: this is
 * drawn once per second per card, and allocating in the draw lambda would
 * feed the garbage collector for nothing. Gaps (unmeasured samples) break
 * the line rather than dropping it to zero.
 */
@Composable
fun SparklineCanvas(
    series: Series,
    colour: Color,
    modifier: Modifier = Modifier,
    max: Float = 100f,
    height: Dp = 32.dp,
    strokeWidth: Dp = 2.dp,
) {
    val path = remember { Path() }
    val fill = remember(colour) { colour.copy(alpha = 0.14f) }
    Canvas(modifier.fillMaxWidth().height(height)) {
        if (series.size < 2) return@Canvas
        path.rewind()
        val step = size.width / (Series.CAPACITY - 1)
        val offset = (Series.CAPACITY - series.size) * step
        var penDown = false
        for (i in 0 until series.size) {
            val v = series[i]
            if (v.isNaN()) {
                penDown = false
                continue
            }
            val x = offset + i * step
            val y = size.height * (1f - (v / max).coerceIn(0f, 1f))
            if (penDown) path.lineTo(x, y) else path.moveTo(x, y)
            penDown = true
        }
        drawPath(path, colour, style = Stroke(width = strokeWidth.toPx(), cap = StrokeCap.Round))
        drawLine(fill, Offset(0f, size.height), Offset(size.width, size.height), strokeWidth = 1.dp.toPx())
    }
}

/**
 * A horizontal fill bar. Null draws the track only, never an empty bar that
 * reads as "0". The animated value is read in the draw phase, so a frame
 * redraws the bar without recomposing the card around it.
 */
@Composable
fun FillBarCanvas(fraction: Float?, colour: Color, track: Color, modifier: Modifier = Modifier, height: Dp = 6.dp) {
    val value = animateFloatAsState(
        targetValue = fraction?.coerceIn(0f, 1f) ?: 0f,
        animationSpec = tween(LIVE_ANIMATION_MS, easing = FastOutSlowInEasing),
        label = "bar",
    )
    Canvas(modifier.fillMaxWidth().height(height)) {
        val r = CornerRadius(size.height / 2)
        drawRoundRect(track, cornerRadius = r)
        val v = value.value
        if (fraction != null && v > 0f) {
            drawRoundRect(colour, size = Size(size.width * v, size.height), cornerRadius = r)
        }
    }
}

/**
 * The 270° arc of a ring gauge; the caller sizes it and puts the number in
 * the middle. [fraction] null draws the track only. Measured on the S25:
 * springs here kept the screen drawing ~200 frames per 10 s idle because they
 * never settle inside the one-second update; this short tween, read in the
 * draw phase, gave 114.
 */
@Composable
fun RingArc(fraction: Float?, colour: Color, track: Color, modifier: Modifier = Modifier, stroke: Dp = 8.dp) {
    val sweep = animateFloatAsState(
        targetValue = fraction?.coerceIn(0f, 1f) ?: 0f,
        animationSpec = tween(LIVE_ANIMATION_MS, easing = FastOutSlowInEasing),
        label = "gauge",
    )
    Canvas(modifier) {
        val w = stroke.toPx()
        val inset = w / 2
        val arcSize = Size(size.width - w, size.height - w)
        val style = Stroke(width = w, cap = StrokeCap.Round)
        drawArc(track, 135f, 270f, false, Offset(inset, inset), arcSize, style = style)
        val s = sweep.value
        if (fraction != null && s > 0f) {
            drawArc(colour, 135f, 270f * s, false, Offset(inset, inset), arcSize, style = style)
        }
    }
}

/** A bucketed history line over a light grid. `NaN` buckets break the line. */
@Composable
fun HistoryCanvas(chart: Chart, colour: Color, grid: Color, modifier: Modifier = Modifier, height: Dp = 120.dp) {
    val path = remember { Path() }
    Canvas(modifier.fillMaxWidth().height(height)) {
        for (i in 1..3) {
            val y = size.height * i / 4
            drawLine(grid, Offset(0f, y), Offset(size.width, y), 1f)
        }
        val pts = chart.points
        if (pts.size < 2) return@Canvas
        path.rewind()
        val step = size.width / (pts.size - 1)
        var penDown = false
        for (i in pts.indices) {
            val v = pts[i]
            if (v.isNaN()) {
                penDown = false
                continue
            }
            val x = i * step
            val y = size.height * (1f - (v / chart.max).coerceIn(0f, 1f))
            if (penDown) path.lineTo(x, y) else path.moveTo(x, y)
            penDown = true
        }
        drawPath(path, colour, style = Stroke(width = 2.dp.toPx(), cap = StrokeCap.Round))
    }
}
