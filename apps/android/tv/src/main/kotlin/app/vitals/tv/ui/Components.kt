package app.vitals.tv.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.tv.material3.Border
import androidx.tv.material3.Button
import androidx.tv.material3.ButtonDefaults
import androidx.tv.material3.ClickableSurfaceDefaults
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.OutlinedButton
import androidx.tv.material3.SelectableSurfaceDefaults
import androidx.tv.material3.Surface
import androidx.tv.material3.SurfaceDefaults
import androidx.tv.material3.Text
import app.vitals.ui.FillBarCanvas
import app.vitals.ui.Level
import app.vitals.ui.Palette
import app.vitals.ui.RingArc
import app.vitals.ui.Series
import app.vitals.ui.SparklineCanvas

// TV building blocks. Everything is sized for the 10-foot view: body text
// at least 16 sp at 1080p, focus shown by scale plus a white border, never by
// colour alone (WCAG 2.4.7 and 1.4.11), and every interactive thing reachable
// with the D-pad because a TV has nothing else.

val CardShape = RoundedCornerShape(20.dp)

/** Overscan-safe margin: Google TV asks for 5 % of each edge, 48 × 27 dp at 1080p. */
val SafeHorizontal = 48.dp
val SafeVertical = 27.dp

/** A section of readings. Not focusable: a TV should not make the user step through every label. */
@Composable
fun Panel(
    title: String,
    modifier: Modifier = Modifier,
    accent: Color = MaterialTheme.colorScheme.primary,
    content: @Composable ColumnScope.() -> Unit,
) {
    Surface(
        modifier = modifier.fillMaxWidth(),
        shape = CardShape,
        colors = SurfaceDefaults.colors(containerColor = MaterialTheme.colorScheme.surface),
    ) {
        Column(Modifier.padding(20.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Text(title, style = MaterialTheme.typography.titleMedium, color = readable(accent), fontWeight = FontWeight.SemiBold)
            content()
        }
    }
}

/**
 * A focusable card. The focused state inverts and grows slightly, and a
 * border is added so focus survives colour-blindness and a washed-out panel.
 */
@Composable
fun FocusCard(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    content: @Composable ColumnScope.() -> Unit,
) {
    Surface(
        onClick = onClick,
        modifier = modifier.fillMaxWidth(),
        shape = ClickableSurfaceDefaults.shape(CardShape),
        scale = ClickableSurfaceDefaults.scale(focusedScale = 1.03f),
        colors = ClickableSurfaceDefaults.colors(
            containerColor = MaterialTheme.colorScheme.surface,
            focusedContainerColor = MaterialTheme.colorScheme.surfaceVariant,
        ),
        border = ClickableSurfaceDefaults.border(
            focusedBorder = Border(BorderStroke(3.dp, Color.White), shape = CardShape),
        ),
    ) {
        Column(Modifier.padding(20.dp), verticalArrangement = Arrangement.spacedBy(10.dp), content = content)
    }
}

/** One choice among several (a sort, a span, a tab). Selection and focus look different on purpose. */
@Composable
fun Choice(label: String, selected: Boolean, onClick: () -> Unit, modifier: Modifier = Modifier) {
    val pill = RoundedCornerShape(50)
    Surface(
        selected = selected,
        onClick = onClick,
        modifier = modifier,
        shape = SelectableSurfaceDefaults.shape(pill),
        colors = SelectableSurfaceDefaults.colors(
            containerColor = MaterialTheme.colorScheme.surface,
            contentColor = MaterialTheme.colorScheme.onSurface,
            selectedContainerColor = MaterialTheme.colorScheme.primaryContainer,
            selectedContentColor = MaterialTheme.colorScheme.onPrimaryContainer,
            focusedContainerColor = MaterialTheme.colorScheme.inverseSurface,
            focusedContentColor = MaterialTheme.colorScheme.inverseOnSurface,
            focusedSelectedContainerColor = MaterialTheme.colorScheme.inverseSurface,
            focusedSelectedContentColor = MaterialTheme.colorScheme.inverseOnSurface,
        ),
        border = SelectableSurfaceDefaults.border(
            selectedBorder = Border(BorderStroke(2.dp, MaterialTheme.colorScheme.primary), shape = pill),
        ),
    ) {
        Text(
            label,
            modifier = Modifier.padding(horizontal = 18.dp, vertical = 8.dp),
            style = MaterialTheme.typography.labelLarge,
        )
    }
}

@Composable
fun ChoiceRow(content: @Composable RowScope.() -> Unit) {
    Row(horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.CenterVertically, content = content)
}

@Composable
fun Action(label: String, onClick: () -> Unit, modifier: Modifier = Modifier, enabled: Boolean = true, danger: Boolean = false) {
    if (danger) {
        Button(
            onClick = onClick,
            enabled = enabled,
            modifier = modifier,
            colors = ButtonDefaults.colors(
                // Darkened until white reads at 4.5:1; the raw danger red gave 4.05:1.
                containerColor = app.vitals.ui.Contrast.readable(Palette.Danger, Color.White),
                contentColor = Color.White,
            ),
        ) { Text(label) }
    } else {
        OutlinedButton(onClick = onClick, enabled = enabled, modifier = modifier) { Text(label) }
    }
}

/** A label on the left, a value on the right; the value is already formatted, em dash included. */
@Composable
fun InfoRow(label: String, value: String, modifier: Modifier = Modifier, valueColour: Color = Color.Unspecified) {
    Row(modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text(
            label,
            modifier = Modifier.weight(1f),
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        Text(
            value,
            style = MaterialTheme.typography.bodyLarge,
            fontWeight = FontWeight.SemiBold,
            color = if (valueColour == Color.Unspecified) MaterialTheme.colorScheme.onSurface else readable(valueColour),
            maxLines = 1,
        )
    }
}

@Composable
fun Hint(text: String, modifier: Modifier = Modifier) {
    Text(text, modifier = modifier, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
}

/** A ring gauge with its number inside; an unmeasured value is the track alone and an em dash. */
@Composable
fun Ring(
    fraction: Float?,
    colour: Color,
    valueText: String,
    label: String,
    modifier: Modifier = Modifier,
    diameter: Dp = 120.dp,
    stroke: Dp = 12.dp,
    level: Level = Level.Ok,
) {
    val arc = if (level == Level.Danger || level == Level.Warn) Palette.of(level) else colour
    Column(
        modifier.semantics(mergeDescendants = true) { contentDescription = "$label $valueText" },
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Box(Modifier.size(diameter), contentAlignment = Alignment.Center) {
            RingArc(fraction, arc, MaterialTheme.colorScheme.surfaceVariant, Modifier.size(diameter), stroke)
            Text(valueText, style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.SemiBold, maxLines = 1)
        }
        Text(
            label,
            style = MaterialTheme.typography.titleSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
            maxLines = 1,
        )
    }
}

@Composable
fun Bar(fraction: Float?, colour: Color, modifier: Modifier = Modifier, height: Dp = 8.dp) =
    FillBarCanvas(fraction, colour, MaterialTheme.colorScheme.surfaceVariant, modifier, height)

@Composable
fun Spark(series: Series, colour: Color, modifier: Modifier = Modifier, max: Float = 100f) =
    SparklineCanvas(series, colour, modifier, max, height = 48.dp, strokeWidth = 3.dp)

@Composable
fun ScreenTitle(text: String, modifier: Modifier = Modifier) {
    Text(text, modifier = modifier, style = MaterialTheme.typography.headlineMedium, fontWeight = FontWeight.SemiBold)
}
