package app.vitals.wear.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.wear.compose.foundation.lazy.TransformingLazyColumn
import androidx.wear.compose.foundation.lazy.items
import androidx.wear.compose.foundation.lazy.rememberTransformingLazyColumnState
import androidx.wear.compose.material3.Button
import androidx.wear.compose.material3.Card
import androidx.wear.compose.material3.EdgeButton
import androidx.wear.compose.material3.EdgeButtonSize
import androidx.wear.compose.material3.ListHeader
import androidx.wear.compose.material3.MaterialTheme
import androidx.wear.compose.material3.ScreenScaffold
import androidx.wear.compose.material3.SurfaceTransformation
import androidx.wear.compose.material3.Text
import androidx.wear.compose.material3.lazy.rememberTransformationSpec
import androidx.wear.compose.material3.lazy.transformedHeight
import app.vitals.core.model.Severity
import app.vitals.core.wear.PcState
import app.vitals.ui.Format
import app.vitals.ui.Palette
import app.vitals.ui.Readings
import app.vitals.ui.Thresholds
import app.vitals.wear.R

@Composable
fun PcDetailScreen(
    state: PcState?,
    label: String,
    onSensors: () -> Unit,
    onProcesses: () -> Unit,
) {
    val listState = rememberTransformingLazyColumnState()
    val spec = rememberTransformationSpec()
    val system = state?.summary?.system
    ScreenScaffold(
        scrollState = listState,
        edgeButton = {
            EdgeButton(onClick = onSensors, buttonSize = EdgeButtonSize.Medium) {
                Text(stringResource(R.string.sensors))
            }
        },
    ) { padding ->
        TransformingLazyColumn(state = listState, contentPadding = padding) {
            item {
                ListHeader(modifier = Modifier.transformedHeight(this, spec), transformation = SurfaceTransformation(spec)) {
                    Text(label, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
            item {
                Hero(state, modifier = Modifier.transformedHeight(this, spec))
            }
            if (state != null && system == null) {
                item {
                    Message(
                        title = stringResource(R.string.unreachable_title),
                        body = stringResource(R.string.last_seen, relativeAge(state.fetchedMs)),
                        modifier = Modifier.transformedHeight(this, spec),
                    )
                }
            }
            val critical = state?.alerts.orEmpty().sortedByDescending { it.severity }
            items(critical, key = { "${it.kind}|${it.subject}" }) { alert ->
                Card(
                    modifier = Modifier.fillMaxWidth().transformedHeight(this, spec),
                    transformation = SurfaceTransformation(spec),
                ) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        StatusDot(if (alert.severity == Severity.Critical) Palette.Danger else Palette.Warn)
                        Text(
                            alert.title,
                            modifier = Modifier.padding(start = 6.dp),
                            style = MaterialTheme.typography.labelMedium,
                            maxLines = 2,
                        )
                    }
                    Text(alert.cause, style = MaterialTheme.typography.bodySmall, maxLines = 3)
                }
            }
            if (system != null) {
                val gpu = Readings.primaryGpu(system)
                item {
                    Section(stringResource(R.string.section_cpu), Modifier.transformedHeight(this, spec), SurfaceTransformation(spec)) {
                        ValueRow(stringResource(R.string.usage), Format.percent(system.cpu.total))
                        ValueRow(stringResource(R.string.clock), Format.ghz(system.cpu.effectiveClock))
                        ValueRow(stringResource(R.string.power), Format.watts(system.cpu.power))
                    }
                }
                item {
                    Section(stringResource(R.string.section_memory), Modifier.transformedHeight(this, spec), SurfaceTransformation(spec)) {
                        ValueRow(
                            stringResource(R.string.in_use),
                            stringResource(R.string.of_total, Format.bytes(system.memory.used), Format.bytes(system.memory.total)),
                        )
                        Gauge(Readings.memoryPercent(system)?.div(100f), Palette.Memory, Modifier.padding(horizontal = 4.dp, vertical = 2.dp))
                    }
                }
                if (gpu != null) {
                    item {
                        Section(gpu.name, Modifier.transformedHeight(this, spec), SurfaceTransformation(spec)) {
                            ValueRow(stringResource(R.string.usage), Format.percent(gpu.utilization))
                            ValueRow(
                                stringResource(R.string.temperature),
                                Format.celsius(gpu.temperature),
                                valueColor = temperatureColor(Thresholds.gpuTemp(gpu.temperature)),
                            )
                            ValueRow(stringResource(R.string.power), Format.watts(gpu.power))
                        }
                    }
                }
                item {
                    Section(stringResource(R.string.section_io), Modifier.transformedHeight(this, spec), SurfaceTransformation(spec)) {
                        // A PC with no disks reported has no rate to sum; zero would claim idle.
                        val read = system.disks.takeIf { it.isNotEmpty() }?.sumOf { it.read }
                        val write = system.disks.takeIf { it.isNotEmpty() }?.sumOf { it.write }
                        val connected = system.networks.any { it.connected }
                        ValueRow(stringResource(R.string.disk_read), Format.rate(read))
                        ValueRow(stringResource(R.string.disk_write), Format.rate(write))
                        ValueRow(stringResource(R.string.net_down), Format.rate(Readings.networkRx(system).takeIf { connected }))
                        ValueRow(stringResource(R.string.net_up), Format.rate(Readings.networkTx(system).takeIf { connected }))
                    }
                }
                if (system.fans.isNotEmpty()) {
                    item {
                        Section(stringResource(R.string.section_fans), Modifier.transformedHeight(this, spec), SurfaceTransformation(spec)) {
                            system.fans.forEach { ValueRow(it.name, Format.rpm(it.rpm)) }
                        }
                    }
                }
                item {
                    Button(
                        onClick = onProcesses,
                        modifier = Modifier.fillMaxWidth().transformedHeight(this, spec),
                        transformation = SurfaceTransformation(spec),
                        label = { Text(stringResource(R.string.top_processes)) },
                        secondaryLabel = {
                            val count = state.summary?.processCount ?: 0
                            // Wear M3 dims a secondary label on a filled button;
                            // on the watch-face blue that was 4.0:1.
                            Text(
                                pluralStringResource(R.plurals.process_count, count, count),
                                color = MaterialTheme.colorScheme.onPrimary,
                            )
                        },
                    )
                }
            }
        }
    }
}

@Composable
private fun Hero(state: PcState?, modifier: Modifier = Modifier) {
    val system = state?.summary?.system
    val cpu = system?.cpu?.total
    val memory = system?.let(Readings::memoryPercent)
    val gpu = system?.let(Readings::primaryGpu)?.utilization
    val temp = system?.let(Readings::cpuTemperature)
    val description = stringResource(
        R.string.hero_description,
        Format.percent(cpu),
        Format.percent(memory),
        Format.percent(gpu),
        Format.celsius(temp),
    )
    Box(
        modifier = modifier
            .fillMaxWidth()
            .padding(vertical = 4.dp)
            .semantics { contentDescription = description },
        contentAlignment = Alignment.Center,
    ) {
        MetricRing(cpu?.div(100f), Palette.Cpu, Modifier.size(128.dp), strokeWidth = 8.dp)
        MetricRing(memory?.div(100f), Palette.Memory, Modifier.size(104.dp), strokeWidth = 8.dp)
        MetricRing(gpu?.div(100f), Palette.Gpu, Modifier.size(80.dp), strokeWidth = 8.dp)
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.Center) {
            Text(
                Format.celsiusCompact(temp),
                style = MaterialTheme.typography.numeralSmall,
                color = temperatureColor(Thresholds.cpuTemp(temp)),
            )
            Text(
                stringResource(R.string.cpu_short),
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
internal fun Section(
    title: String,
    modifier: Modifier,
    transformation: SurfaceTransformation,
    content: @Composable () -> Unit,
) {
    Card(
        modifier = modifier.fillMaxWidth(),
        transformation = transformation,
    ) {
        Text(
            title,
            style = MaterialTheme.typography.titleSmall,
            // The card's default title colour comes from the watch face's
            // dynamic palette and measured 2.2:1 on the Galaxy Watch 7.
            color = readable(MaterialTheme.colorScheme.primary),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.padding(start = 4.dp, bottom = 2.dp),
        )
        content()
    }
}

/** Unknown stays the text colour: grey on a number that is not there would look like a reading. */
@Composable
fun temperatureColor(level: app.vitals.ui.Level) =
    if (level == app.vitals.ui.Level.Unknown) MaterialTheme.colorScheme.onSurface else readable(levelColor(level))
