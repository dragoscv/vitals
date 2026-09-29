package app.vitals.tv.ui.device

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
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
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Text
import app.vitals.device.model.DeviceInfo
import app.vitals.device.model.DeviceSample
import app.vitals.device.model.DeviceSnapshot
import app.vitals.device.model.HardwareSensor
import app.vitals.tv.R
import app.vitals.tv.data.tvGraph
import app.vitals.tv.ui.Bar
import app.vitals.tv.ui.Choice
import app.vitals.tv.ui.ChoiceRow
import app.vitals.tv.ui.Hint
import app.vitals.tv.ui.InfoRow
import app.vitals.tv.ui.Panel
import app.vitals.tv.ui.Ring
import app.vitals.tv.ui.SafeHorizontal
import app.vitals.tv.ui.SafeVertical
import app.vitals.tv.ui.ScreenTitle
import app.vitals.tv.ui.Spark
import app.vitals.tv.ui.clusterRole
import app.vitals.tv.ui.deviceAlertTitle
import app.vitals.tv.ui.pc.ChartGrid
import app.vitals.tv.ui.pc.ChartSpec
import app.vitals.tv.ui.pc.Span
import app.vitals.tv.ui.readable
import app.vitals.tv.ui.relativeTime
import app.vitals.tv.ui.thermalStatus
import app.vitals.tv.ui.transport
import app.vitals.tv.ui.zoneGroupName
import app.vitals.ui.DeviceLive
import app.vitals.ui.Format
import app.vitals.ui.Level
import app.vitals.ui.Palette
import app.vitals.ui.Thresholds
import app.vitals.ui.chart
import java.util.Locale
import kotlin.math.roundToInt

private enum class TvTab(val label: Int) {
    Now(R.string.tab_now),
    Storage(R.string.device_tab_storage),
    Apps(R.string.device_tab_apps),
    Sensors(R.string.tab_sensors),
    History(R.string.tab_history),
    About(R.string.tab_about),
}

/**
 * The TV itself, through the same :device monitor the phone uses (ADR-0035):
 * official APIs only, so a reading the platform refuses is an em dash. A TV
 * has no battery; that tab is replaced by a line saying so on Now.
 */
@Composable
fun TvDeviceScreen() {
    var tab by rememberSaveable { mutableStateOf(TvTab.Now) }
    val graph = LocalContext.current.tvGraph
    val info by produceState<DeviceInfo?>(null) { value = graph.device.info() }
    Column(Modifier.fillMaxSize().padding(horizontal = SafeHorizontal, vertical = SafeVertical), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        ScreenTitle(info?.let { it.marketingName ?: "${it.manufacturer} ${it.model}" } ?: stringResource(R.string.nav_tv))
        ChoiceRow { TvTab.entries.forEach { t -> Choice(stringResource(t.label), selected = tab == t, onClick = { tab = t }) } }
        when (tab) {
            TvTab.Now -> NowTab()
            TvTab.Storage -> StorageTab()
            TvTab.Apps -> AppsTab()
            TvTab.Sensors -> SensorsTab()
            TvTab.History -> HistoryTab()
            TvTab.About -> AboutTab(info)
        }
    }
}

@Composable
private fun NowTab() {
    val graph = LocalContext.current.tvGraph
    val live by graph.deviceLive.collectAsStateWithLifecycle(DeviceLive())
    val alerts by graph.device.alerts.collectAsStateWithLifecycle()
    val s = live.snapshot
    if (s == null) {
        Hint(stringResource(R.string.device_loading))
        return
    }
    Row(horizontalArrangement = Arrangement.spacedBy(20.dp), modifier = Modifier.fillMaxSize()) {
        LazyColumn(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(16.dp), contentPadding = PaddingValues(bottom = 24.dp)) {
            if (alerts.isNotEmpty()) {
                item(key = "alerts") {
                    Panel(stringResource(R.string.alert_count_title), accent = Palette.Warn) {
                        alerts.forEach { a ->
                            Text(
                                stringResource(deviceAlertTitle(a.kind)),
                                style = MaterialTheme.typography.bodyLarge,
                                color = readable(if (a.severity == "critical") Palette.Danger else Palette.Warn),
                            )
                        }
                    }
                }
            }
            item(key = "hero") { Hero(live, s) }
            item(key = "cpu") { Cpu(s) }
        }
        LazyColumn(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(16.dp), contentPadding = PaddingValues(bottom = 24.dp)) {
            item(key = "mem") { Memory(s) }
            s.gpu?.let { g ->
                item(key = "gpu") {
                    Panel(stringResource(R.string.metric_gpu), accent = Palette.Gpu) {
                        InfoRow(stringResource(R.string.device_model), g.model ?: Format.DASH)
                        InfoRow(stringResource(R.string.device_load), Format.percent(g.load))
                        Bar(g.load?.div(100f), Palette.Gpu)
                        InfoRow(stringResource(R.string.cpu_clock), Format.ghz(g.frequencyHz))
                        InfoRow(stringResource(R.string.device_max_clock), Format.ghz(g.maxFrequencyHz))
                    }
                }
            }
            item(key = "thermal") { Thermal(s) }
            item(key = "net") { Network(s) }
            item(key = "power") {
                Panel(stringResource(R.string.metric_power), accent = Palette.Power) {
                    // A TV reports a "battery" that is not present; :device
                    // returns null for it, and this says why nothing is shown.
                    if (s.battery == null) {
                        InfoRow(stringResource(R.string.device_mains), "")
                        Hint(stringResource(R.string.device_mains_body))
                    } else {
                        InfoRow(stringResource(R.string.metric_battery), Format.percent(s.battery?.percent))
                    }
                }
            }
        }
    }
}

@Composable
private fun Hero(live: DeviceLive, s: DeviceSnapshot) {
    val mem = if (s.memory.totalBytes > 0) s.memory.usedBytes * 100f / s.memory.totalBytes else null
    Panel(stringResource(R.string.tab_now)) {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
            Ring(s.cpu.load?.div(100f), Palette.Cpu, Format.percentCompact(s.cpu.load), stringResource(R.string.metric_cpu), level = Thresholds.cpu(s.cpu.load))
            Ring(mem?.div(100f), Palette.Memory, Format.percentCompact(mem), stringResource(R.string.metric_memory_short), level = Thresholds.memory(mem))
            Ring(s.gpu?.load?.div(100f), Palette.Gpu, Format.percentCompact(s.gpu?.load), stringResource(R.string.metric_gpu))
            Ring(s.cpu.temperature?.div(110f), Palette.Thermal, Format.celsiusCompact(s.cpu.temperature), stringResource(R.string.metric_temp), level = Thresholds.cpuTemp(s.cpu.temperature))
        }
        Spark(live.cpu, Palette.Cpu)
        Spark(live.memory, Palette.Memory)
        if (live.temperature.last() != null) Spark(live.temperature, Palette.Thermal, max = 110f)
        if (!s.cpu.measured) Hint(stringResource(R.string.device_cpu_estimate))
    }
}

@Composable
private fun Cpu(s: DeviceSnapshot) {
    Panel(stringResource(R.string.metric_cpu), accent = Palette.Cpu) {
        InfoRow(stringResource(R.string.device_load), Format.percent(s.cpu.load))
        InfoRow(stringResource(R.string.cpu_temperature), Format.celsius(s.cpu.temperature))
        s.cpu.clusters.forEach { c ->
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    stringResource(clusterRole(c.role)) + " · " + pluralStringResource(R.plurals.device_cores_count, c.cores.size, c.cores.size),
                    style = MaterialTheme.typography.titleSmall,
                    modifier = Modifier.weight(1f),
                )
                Hint(Format.ghz(c.currentFrequencyHz) + " / " + Format.ghz(c.maxFrequencyHz))
            }
            Bar(c.load?.div(100f), Palette.Cpu)
        }
        Text(stringResource(R.string.device_each_core), style = MaterialTheme.typography.titleSmall)
        s.cpu.cores.chunked(4).forEach { row ->
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                row.forEach { core ->
                    val max = core.maxFrequencyHz
                    val f = core.frequencyHz
                    val frac = if (f != null && max != null && max > 0) f.toFloat() / max else null
                    Column(Modifier.weight(1f)) {
                        Hint(if (core.online) Format.ghz(f) else stringResource(R.string.device_core_offline))
                        Bar(frac, Palette.Cpu, height = 6.dp)
                    }
                }
                repeat(4 - row.size) { Spacer(Modifier.weight(1f)) }
            }
        }
    }
}

@Composable
private fun Memory(s: DeviceSnapshot) {
    val m = s.memory
    Panel(stringResource(R.string.device_total_memory), accent = Palette.Memory) {
        val pct = if (m.totalBytes > 0) m.usedBytes * 100f / m.totalBytes else null
        InfoRow(stringResource(R.string.memory_used_of, Format.bytes(m.usedBytes), Format.bytes(m.totalBytes)), Format.percent(pct))
        Bar(pct?.div(100f), Palette.Memory, height = 12.dp)
        InfoRow(stringResource(R.string.memory_available), Format.bytes(m.availableBytes))
        InfoRow(stringResource(R.string.memory_cached), Format.bytes(m.cachedBytes))
        val swapTotal = m.swapTotalBytes
        if (swapTotal != null) {
            InfoRow(stringResource(R.string.device_swap), Format.bytes(m.swapFreeBytes?.let { swapTotal - it }) + " / " + Format.bytes(swapTotal))
        }
        if (m.lowMemory) Text(stringResource(R.string.device_low_memory), color = readable(Palette.Warn))
    }
}

@Composable
private fun Thermal(s: DeviceSnapshot) {
    val t = s.thermal
    Panel(stringResource(R.string.device_thermal), accent = Palette.Thermal) {
        val level = when (t.status) {
            "none", "light" -> Level.Ok
            "moderate" -> Level.Warn
            else -> Level.Danger
        }
        InfoRow(stringResource(R.string.device_throttling), stringResource(thermalStatus(t.status)), valueColour = Palette.of(level))
        t.headroom?.let {
            InfoRow(stringResource(R.string.device_headroom), Format.percent((it * 100f).coerceAtMost(100f)))
            Bar(it.coerceIn(0f, 1f), Palette.Thermal)
        }
        t.zones.groupBy { it.group }
            .map { (group, zones) -> group to zones.maxOf { it.celsius } }
            .sortedByDescending { it.second }
            .forEach { (group, max) -> InfoRow(stringResource(zoneGroupName(group)), Format.celsius(max), valueColour = Palette.of(Thresholds.cpuTemp(max))) }
        if (t.zones.isEmpty()) Hint(stringResource(R.string.device_zones_hidden))
    }
}

@Composable
private fun Network(s: DeviceSnapshot) {
    val n = s.network
    Panel(stringResource(R.string.metric_network), accent = Palette.Network) {
        InfoRow(stringResource(R.string.device_connection), stringResource(transport(n.transport)))
        Text(stringResource(R.string.net_down_up, Format.rate(n.rxBytesPerSec), Format.rate(n.txBytesPerSec)), style = MaterialTheme.typography.bodyLarge)
        n.wifiRssiDbm?.let { InfoRow(stringResource(R.string.device_signal), "$it dBm") }
        n.wifiLinkMbps?.let { InfoRow(stringResource(R.string.device_link_speed), "$it Mb/s") }
        n.wifiFrequencyMhz?.let { InfoRow(stringResource(R.string.device_band), if (it >= 5900) "6 GHz" else if (it >= 4900) "5 GHz" else "2.4 GHz") }
        n.wifiStandard?.let { InfoRow(stringResource(R.string.device_standard), it) }
        n.metered?.let { InfoRow(stringResource(R.string.device_metered), stringResource(if (it) R.string.device_yes else R.string.device_no)) }
        n.ipv4?.let { InfoRow("IPv4", it) }
        InfoRow(stringResource(R.string.device_since_boot), Format.bytes(n.rxTotalBytes) + " ↓ · " + Format.bytes(n.txTotalBytes) + " ↑")
    }
}

@Composable
private fun SensorsTab() {
    val graph = LocalContext.current.tvGraph
    val live by graph.deviceLive.collectAsStateWithLifecycle(DeviceLive())
    val sensors by produceState<List<HardwareSensor>?>(null) { value = graph.device.hardwareSensors() }
    val zones = live.snapshot?.thermal?.zones.orEmpty()
    // Read through the configuration so a language change recomposes this.
    val locale = LocalConfiguration.current.locales[0]
    LazyVerticalGrid(
        GridCells.Fixed(3),
        horizontalArrangement = Arrangement.spacedBy(16.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
        contentPadding = PaddingValues(bottom = 24.dp),
        modifier = Modifier.fillMaxSize(),
    ) {
        zones.groupBy { it.group }.entries.sortedByDescending { e -> e.value.maxOf { it.celsius } }.forEach { (group, list) ->
            item(key = "z-$group") {
                Panel(stringResource(zoneGroupName(group)), accent = Palette.Thermal) {
                    list.sortedByDescending { it.celsius }.forEach { z -> InfoRow(z.name, Format.celsius(z.celsius), valueColour = Palette.of(Thresholds.cpuTemp(z.celsius))) }
                }
            }
        }
        val hw = sensors
        if (hw == null) {
            item(key = "loading") { Hint(stringResource(R.string.device_loading)) }
        } else {
            item(key = "hw-title", span = { androidx.compose.foundation.lazy.grid.GridItemSpan(maxLineSpan) }) {
                Text(stringResource(R.string.device_hardware_sensors, hw.size), style = MaterialTheme.typography.titleLarge)
            }
            items(hw, key = { "s-" + it.type + it.name }) { s ->
                Panel(s.name, accent = Palette.Accent) {
                    Hint(s.vendor + " · " + s.type.substringAfterLast('.'))
                    InfoRow(stringResource(R.string.device_value), values(s.values, s.unit, locale))
                    InfoRow(stringResource(R.string.device_sensor_power), String.format(locale, "%.2f mA", s.powerMa))
                }
            }
        }
    }
}

private fun values(v: List<Float>?, unit: String?, locale: Locale): String {
    if (v.isNullOrEmpty()) return Format.DASH
    val shown = v.take(3).joinToString(" · ") { String.format(locale, "%.2f", it) }
    return if (unit != null) "$shown $unit" else shown
}

@Composable
private fun HistoryTab() {
    val graph = LocalContext.current.tvGraph
    val recording by graph.deviceHistory.collectAsStateWithLifecycle(true)
    var span by rememberSaveable { mutableStateOf(Span.Day) }
    var samples by remember { mutableStateOf<List<DeviceSample>?>(null) }
    LaunchedEffect(span) {
        samples = null
        samples = graph.device.history(System.currentTimeMillis() - span.seconds * 1000L).sortedBy { it.ts }
    }
    Column(verticalArrangement = Arrangement.spacedBy(16.dp)) {
        ChoiceRow { Span.entries.forEach { s -> Choice(stringResource(s.label), selected = span == s, onClick = { span = s }) } }
        val s = samples
        when {
            s == null -> Hint(stringResource(R.string.device_loading))
            s.isEmpty() -> Hint(stringResource(if (recording) R.string.device_history_empty else R.string.device_history_off))
            else -> ChartGrid(
                listOf(
                    ChartSpec(stringResource(R.string.metric_cpu), chart(s, 100f) { it.cpuLoad }, Palette.Cpu, Format::percent),
                    ChartSpec(stringResource(R.string.metric_memory), chart(s, 100f) { it.memoryUsedPercent }, Palette.Memory, Format::percent),
                    ChartSpec(stringResource(R.string.cpu_temperature), chart(s, 110f) { it.cpuTempC }, Palette.Thermal, Format::celsius),
                    ChartSpec(stringResource(R.string.metric_gpu), chart(s, 100f) { it.gpuLoad }, Palette.Gpu, Format::percent),
                ),
            )
        }
    }
}

@Composable
private fun AboutTab(info: DeviceInfo?) {
    if (info == null) {
        Hint(stringResource(R.string.device_loading))
        return
    }
    val grid = listOf<@Composable () -> Unit>(
        {
            Panel(stringResource(R.string.tab_about), accent = Palette.Accent) {
                InfoRow(stringResource(R.string.device_manufacturer), info.manufacturer)
                InfoRow(stringResource(R.string.device_model), info.model)
                InfoRow(stringResource(R.string.device_codename), info.device)
                InfoRow("Android", "${info.androidVersion} (API ${info.sdkInt})")
                InfoRow(stringResource(R.string.device_security_patch), info.securityPatch ?: Format.DASH)
                InfoRow(stringResource(R.string.device_kernel), info.kernel ?: Format.DASH)
                InfoRow(stringResource(R.string.device_started), relativeTime(info.bootTimeMs))
            }
        },
        {
            Panel(stringResource(R.string.device_chip), accent = Palette.Cpu) {
                InfoRow(stringResource(R.string.device_soc), listOfNotNull(info.socManufacturer, info.soc).joinToString(" ").ifEmpty { Format.DASH })
                InfoRow(stringResource(R.string.device_core_count), info.cpuCores.toString())
                InfoRow("ABI", info.abis.joinToString(", "))
                InfoRow(stringResource(R.string.device_total_memory), Format.bytes(info.totalMemoryBytes))
                InfoRow("OpenGL ES", info.glEsVersion ?: Format.DASH)
                InfoRow("Vulkan", stringResource(if (info.vulkan) R.string.device_yes else R.string.device_no))
                if (info.mediaPerformanceClass > 0) InfoRow(stringResource(R.string.device_perf_class), info.mediaPerformanceClass.toString())
                if (info.isLowRam) InfoRow(stringResource(R.string.device_low_ram), stringResource(R.string.device_yes))
            }
        },
        {
            Panel(stringResource(R.string.device_display), accent = Palette.Network) {
                InfoRow(stringResource(R.string.device_resolution), "${info.displayWidthPx} × ${info.displayHeightPx}")
                InfoRow(stringResource(R.string.device_density), "${info.displayDensityDpi} dpi")
                InfoRow(stringResource(R.string.device_refresh), info.refreshRatesHz.joinToString(" / ") { "${it.roundToInt()} Hz" }.ifEmpty { Format.DASH })
                InfoRow(stringResource(R.string.device_hdr), info.hdr.joinToString(", ").ifEmpty { Format.DASH })
            }
        },
    )
    LazyVerticalGrid(GridCells.Fixed(3), horizontalArrangement = Arrangement.spacedBy(16.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        items(grid.size) { i -> grid[i]() }
    }
}
