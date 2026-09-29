package app.vitals.phone.ui.detail

import androidx.annotation.StringRes
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import app.vitals.core.model.MachineSample
import app.vitals.core.net.ApiResult
import app.vitals.core.pairing.Pairing
import app.vitals.phone.R
import app.vitals.phone.graph
import app.vitals.phone.ui.components.SectionCard
import app.vitals.ui.Chart
import app.vitals.ui.Format
import app.vitals.ui.HistoryCanvas
import app.vitals.ui.Palette
import app.vitals.ui.chart

internal enum class Span(val seconds: Int, @param:StringRes val label: Int) {
    Hour(3_600, R.string.history_1h),
    Day(86_400, R.string.history_24h),
    Week(7 * 86_400, R.string.history_7d),
}

private sealed interface HistoryState {
    data object Loading : HistoryState
    data object Failed : HistoryState
    data class Loaded(val cpu: Chart, val memory: Chart, val gpu: Chart, val temp: Chart, val empty: Boolean) : HistoryState
}

@Composable
fun HistoryTab(pairing: Pairing, padding: PaddingValues) {
    val graph = LocalContext.current.graph
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

    LazyColumn(contentPadding = padding, verticalArrangement = Arrangement.spacedBy(12.dp), modifier = Modifier.fillMaxSize()) {
        item {
            SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
                Span.entries.forEach { s ->
                    SegmentedButton(
                        selected = span == s,
                        onClick = { span = s },
                        shape = SegmentedButtonDefaults.itemShape(s.ordinal, Span.entries.size),
                    ) { Text(stringResource(s.label)) }
                }
            }
        }
        when (val s = state) {
            HistoryState.Loading -> item { Text(stringResource(R.string.history_loading)) }
            HistoryState.Failed -> item { Text(stringResource(R.string.state_unreachable)) }
            is HistoryState.Loaded -> if (s.empty) {
                item { Text(stringResource(R.string.history_empty)) }
            } else {
                item(key = "cpu") { ChartCard(stringResource(R.string.metric_cpu), s.cpu, Palette.Cpu, Format::percent) }
                item(key = "mem") { ChartCard(stringResource(R.string.metric_memory), s.memory, Palette.Memory, Format::percent) }
                item(key = "gpu") { ChartCard(stringResource(R.string.metric_gpu), s.gpu, Palette.Gpu, Format::percent) }
                item(key = "temp") { ChartCard(stringResource(R.string.metric_temp), s.temp, Palette.Thermal, Format::celsius) }
            }
        }
    }
}

@Composable
internal fun ChartCard(title: String, chart: Chart, colour: Color, format: (Float?) -> String) {
    SectionCard(title, accent = colour) {
        Text(
            if (chart.peak == null) Format.DASH else stringResource(R.string.history_peak, format(chart.peak)),
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        HistoryCanvas(chart, colour, MaterialTheme.colorScheme.outlineVariant)
    }
}
