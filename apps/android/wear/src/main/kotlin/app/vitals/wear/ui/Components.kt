package app.vitals.wear.ui

import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.wear.compose.material3.CircularProgressIndicator
import androidx.wear.compose.material3.MaterialTheme
import androidx.wear.compose.material3.ProgressIndicatorDefaults
import androidx.wear.compose.material3.Text
import app.vitals.ui.Format
import app.vitals.ui.Level
import app.vitals.ui.Palette
import app.vitals.wear.R
import kotlinx.coroutines.delay

/**
 * A ring whose fill springs to the new value. An unmeasured value draws an
 * empty track — not a ring at zero, which would read as "idle".
 */
@Composable
fun MetricRing(
    fraction: Float?,
    color: Color,
    modifier: Modifier = Modifier,
    strokeWidth: Dp = 6.dp,
) {
    val target = (fraction ?: 0f).coerceIn(0f, 1f)
    val animated by animateFloatAsState(
        targetValue = target,
        animationSpec = spring(dampingRatio = Spring.DampingRatioLowBouncy, stiffness = Spring.StiffnessLow),
        label = "ring",
    )
    CircularProgressIndicator(
        progress = { animated },
        modifier = modifier,
        colors = ProgressIndicatorDefaults.colors(
            indicatorColor = if (fraction == null) Color.Transparent else color,
            trackColor = color.copy(alpha = 0.22f),
        ),
        strokeWidth = strokeWidth,
    )
}

/** A label on the left, a value on the right; the workhorse of every detail list. */
@Composable
fun ValueRow(label: String, value: String, modifier: Modifier = Modifier, valueColor: Color = Color.Unspecified) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .padding(horizontal = 4.dp, vertical = 3.dp)
            .semantics(mergeDescendants = true) {},
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            text = label,
            modifier = Modifier.weight(1f),
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        Spacer(Modifier.width(6.dp))
        Text(
            text = value,
            style = MaterialTheme.typography.labelMedium,
            color = if (valueColor == Color.Unspecified) MaterialTheme.colorScheme.onSurface else valueColor,
            maxLines = 1,
            textAlign = TextAlign.End,
        )
    }
}

/** A thin horizontal gauge; empty when the value is unknown. */
@Composable
fun Gauge(fraction: Float?, color: Color, modifier: Modifier = Modifier) {
    Box(
        modifier
            .fillMaxWidth()
            .height(4.dp)
            .clip(RoundedCornerShape(2.dp))
            .background(color.copy(alpha = 0.18f)),
    ) {
        if (fraction != null) {
            Box(
                Modifier
                    .fillMaxWidth(fraction.coerceIn(0.02f, 1f))
                    .height(4.dp)
                    .background(color),
            )
        }
    }
}

@Composable
fun StatusDot(color: Color, modifier: Modifier = Modifier) {
    Box(modifier.size(8.dp).clip(CircleShape).background(color))
}

/** A value coloured by how worried to look; [Level.Unknown] is neutral grey, never green. */
fun levelColor(level: Level): Color = Palette.of(level)

/** "12 s ago", ticking on its own so the list does not need fresh data to stay honest. */
@Composable
fun relativeAge(fetchedMs: Long): String {
    var now by remember { mutableLongStateOf(System.currentTimeMillis()) }
    LaunchedEffect(fetchedMs) {
        while (true) {
            now = System.currentTimeMillis()
            delay(1_000)
        }
    }
    if (fetchedMs <= 0L) return Format.DASH
    val secs = ((now - fetchedMs) / 1_000).coerceAtLeast(0)
    return when {
        secs < 5 -> stringResource(R.string.age_now)
        secs < 60 -> stringResource(R.string.age_seconds, secs)
        secs < 3_600 -> stringResource(R.string.age_minutes, secs / 60)
        secs < 86_400 -> stringResource(R.string.age_hours, secs / 3_600)
        else -> stringResource(R.string.age_days, secs / 86_400)
    }
}

/** A centred message for an empty or failed screen. */
@Composable
fun Message(title: String, body: String, modifier: Modifier = Modifier) {
    Column(
        modifier = modifier.fillMaxWidth().padding(horizontal = 8.dp).semantics { contentDescription = "$title. $body" },
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(title, style = MaterialTheme.typography.titleMedium, textAlign = TextAlign.Center)
        Spacer(Modifier.height(4.dp))
        Text(
            body,
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
        )
    }
}
