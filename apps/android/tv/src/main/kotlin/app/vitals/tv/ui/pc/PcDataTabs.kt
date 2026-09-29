package app.vitals.tv.ui.pc

import androidx.annotation.StringRes
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.repeatOnLifecycle
import androidx.tv.material3.MaterialTheme
import app.vitals.core.model.HostInfo
import app.vitals.core.model.SensorLine
import app.vitals.core.net.ApiResult
import app.vitals.core.pairing.Pairing
import app.vitals.tv.R
import app.vitals.tv.data.tvGraph
import app.vitals.tv.ui.Choice
import app.vitals.tv.ui.ChoiceRow
import app.vitals.tv.ui.Hint
import app.vitals.tv.ui.InfoRow
import app.vitals.tv.ui.Panel
import app.vitals.tv.ui.relativeTime
import app.vitals.ui.Chart
import app.vitals.ui.Format
import app.vitals.ui.HistoryCanvas
import app.vitals.ui.Level
import app.vitals.ui.Palette
import app.vitals.ui.Thresholds
import app.vitals.ui.chart
import kotlinx.coroutines.delay

internal enum class Span(val seconds: Int, @param:StringRes val label: Int) {
    Hour(3_600, R.string.history_1h),
    Day(86_400, R.string.history_24h),
    Week(7 * 86_400, R.string.history_7d),
}

private enum class SensorGroup(@param:StringRes val title: Int, val accent: Color) {
    Temperature(R.string.sensors_group_temperature, Palette.Thermal),
    Fan(R.string.sensors_group_fan, Palette.Power),
    Power(R.string.sensors_group_power, Palette.Gpu),
    Voltage(R.string.sensors_group_voltage, Palette.Disk),
    Percent(R.string.sensors_group_percent, Palette.Memory),
    Other(R.string.sensors_group_other, Palette.Network),
}

private fun groupOf(unit: String) = when (unit) {
    "temperature" -> SensorGroup.Temperature
    "fanSpeed" -> SensorGroup.Fan
    "power" -> SensorGroup.Power
    "voltage" -> SensorGroup.Voltage
    "percent", "charge" -> SensorGroup.Percent
    else -> SensorGroup.Other
}

/**
 * Polled every five seconds only while shown and the app is in front: the
 * desktop caches `/sensors` for five seconds because reading them means WMI,
 * so polling faster would buy nothing.
 */
@Composable
internal fun SensorsTab(pairing: Pairing) {
    val graph = LocalContext.current.tvGraph
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    var lines by remember { mutableStateOf<List<SensorLine>?>(null) }
    var failed by remember { mutableStateOf(false) }
    LaunchedEffect(pairing.id, lifecycle) {
        lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) {
            val client = graph.client(pairing)
            while (true) {
                when (val r = client.sensors()) {
                    is ApiResult.Ok -> {
                        lines = r.value
                        failed = false
                    }
                    is ApiResult.Err -> failed = lines == null
                }
                delay(5_000)
            }
        }
    }
    val l = lines
    when {
        failed -> Hint(stringResource(R.string.sensors_unavailable))
        l == null -> Hint(stringResource(R.string.state_connecting))
        l.isEmpty() -> Hint(stringResource(R.string.sensors_empty))
        else -> {
            val groups = l.groupBy { groupOf(it.unit) }.toSortedMap().toList()
            // Three columns of cards: sensor lists are short and many.
            LazyVerticalGrid(
                GridCells.Fixed(3),
                horizontalArrangement = Arrangement.spacedBy(16.dp),
                verticalArrangement = Arrangement.spacedBy(16.dp),
                contentPadding = PaddingValues(bottom = 24.dp),
                modifier = Modifier.fillMaxSize(),
            ) {
                items(groups, key = { it.first.name }) { (group, items) ->
                    Panel(stringResource(group.title), accent = group.accent) {
                        items.forEach { line ->
                            val level = if (group == SensorGroup.Temperature) Thresholds.temperature(line.key, line.value) else Level.Unknown
                            InfoRow(line.label, Format.sensor(line.unit, line.value), valueColour = if (level == Level.Unknown) Color.Unspecified else Palette.of(level))
                        }
                    }
                }
            }
        }
    }
}

private sealed interface HistoryState {
    data object Loading : HistoryState
    data object Failed : HistoryState
    data class Loaded(val cpu: Chart, val memory: Chart, val gpu: Chart, val temp: Chart, val empty: Boolean) : HistoryState
}

@Composable
internal fun HistoryTab(pairing: Pairing) {
    val graph = LocalContext.current.tvGraph
    var span by rememberSaveable { mutableStateOf(Span.Hour) }
    var state by remember { mutableStateOf<HistoryState>(HistoryState.Loading) }
    LaunchedEffect(pairing.id, span) {
        state = HistoryState.Loading
        state = when (val r = graph.client(pairing).history(span.seconds)) {
            is ApiResult.Ok -> {
                val s = r.value.sortedBy { it.ts }
                HistoryState.Loaded(
                    cpu = chart(s, 100f) { it.cpuPercent },
                    memory = chart(s, 100f) { if (it.memoryTotal > 0) it.memoryUsed * 100f / it.memoryTotal else null },
                    gpu = chart(s, 100f) { it.gpuPercent },
                    temp = chart(s, 110f) { it.cpuTempC },
                    empty = s.isEmpty(),
                )
            }
            is ApiResult.Err -> HistoryState.Failed
        }
    }
    Column(verticalArrangement = Arrangement.spacedBy(16.dp)) {
        ChoiceRow { Span.entries.forEach { s -> Choice(stringResource(s.label), selected = span == s, onClick = { span = s }) } }
        when (val s = state) {
            HistoryState.Loading -> Hint(stringResource(R.string.history_loading))
            HistoryState.Failed -> Hint(stringResource(R.string.state_unreachable))
            is HistoryState.Loaded -> if (s.empty) {
                Hint(stringResource(R.string.history_empty))
            } else {
                ChartGrid(
                    listOf(
                        ChartSpec(stringResource(R.string.metric_cpu), s.cpu, Palette.Cpu, Format::percent),
                        ChartSpec(stringResource(R.string.metric_memory), s.memory, Palette.Memory, Format::percent),
                        ChartSpec(stringResource(R.string.metric_gpu), s.gpu, Palette.Gpu, Format::percent),
                        ChartSpec(stringResource(R.string.metric_temp), s.temp, Palette.Thermal, Format::celsius),
                    ),
                )
            }
        }
    }
}

internal class ChartSpec(val title: String, val chart: Chart, val colour: Color, val format: (Float?) -> String)

/** Two charts per row, so four fit on one 1080p screen without scrolling. */
@Composable
internal fun ChartGrid(charts: List<ChartSpec>) {
    LazyVerticalGrid(
        GridCells.Fixed(2),
        horizontalArrangement = Arrangement.spacedBy(16.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
        contentPadding = PaddingValues(bottom = 24.dp),
    ) {
        items(charts, key = { it.title }) { c ->
            Panel(c.title, accent = c.colour) {
                Hint(if (c.chart.peak == null) Format.DASH else stringResource(R.string.history_peak, c.format(c.chart.peak)))
                HistoryCanvas(c.chart, c.colour, MaterialTheme.colorScheme.border.copy(alpha = 0.4f), height = 140.dp)
            }
        }
    }
}

@Composable
internal fun AboutTab(pairing: Pairing) {
    val graph = LocalContext.current.tvGraph
    var failed by remember { mutableStateOf(false) }
    val host by produceState<HostInfo?>(null, pairing.id) {
        value = (graph.client(pairing).host() as? ApiResult.Ok)?.value
        // A headless server has no host facts: say so rather than load forever.
        failed = value == null
    }
    val h = host
    if (h == null) {
        Hint(stringResource(if (failed) R.string.sensors_unavailable else R.string.device_loading))
        return
    }
    Row(horizontalArrangement = Arrangement.spacedBy(20.dp)) {
        LazyColumn(Modifier.weight(1f)) {
            item {
                Panel(h.hostname, accent = Palette.Accent) {
                    InfoRow(stringResource(R.string.host_os), "${h.osName} ${h.osVersion}")
                    InfoRow(stringResource(R.string.device_kernel), h.kernelVersion)
                    InfoRow(stringResource(R.string.host_started), relativeTime(h.bootTimeMs))
                    InfoRow(stringResource(R.string.host_board), h.motherboard ?: Format.DASH)
                    InfoRow(stringResource(R.string.host_bios), h.biosVersion ?: Format.DASH)
                    if (h.isVirtualMachine) InfoRow(stringResource(R.string.host_vm), stringResource(R.string.device_yes))
                }
            }
        }
        LazyColumn(Modifier.weight(1f)) {
            item {
                Panel(stringResource(R.string.host_cpu), accent = Palette.Cpu) {
                    InfoRow(stringResource(R.string.device_model), h.cpuModel)
                    InfoRow(stringResource(R.string.device_manufacturer), h.cpuVendor)
                    InfoRow(
                        stringResource(R.string.device_core_count),
                        pluralStringResource(R.plurals.device_cores_count, h.physicalCores, h.physicalCores) + ", " +
                            pluralStringResource(R.plurals.host_threads, h.logicalCores, h.logicalCores),
                    )
                    InfoRow("ABI", h.architecture)
                    InfoRow(stringResource(R.string.device_total_memory), Format.bytes(h.totalMemory))
                }
            }
        }
    }
}
