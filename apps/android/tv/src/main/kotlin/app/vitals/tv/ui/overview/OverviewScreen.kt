package app.vitals.tv.ui.overview

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Text
import app.vitals.core.model.Severity
import app.vitals.core.pairing.Pairing
import app.vitals.tv.LocalNavigator
import app.vitals.tv.R
import app.vitals.tv.Route
import app.vitals.tv.data.tvGraph
import app.vitals.tv.ui.FocusCard
import app.vitals.tv.ui.Hint
import app.vitals.tv.ui.Ring
import app.vitals.tv.ui.SafeHorizontal
import app.vitals.tv.ui.SafeVertical
import app.vitals.tv.ui.ScreenTitle
import app.vitals.tv.ui.Spark
import app.vitals.tv.ui.readable
import app.vitals.tv.ui.relativeTime
import app.vitals.ui.DeviceLive
import app.vitals.ui.Format
import app.vitals.ui.LiveState
import app.vitals.ui.Palette
import app.vitals.ui.Readings
import app.vitals.ui.Thresholds

/**
 * The home screen: this TV and every paired PC, each a focusable card with
 * its four headline rings. What a person glances at from the sofa; Enter on a
 * card opens the whole machine.
 */
@Composable
fun OverviewScreen() {
    val graph = LocalContext.current.tvGraph
    val navigator = LocalNavigator.current
    val pairings by graph.pairings.pairings.collectAsStateWithLifecycle()
    val ready by graph.ready.collectAsStateWithLifecycle()
    val live by graph.deviceLive.collectAsStateWithLifecycle(DeviceLive())

    LazyColumn(
        contentPadding = PaddingValues(horizontal = SafeHorizontal, vertical = SafeVertical),
        verticalArrangement = Arrangement.spacedBy(20.dp),
        modifier = Modifier.fillMaxSize(),
    ) {
        item { ScreenTitle(stringResource(R.string.overview_title)) }
        item { Text(stringResource(R.string.overview_tv), style = MaterialTheme.typography.titleLarge) }
        item { TvCard(live) { navigator.top(Route.ThisTv) } }
        item { Text(stringResource(R.string.overview_pcs), style = MaterialTheme.typography.titleLarge) }
        if (ready && pairings.isEmpty()) {
            item { Hint(stringResource(R.string.overview_no_pcs)) }
        } else {
            item {
                LazyRow(horizontalArrangement = Arrangement.spacedBy(20.dp), contentPadding = PaddingValues(vertical = 8.dp)) {
                    items(pairings, key = { it.id }) { p -> PcCard(p, Modifier.width(520.dp)) { navigator.push(Route.Pc(p.id)) } }
                }
            }
        }
    }
}

@Composable
private fun TvCard(live: DeviceLive, onOpen: () -> Unit) {
    val s = live.snapshot
    FocusCard(onClick = onOpen) {
        if (s == null) {
            Hint(stringResource(R.string.device_loading))
            return@FocusCard
        }
        val mem = if (s.memory.totalBytes > 0) s.memory.usedBytes * 100f / s.memory.totalBytes else null
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(40.dp), verticalAlignment = Alignment.CenterVertically) {
            Ring(s.cpu.load?.div(100f), Palette.Cpu, Format.percentCompact(s.cpu.load), stringResource(R.string.metric_cpu), level = Thresholds.cpu(s.cpu.load))
            Ring(mem?.div(100f), Palette.Memory, Format.percentCompact(mem), stringResource(R.string.metric_memory_short), level = Thresholds.memory(mem))
            Ring(s.gpu?.load?.div(100f), Palette.Gpu, Format.percentCompact(s.gpu?.load), stringResource(R.string.metric_gpu))
            Ring(
                s.cpu.temperature?.div(110f), Palette.Thermal, Format.celsiusCompact(s.cpu.temperature),
                stringResource(R.string.metric_temp), level = Thresholds.cpuTemp(s.cpu.temperature),
            )
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Spark(live.cpu, Palette.Cpu)
                Spark(live.memory, Palette.Memory)
                Text(
                    stringResource(R.string.net_down_up, Format.rate(s.network.rxBytesPerSec), Format.rate(s.network.txBytesPerSec)),
                    style = MaterialTheme.typography.bodyLarge,
                )
            }
        }
    }
}

@Composable
private fun PcCard(pairing: Pairing, modifier: Modifier, onOpen: () -> Unit) {
    val graph = LocalContext.current.tvGraph
    val flow = remember(pairing.id, pairing.baseUrl, pairing.token) { graph.live(pairing) }
    val state by flow.collectAsStateWithLifecycle(LiveState())
    FocusCard(onClick = onOpen, modifier = modifier) {
        Text(pairing.label, style = MaterialTheme.typography.titleLarge, maxLines = 1)
        PcStatus(state)
        val system = state.view?.system
        if (system != null && !state.unreachable) {
            val memory = Readings.memoryPercent(system)
            val gpu = Readings.primaryGpu(system)?.utilization
            val temp = Readings.cpuTemperature(system)
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                Ring(system.cpu.total / 100f, Palette.Cpu, Format.percentCompact(system.cpu.total), stringResource(R.string.metric_cpu), diameter = 96.dp, stroke = 10.dp, level = Thresholds.cpu(system.cpu.total))
                Ring(memory?.div(100f), Palette.Memory, Format.percentCompact(memory), stringResource(R.string.metric_memory_short), diameter = 96.dp, stroke = 10.dp, level = Thresholds.memory(memory))
                Ring(gpu?.div(100f), Palette.Gpu, Format.percentCompact(gpu), stringResource(R.string.metric_gpu), diameter = 96.dp, stroke = 10.dp)
                Ring(temp?.div(110f), Palette.Thermal, Format.celsiusCompact(temp), stringResource(R.string.metric_temp), diameter = 96.dp, stroke = 10.dp, level = Thresholds.cpuTemp(temp))
            }
            Spark(state.cpu, Palette.Cpu)
        }
    }
}

/** One line under a PC's name: why it is not showing numbers, or how many warnings it has. */
@Composable
fun PcStatus(state: LiveState) {
    val serious = state.alerts.count { it.severity != Severity.Info }
    val text = when {
        state.unauthorised -> stringResource(R.string.state_unauthorised)
        state.incompatible -> stringResource(R.string.state_incompatible)
        state.unreachable -> state.lastSeenMs?.let { stringResource(R.string.last_seen, relativeTime(it)) }
            ?: stringResource(R.string.state_unreachable)
        state.view == null -> stringResource(R.string.state_connecting)
        serious > 0 -> pluralStringResource(R.plurals.alert_count, serious, serious)
        else -> null
    } ?: return
    val critical = state.alerts.any { it.severity == Severity.Critical }
    val colour = when {
        serious > 0 && state.view != null && !state.unreachable -> readable(if (critical) Palette.Danger else Palette.Warn)
        else -> MaterialTheme.colorScheme.onSurfaceVariant
    }
    Text(text, style = MaterialTheme.typography.bodyLarge, color = colour)
}
