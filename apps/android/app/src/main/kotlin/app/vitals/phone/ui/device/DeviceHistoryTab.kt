package app.vitals.phone.ui.device

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.lazy.LazyColumn
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.vitals.device.model.DeviceSample
import app.vitals.phone.R
import app.vitals.phone.graph
import app.vitals.phone.ui.detail.ChartCard
import app.vitals.phone.ui.detail.Span
import app.vitals.phone.ui.detail.chart
import app.vitals.ui.Format
import app.vitals.ui.Palette

@Composable
internal fun DeviceHistoryTab(padding: PaddingValues) {
    val graph = LocalContext.current.graph
    val recording by graph.settings.deviceHistory.collectAsStateWithLifecycle(true)
    var span by rememberSaveable { mutableStateOf(Span.Day) }
    var samples by remember { mutableStateOf<List<DeviceSample>?>(null) }

    LaunchedEffect(span) {
        samples = null
        samples = graph.device.history(System.currentTimeMillis() - span.seconds * 1000L).sortedBy { it.ts }
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
        val s = samples
        when {
            s == null -> item { Text(stringResource(R.string.device_loading)) }
            s.isEmpty() -> item {
                Text(stringResource(if (recording) R.string.device_history_empty else R.string.device_history_off))
            }
            else -> {
                item(key = "cpu") { ChartCard(stringResource(R.string.metric_cpu), chart(s, 100f) { it.cpuLoad }, Palette.Cpu, Format::percent) }
                item(key = "bat") { ChartCard(stringResource(R.string.metric_battery), chart(s, 100f) { it.batteryPercent }, Palette.Power, Format::percent) }
                item(key = "mem") { ChartCard(stringResource(R.string.metric_memory), chart(s, 100f) { it.memoryUsedPercent }, Palette.Memory, Format::percent) }
                item(key = "temp") { ChartCard(stringResource(R.string.cpu_temperature), chart(s, 110f) { it.cpuTempC }, Palette.Thermal, Format::celsius) }
                item(key = "btemp") {
                    ChartCard(stringResource(R.string.device_battery_temp), chart(s, 60f) { it.batteryTempC }, Palette.Thermal, Format::celsius)
                }
                item(key = "gpu") { ChartCard(stringResource(R.string.metric_gpu), chart(s, 100f) { it.gpuLoad }, Palette.Gpu, Format::percent) }
            }
        }
    }
}
