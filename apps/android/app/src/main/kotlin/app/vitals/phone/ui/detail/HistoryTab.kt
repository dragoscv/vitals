package app.vitals.phone.ui.detail

import androidx.annotation.StringRes
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import app.vitals.core.model.MachineSample
import app.vitals.core.net.ApiResult
import app.vitals.core.pairing.Pairing
import app.vitals.phone.R
import app.vitals.phone.graph
import app.vitals.phone.ui.components.SectionCard
import app.vitals.ui.Format
import app.vitals.ui.Palette

internal enum class Span(val seconds: Int, @param:StringRes val label: Int) {
    Hour(3_600, R.string.history_1h),
    Day(86_400, R.string.history_24h),
    Week(7 * 86_400, R.string.history_7d),
}

/**
 * One chart's points, bucketed to at most [MAX_POINTS] so a week of
 * one-second samples does not become 600 000 path segments on a phone.
 * `NaN` is a bucket where the metric was never measured.
 */
@Immutable
internal class Chart(val points: FloatArray, val max: Float, val peak: Float?)

private const val MAX_POINTS = 240

internal fun <T> chart(samples: List<T>, max: Float, pick: (T) -> Float?): Chart {
    if (samples.isEmpty()) return Chart(FloatArray(0), max, null)
    val buckets = minOf(MAX_POINTS, samples.size)
    val out = FloatArray(buckets) { Float.NaN }
    val per = samples.size.toFloat() / buckets
    var peak: Float? = null
    for (b in 0 until buckets) {
        val from = (b * per).toInt()
        val to = ((b + 1) * per).toInt().coerceAtMost(samples.size).coerceAtLeast(from + 1)
        var sum = 0f
        var n = 0
        for (i in from until to) {
            val v = pick(samples[i]) ?: continue
            sum += v
            n++
            if (peak == null || v > peak) peak = v
        }
        if (n > 0) out[b] = sum / n
    }
    return Chart(out, max, peak)
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
        val path = remember { Path() }
        val grid = MaterialTheme.colorScheme.outlineVariant
        Canvas(Modifier.fillMaxWidth().height(120.dp)) {
            for (i in 1..3) {
                val y = size.height * i / 4
                drawLine(grid, androidx.compose.ui.geometry.Offset(0f, y), androidx.compose.ui.geometry.Offset(size.width, y), 1f)
            }
            val pts = chart.points
            if (pts.size < 2) return@Canvas
            path.rewind()
            val step = size.width / (pts.size - 1)
            var penDown = false
            for (i in pts.indices) {
                val v = pts[i]
                if (v.isNaN()) {
                    penDown = false
                    continue
                }
                val x = i * step
                val y = size.height * (1f - (v / chart.max).coerceIn(0f, 1f))
                if (penDown) path.lineTo(x, y) else path.moveTo(x, y)
                penDown = true
            }
            drawPath(path, colour, style = Stroke(width = 2.dp.toPx(), cap = StrokeCap.Round))
        }
    }
}
