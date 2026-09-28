package app.vitals.phone.ui.components

import app.vitals.phone.ui.theme.readable
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.size
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.vitals.phone.data.Series
import app.vitals.ui.Level
import app.vitals.ui.Palette

/** Long enough to read as motion, short enough that most of each one-second update is idle. */
private const val LIVE_MS = 200

/**
 * A ring gauge. [fraction] is null for an unmeasured reading: the track is
 * drawn empty and [valueText] is the em dash, rather than a ring at zero
 * that reads as "idle".
 */
@Composable
fun RingGauge(
    fraction: Float?,
    colour: Color,
    valueText: String,
    label: String,
    modifier: Modifier = Modifier,
    diameter: Dp = 72.dp,
    stroke: Dp = 8.dp,
    level: Level = Level.Ok,
) {
    val target = fraction?.coerceIn(0f, 1f) ?: 0f
    // A short tween read in the draw phase. Measured on the S25, Now tab,
    // frames per 10 s idle: springs everywhere 190-200 (low-stiffness
    // springs never settle inside the one-second update, so the screen
    // never stopped); nothing animated 10; this 114. The State is read in
    // the Canvas lambda so a frame redraws the arc without recomposing.
    val sweep = animateFloatAsState(
        targetValue = target,
        animationSpec = tween(LIVE_MS, easing = FastOutSlowInEasing),
        label = "gauge",
    )
    val track = MaterialTheme.colorScheme.surfaceContainerHighest
    val arc = if (level == Level.Danger || level == Level.Warn) Palette.of(level) else colour
    Column(
        modifier.semantics(mergeDescendants = true) { contentDescription = "$label $valueText" },
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Box(Modifier.size(diameter), contentAlignment = Alignment.Center) {
            Canvas(Modifier.size(diameter)) {
                val w = stroke.toPx()
                val inset = w / 2
                val arcSize = Size(size.width - w, size.height - w)
                val style = Stroke(width = w, cap = StrokeCap.Round)
                drawArc(track, 135f, 270f, false, Offset(inset, inset), arcSize, style = style)
                val s = sweep.value
                if (fraction != null && s > 0f) {
                    drawArc(arc, 135f, 270f * s, false, Offset(inset, inset), arcSize, style = style)
                }
            }
            Text(valueText, style = MaterialTheme.typography.titleSmall, fontWeight = FontWeight.SemiBold, maxLines = 1)
        }
        Text(
            label,
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
            maxLines = 1,
        )
    }
}

/** A number that fades between values instead of snapping, so a changing reading is easy to follow. */
@Composable
fun AnimatedNumber(
    text: String,
    modifier: Modifier = Modifier,
    style: androidx.compose.ui.text.TextStyle = MaterialTheme.typography.bodyLarge,
    colour: Color = Color.Unspecified,
) {
    AnimatedContent(
        targetState = text,
        transitionSpec = { fadeIn(spring(stiffness = Spring.StiffnessMediumLow)) togetherWith fadeOut() },
        label = "number",
        modifier = modifier,
    ) { value ->
        Text(value, style = style, fontWeight = FontWeight.SemiBold, color = readable(colour), maxLines = 1)
    }
}

/**
 * A 60-point sparkline. The path object is reused across frames: this is
 * drawn once per second per card, and allocating in the draw lambda would
 * feed the garbage collector for nothing. Gaps (unmeasured samples) break
 * the line rather than dropping it to zero.
 */
@Composable
fun Sparkline(
    series: Series,
    colour: Color,
    modifier: Modifier = Modifier,
    max: Float = 100f,
) {
    val path = remember { Path() }
    val fill = remember(colour) { colour.copy(alpha = 0.14f) }
    Canvas(modifier.fillMaxWidth().height(32.dp)) {
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
        drawPath(path, colour, style = Stroke(width = 2.dp.toPx(), cap = StrokeCap.Round))
        drawLine(fill, Offset(0f, size.height), Offset(size.width, size.height), strokeWidth = 1.dp.toPx())
    }
}

/**
 * A horizontal fill bar, for per-core load and disk usage. Null draws the
 * track only. The animated value is read in the draw phase (see [RingGauge]).
 */
@Composable
fun FillBar(fraction: Float?, colour: Color, modifier: Modifier = Modifier, height: Dp = 6.dp) {
    val value = animateFloatAsState(
        targetValue = fraction?.coerceIn(0f, 1f) ?: 0f,
        animationSpec = tween(LIVE_MS, easing = FastOutSlowInEasing),
        label = "bar",
    )
    val track = MaterialTheme.colorScheme.surfaceContainerHighest
    Canvas(modifier.fillMaxWidth().height(height)) {
        val r = androidx.compose.ui.geometry.CornerRadius(size.height / 2)
        drawRoundRect(track, cornerRadius = r)
        val v = value.value
        if (fraction != null && v > 0f) {
            drawRoundRect(colour, size = Size(size.width * v, size.height), cornerRadius = r)
        }
    }
}
