package app.vitals.wear.ui

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.calculateEndPadding
import androidx.compose.foundation.layout.calculateStartPadding
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.wear.compose.foundation.lazy.TransformingLazyColumn
import androidx.wear.compose.foundation.lazy.TransformingLazyColumnItemScope
import androidx.wear.compose.foundation.lazy.TransformingLazyColumnScope
import androidx.wear.compose.foundation.lazy.TransformingLazyColumnState
import androidx.wear.compose.foundation.lazy.rememberTransformingLazyColumnState
import androidx.wear.compose.material3.ButtonDefaults
import androidx.wear.compose.material3.Card
import androidx.wear.compose.material3.CardDefaults
import androidx.wear.compose.material3.ListHeader
import androidx.wear.compose.material3.ListHeaderDefaults
import androidx.wear.compose.material3.MaterialTheme
import androidx.wear.compose.material3.ScreenScaffold
import androidx.wear.compose.material3.SurfaceTransformation
import androidx.wear.compose.material3.Text
import androidx.wear.compose.material3.lazy.TransformationSpec
import androidx.wear.compose.material3.lazy.rememberTransformationSpec
import androidx.wear.compose.material3.lazy.transformedHeight

/**
 * Every scrolling screen of the watch app (S22-12).
 *
 * Play rejected 1.0.4 on 2026-10-19 under "Wear App Quality: Watch shapes":
 * on a round watch the first card, the "No PCs yet" text and the sensor rows
 * were cut by the bezel. Two causes, both fixed here rather than per screen:
 *
 * - The scaffold's default vertical padding let the first and last items rest
 *   in the part of the circle that is narrower than they are. The padding is
 *   now at least what Material 3 recommends for the item types these lists
 *   start and end with (a header at the top; a button or card at the bottom).
 * - Several screens put six to thirteen rows inside ONE card. A card taller
 *   than the visible circle always has rows at its top and bottom edge where
 *   the display is narrowest, and the list's edge morphing cannot shrink part
 *   of an item. Rows are now list items of their own ([ValueCard]), so each
 *   one is narrowed as it approaches the edge.
 *
 * `SafeAreaTest` renders every screen on small and large round watches, a
 * square one and the largest font, and fails when a line of text or a
 * control crosses the display.
 */
@Composable
fun VitalsList(
    state: TransformingLazyColumnState = rememberTransformingLazyColumnState(),
    edgeButton: (@Composable BoxScope.() -> Unit)? = null,
    content: TransformingLazyColumnScope.(TransformationSpec) -> Unit,
) {
    val spec = rememberTransformationSpec()
    val top = ListHeaderDefaults.minimumTopListContentPadding
    val bottom = maxOf(
        ButtonDefaults.minimumVerticalListContentPadding,
        CardDefaults.minimumVerticalListContentPadding,
        ListHeaderDefaults.minimumBottomListContentPadding,
    )
    val direction = LocalLayoutDirection.current
    val list: @Composable BoxScope.(PaddingValues) -> Unit = { padding ->
        TransformingLazyColumn(
            state = state,
            contentPadding = PaddingValues(
                start = padding.calculateStartPadding(direction),
                end = padding.calculateEndPadding(direction),
                top = maxOf(top, padding.calculateTopPadding()),
                bottom = maxOf(bottom, padding.calculateBottomPadding()),
            ),
            modifier = Modifier.fillMaxWidth(),
        ) { content(spec) }
    }
    if (edgeButton != null) {
        ScreenScaffold(scrollState = state, edgeButton = edgeButton, content = list)
    } else {
        ScreenScaffold(scrollState = state, content = list)
    }
}

/** The screen title: centred, one line, ellipsised, so it never reaches the curve. */
@Composable
fun TransformingLazyColumnItemScope.ScreenHeader(spec: TransformationSpec, text: String, color: Color = Color.Unspecified) {
    ListHeader(modifier = Modifier.fillMaxWidth().transformedHeight(this, spec), transformation = SurfaceTransformation(spec)) {
        Text(text, color = color, textAlign = TextAlign.Center, maxLines = 2, overflow = TextOverflow.Ellipsis)
    }
}

/** A section title between rows (CPU, Memory, a GPU's name). */
@Composable
fun TransformingLazyColumnItemScope.SectionHeader(spec: TransformationSpec, text: String) {
    ListHeader(modifier = Modifier.fillMaxWidth().transformedHeight(this, spec), transformation = SurfaceTransformation(spec)) {
        Text(
            text,
            style = MaterialTheme.typography.titleSmall,
            // The card's default title colour comes from the watch face's
            // dynamic palette and measured 2.2:1 on the Galaxy Watch 7.
            color = readable(MaterialTheme.colorScheme.primary),
            textAlign = TextAlign.Center,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

/**
 * One reading as one list item: label and value, with an optional gauge
 * underneath. Its own item so the list narrows it near the edge of a round
 * screen; see [VitalsList].
 */
@Composable
fun TransformingLazyColumnItemScope.ValueCard(
    spec: TransformationSpec,
    label: String,
    value: String,
    valueColor: Color = Color.Unspecified,
    gauge: Float? = null,
    gaugeColor: Color? = null,
) {
    Card(
        modifier = Modifier.fillMaxWidth().transformedHeight(this, spec),
        transformation = SurfaceTransformation(spec),
        contentPadding = CompactCardPadding,
    ) {
        ValueRow(label, value, valueColor = valueColor)
        if (gaugeColor != null) {
            Gauge(gauge, gaugeColor, Modifier.padding(horizontal = 4.dp, vertical = 2.dp))
        }
    }
}

/**
 * Plain text in a list (an explanation, a footnote). Not a surface, so the
 * list does not narrow it: it is centred and kept to the middle of the
 * width, where a round display is widest.
 */
@Composable
fun TransformingLazyColumnItemScope.Note(spec: TransformationSpec, text: String) {
    Box(Modifier.fillMaxWidth().transformedHeight(this, spec), contentAlignment = Alignment.Center) {
        Text(
            text,
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
            modifier = Modifier.fillMaxWidth(TEXT_WIDTH),
        )
    }
}

/** A centred title and body for an empty or failed screen, kept clear of the curve like [Note]. */
@Composable
fun Message(title: String, body: String, modifier: Modifier = Modifier) {
    Box(modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
        Column(
            modifier = Modifier.fillMaxWidth(TEXT_WIDTH).semantics(mergeDescendants = true) {
                contentDescription = if (body.isEmpty()) title else "$title. $body"
            },
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Text(title, style = MaterialTheme.typography.titleMedium, textAlign = TextAlign.Center)
            if (body.isNotEmpty()) {
                Spacer(Modifier.height(4.dp))
                Text(
                    body,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    textAlign = TextAlign.Center,
                )
            }
        }
    }
}

/**
 * Share of the list width plain text may use. A centred line this wide still
 * fits the circle where the first and last items rest (Material's list
 * paddings put them about 13 % and 23 % in from the edge).
 */
internal const val TEXT_WIDTH = 0.8f

private val CompactCardPadding = PaddingValues(horizontal = 10.dp, vertical = 8.dp)
