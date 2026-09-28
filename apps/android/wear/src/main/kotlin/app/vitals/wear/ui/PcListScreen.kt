package app.vitals.wear.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.wear.compose.foundation.lazy.TransformingLazyColumn
import androidx.wear.compose.foundation.lazy.items
import androidx.wear.compose.foundation.lazy.rememberTransformingLazyColumnState
import androidx.wear.compose.material3.ListHeader
import androidx.wear.compose.material3.MaterialTheme
import androidx.wear.compose.material3.ScreenScaffold
import androidx.wear.compose.material3.SurfaceTransformation
import androidx.wear.compose.material3.Text
import androidx.wear.compose.material3.TitleCard
import androidx.wear.compose.material3.lazy.rememberTransformationSpec
import androidx.wear.compose.material3.lazy.transformedHeight
import app.vitals.core.model.Severity
import app.vitals.core.pairing.Pairing
import app.vitals.core.wear.PcState
import app.vitals.ui.Format
import app.vitals.ui.Palette
import app.vitals.ui.Thresholds
import app.vitals.wear.R
import app.vitals.wear.data.Headline

@Composable
fun PcListScreen(
    pairings: List<Pairing>,
    states: Map<String, PcState>,
    loaded: Boolean,
    onOpen: (String) -> Unit,
    device: app.vitals.device.DeviceMonitor,
    onOpenSelf: () -> Unit,
) {
    val listState = rememberTransformingLazyColumnState()
    val spec = rememberTransformationSpec()
    ScreenScaffold(scrollState = listState) { padding ->
        TransformingLazyColumn(state = listState, contentPadding = padding) {
            item {
                ListHeader(modifier = Modifier.transformedHeight(this, spec), transformation = SurfaceTransformation(spec)) {
                    Text(stringResource(R.string.app_name))
                }
            }
            item(key = "self") {
                WatchSelfCard(
                    monitor = device,
                    onClick = onOpenSelf,
                    modifier = Modifier.transformedHeight(this, spec),
                    transformation = SurfaceTransformation(spec),
                )
            }
            if (pairings.isEmpty()) {
                item {
                    Message(
                        title = stringResource(if (loaded) R.string.empty_title else R.string.loading),
                        body = if (loaded) stringResource(R.string.empty_body) else "",
                        modifier = Modifier.transformedHeight(this, spec),
                    )
                }
            } else {
                items(pairings, key = { it.id }) { pairing ->
                    val state = states[pairing.id]
                    PcCard(
                        label = pairing.label,
                        headline = state?.let(Headline::of),
                        onClick = { onOpen(pairing.id) },
                        modifier = Modifier.transformedHeight(this, spec),
                        transformation = SurfaceTransformation(spec),
                    )
                }
            }
        }
    }
}

@Composable
private fun PcCard(
    label: String,
    headline: Headline?,
    onClick: () -> Unit,
    modifier: Modifier,
    transformation: SurfaceTransformation,
) {
    TitleCard(
        onClick = onClick,
        title = { Text(label, maxLines = 1, overflow = TextOverflow.Ellipsis) },
        modifier = modifier.fillMaxWidth(),
        time = {
            val dot = when (headline?.worst) {
                Severity.Critical -> Palette.Danger
                Severity.Warning -> Palette.Warn
                else -> null
            }
            if (dot != null) StatusDot(dot)
        },
        subtitle = {
            Text(
                text = when {
                    headline == null -> stringResource(R.string.waiting_for_data)
                    !headline.reachable -> stringResource(R.string.last_seen, relativeAge(headline.fetchedMs))
                    else -> relativeAge(headline.fetchedMs)
                },
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        },
        transformation = transformation,
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box(Modifier.size(40.dp), contentAlignment = Alignment.Center) {
                MetricRing(fraction = headline?.cpu?.div(100f), color = Palette.Cpu, modifier = Modifier.size(40.dp), strokeWidth = 4.dp)
                Text(Format.percentCompact(headline?.cpu), style = MaterialTheme.typography.labelSmall)
            }
            Spacer(Modifier.width(10.dp))
            Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(
                    stringResource(R.string.ram_value, Format.percent(headline?.memory)),
                    style = MaterialTheme.typography.bodySmall,
                    color = readable(levelColor(Thresholds.memory(headline?.memory))).takeIf { headline?.memory != null }
                        ?: MaterialTheme.colorScheme.onSurface,
                )
                Text(
                    stringResource(R.string.temp_value, Format.celsius(headline?.cpuTemp)),
                    style = MaterialTheme.typography.bodySmall,
                    color = readable(levelColor(Thresholds.cpuTemp(headline?.cpuTemp))).takeIf { headline?.cpuTemp != null }
                        ?: MaterialTheme.colorScheme.onSurface,
                )
            }
        }
    }
}
