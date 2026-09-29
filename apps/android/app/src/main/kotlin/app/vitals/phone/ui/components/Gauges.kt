package app.vitals.phone.ui.components

import app.vitals.phone.ui.theme.readable
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.spring
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.size
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.vitals.ui.FillBarCanvas
import app.vitals.ui.Series
import app.vitals.ui.Level
import app.vitals.ui.Palette
import app.vitals.ui.RingArc
import app.vitals.ui.SparklineCanvas

/**
 * A ring gauge. [fraction] is null for an unmeasured reading: the track is
 * drawn empty and [valueText] is the em dash, rather than a ring at zero
 * that reads as "idle". The arc is [RingArc], shared with the TV.
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
    val track = MaterialTheme.colorScheme.surfaceContainerHighest
    val arc = if (level == Level.Danger || level == Level.Warn) Palette.of(level) else colour
    Column(
        modifier.semantics(mergeDescendants = true) { contentDescription = "$label $valueText" },
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Box(Modifier.size(diameter), contentAlignment = Alignment.Center) {
            RingArc(fraction, arc, track, Modifier.size(diameter), stroke)
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

/** A 60-point sparkline; see [SparklineCanvas]. */
@Composable
fun Sparkline(
    series: Series,
    colour: Color,
    modifier: Modifier = Modifier,
    max: Float = 100f,
) = SparklineCanvas(series, colour, modifier, max)

/** A horizontal fill bar, for per-core load and disk usage; see [FillBarCanvas]. */
@Composable
fun FillBar(fraction: Float?, colour: Color, modifier: Modifier = Modifier, height: Dp = 6.dp) =
    FillBarCanvas(fraction, colour, MaterialTheme.colorScheme.surfaceContainerHighest, modifier, height)
