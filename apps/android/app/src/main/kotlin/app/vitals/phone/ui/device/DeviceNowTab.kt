package app.vitals.phone.ui.device

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.vitals.device.model.DeviceSnapshot
import app.vitals.phone.R
import app.vitals.ui.DeviceLive
import app.vitals.phone.graph
import app.vitals.phone.ui.components.FillBar
import app.vitals.phone.ui.components.InfoRow
import app.vitals.phone.ui.components.RingGauge
import app.vitals.phone.ui.components.SectionCard
import app.vitals.phone.ui.components.Sparkline
import app.vitals.phone.ui.theme.readable
import app.vitals.ui.Format
import app.vitals.ui.Level
import app.vitals.ui.Palette
import app.vitals.ui.Thresholds

@Composable
internal fun DeviceNowTab(padding: PaddingValues) {
    val graph = LocalContext.current.graph
    val live by graph.deviceLive.live.collectAsStateWithLifecycle(DeviceLive())
    val alerts by graph.device.alerts.collectAsStateWithLifecycle()
    val s = live.snapshot
    LazyColumn(contentPadding = padding, verticalArrangement = Arrangement.spacedBy(12.dp), modifier = Modifier.fillMaxSize()) {
        if (s == null) {
            item { SectionCard(stringResource(R.string.state_connecting)) { } }
            return@LazyColumn
        }
        if (alerts.isNotEmpty()) {
            item(key = "alerts") {
                SectionCard(stringResource(R.string.alert_count_title), accent = Palette.Warn) {
                    alerts.forEach { a ->
                        Text(
                            stringResource(deviceAlertTitle(a.kind)),
                            color = readable(if (a.severity == "critical") Palette.Danger else Palette.Warn),
                        )
                    }
                }
            }
        }
        item(key = "hero") { Hero(live, s) }
        item(key = "cpu") { Cpu(s) }
        s.gpu?.let { g ->
            item(key = "gpu") {
                SectionCard(stringResource(R.string.metric_gpu), accent = Palette.Gpu) {
                    InfoRow(stringResource(R.string.device_model), g.model ?: Format.DASH)
                    InfoRow(stringResource(R.string.device_load), Format.percent(g.load))
                    FillBar(g.load?.div(100f), Palette.Gpu)
                    InfoRow(stringResource(R.string.cpu_clock), Format.ghz(g.frequencyHz))
                    InfoRow(stringResource(R.string.device_max_clock), Format.ghz(g.maxFrequencyHz))
                }
            }
        }
        item(key = "mem") { Memory(s) }
        item(key = "thermal") { Thermal(s) }
        item(key = "net") { Network(s) }
    }
}

@Composable
private fun Hero(live: DeviceLive, s: DeviceSnapshot) {
    val mem = s.memory.usedBytes * 100f / s.memory.totalBytes
    SectionCard(stringResource(R.string.tab_now)) {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
            RingGauge(
                s.cpu.load?.div(100f), Palette.Cpu, Format.percentCompact(s.cpu.load),
                stringResource(R.string.metric_cpu), diameter = 76.dp, level = Thresholds.cpu(s.cpu.load),
            )
            RingGauge(
                mem / 100f, Palette.Memory, Format.percentCompact(mem),
                stringResource(R.string.metric_memory_short), diameter = 76.dp, level = Thresholds.memory(mem),
            )
            RingGauge(
                s.battery?.percent?.div(100f), Palette.Power, Format.percentCompact(s.battery?.percent),
                stringResource(R.string.metric_battery), diameter = 76.dp,
                level = s.battery?.percent?.let { if (it <= 5f) Level.Danger else if (it <= 15f) Level.Warn else Level.Ok } ?: Level.Ok,
            )
            RingGauge(
                s.cpu.temperature?.div(110f), Palette.Thermal, Format.celsiusCompact(s.cpu.temperature),
                stringResource(R.string.metric_temp), diameter = 76.dp, level = Thresholds.cpuTemp(s.cpu.temperature),
            )
        }
        Sparkline(live.cpu, Palette.Cpu)
        Sparkline(live.memory, Palette.Memory)
        if (live.gpu.size > 0) Sparkline(live.gpu, Palette.Gpu)
        if (live.temperature.size > 0) Sparkline(live.temperature, Palette.Thermal, max = 110f)
        if (!s.cpu.measured) {
            Text(
                stringResource(R.string.device_cpu_estimate),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
private fun Cpu(s: DeviceSnapshot) {
    SectionCard(stringResource(R.string.metric_cpu), accent = Palette.Cpu) {
        InfoRow(stringResource(R.string.device_load), Format.percent(s.cpu.load))
        InfoRow(stringResource(R.string.cpu_temperature), Format.celsius(s.cpu.temperature))
        s.cpu.clusters.forEach { c ->
            Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        stringResource(clusterRole(c.role)) + " · " + stringResource(R.string.device_cores_count, c.cores.size),
                        style = MaterialTheme.typography.labelLarge,
                        modifier = Modifier.weight(1f),
                    )
                    Text(
                        Format.ghz(c.currentFrequencyHz) + " / " + Format.ghz(c.maxFrequencyHz),
                        style = MaterialTheme.typography.labelMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                FillBar(c.load?.div(100f), Palette.Cpu)
            }
        }
        Text(stringResource(R.string.device_each_core), style = MaterialTheme.typography.labelLarge)
        s.cpu.cores.chunked(4).forEach { row ->
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                row.forEach { core ->
                    val frac = if (core.frequencyHz != null && core.maxFrequencyHz != null && core.maxFrequencyHz!! > 0) {
                        core.frequencyHz!!.toFloat() / core.maxFrequencyHz!!
                    } else {
                        null
                    }
                    Column(Modifier.weight(1f)) {
                        Text(
                            if (core.online) Format.ghz(core.frequencyHz) else stringResource(R.string.device_core_offline),
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                        FillBar(frac, Palette.Cpu, height = 4.dp)
                    }
                }
                repeat(4 - row.size) { androidx.compose.foundation.layout.Spacer(Modifier.weight(1f).width(0.dp)) }
            }
        }
    }
}

@Composable
private fun Memory(s: DeviceSnapshot) {
    val m = s.memory
    SectionCard(stringResource(R.string.device_total_memory), accent = Palette.Memory) {
        InfoRow(stringResource(R.string.memory_used_of, Format.bytes(m.usedBytes), Format.bytes(m.totalBytes)), Format.percent(m.usedBytes * 100f / m.totalBytes))
        FillBar(m.usedBytes.toFloat() / m.totalBytes, Palette.Memory)
        InfoRow(stringResource(R.string.memory_available), Format.bytes(m.availableBytes))
        InfoRow(stringResource(R.string.memory_cached), Format.bytes(m.cachedBytes))
        if (m.swapTotalBytes != null) {
            InfoRow(
                stringResource(R.string.device_swap),
                Format.bytes(m.swapFreeBytes?.let { m.swapTotalBytes!! - it }) + " / " + Format.bytes(m.swapTotalBytes),
            )
        }
        if (m.lowMemory) Text(stringResource(R.string.device_low_memory), color = readable(Palette.Warn))
    }
}

@Composable
private fun Thermal(s: DeviceSnapshot) {
    val t = s.thermal
    SectionCard(stringResource(R.string.device_thermal), accent = Palette.Thermal) {
        val level = when (t.status) {
            "none", "light" -> Level.Ok
            "moderate" -> Level.Warn
            else -> Level.Danger
        }
        InfoRow(stringResource(R.string.device_throttling), stringResource(thermalStatus(t.status)), valueColour = Palette.of(level))
        t.headroom?.let {
            InfoRow(stringResource(R.string.device_headroom), Format.percent((it * 100f).coerceAtMost(100f)))
            FillBar(it.coerceIn(0f, 1f), Palette.Thermal)
        }
        t.zones.groupBy { it.group }
            .map { (group, zones) -> group to zones.maxOf { it.celsius } }
            .sortedByDescending { it.second }
            .forEach { (group, max) ->
                InfoRow(stringResource(zoneGroupName(group)), Format.celsius(max), valueColour = Palette.of(Thresholds.cpuTemp(max)))
            }
        if (t.zones.isEmpty()) {
            Text(
                stringResource(R.string.device_zones_hidden),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
private fun Network(s: DeviceSnapshot) {
    val n = s.network
    SectionCard(stringResource(R.string.metric_network), accent = Palette.Network) {
        InfoRow(stringResource(R.string.device_connection), stringResource(transport(n.transport)))
        InfoRow(stringResource(R.string.net_down_up, Format.rate(n.rxBytesPerSec), Format.rate(n.txBytesPerSec)), "")
        n.wifiRssiDbm?.let { InfoRow(stringResource(R.string.device_signal), "$it dBm") }
        n.wifiLinkMbps?.let { InfoRow(stringResource(R.string.device_link_speed), "$it Mb/s") }
        n.wifiFrequencyMhz?.let { InfoRow(stringResource(R.string.device_band), if (it >= 5900) "6 GHz" else if (it >= 4900) "5 GHz" else "2.4 GHz") }
        n.wifiStandard?.let { InfoRow(stringResource(R.string.device_standard), it) }
        n.cellularGeneration?.let { InfoRow(stringResource(R.string.device_generation), it) }
        n.cellularSignalDbm?.let { InfoRow(stringResource(R.string.device_signal), "$it dBm") }
        n.operator?.let { InfoRow(stringResource(R.string.device_operator), it) }
        n.metered?.let { InfoRow(stringResource(R.string.device_metered), stringResource(if (it) R.string.device_yes else R.string.device_no)) }
        n.ipv4?.let { InfoRow("IPv4", it) }
        n.ipv6?.let { InfoRow("IPv6", it) }
        InfoRow(stringResource(R.string.device_since_boot), Format.bytes(n.rxTotalBytes) + " ↓ · " + Format.bytes(n.txTotalBytes) + " ↑")
    }
}
