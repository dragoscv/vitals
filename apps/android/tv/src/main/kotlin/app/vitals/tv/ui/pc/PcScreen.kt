package app.vitals.tv.ui.pc

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Text
import app.vitals.core.model.Severity
import app.vitals.core.model.SystemMetrics
import app.vitals.core.net.WakeOnLan
import app.vitals.core.pairing.Pairing
import app.vitals.tv.R
import app.vitals.tv.data.tvGraph
import app.vitals.tv.ui.Action
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
import app.vitals.tv.ui.overview.PcStatus
import app.vitals.tv.ui.readable
import app.vitals.tv.ui.title
import app.vitals.tv.ui.words
import app.vitals.ui.Format
import app.vitals.ui.LiveState
import app.vitals.ui.Palette
import app.vitals.ui.Readings
import app.vitals.ui.Thresholds

private enum class PcTab(val label: Int) {
    Now(R.string.tab_now),
    Programs(R.string.tab_programs),
    Sensors(R.string.tab_sensors),
    History(R.string.tab_history),
    About(R.string.tab_about),
}

/**
 * One PC, the desktop's main screens laid out for 1080p: the readings in two
 * columns rather than one long phone scroll, the tabs as a row the D-pad
 * walks left and right.
 */
@Composable
fun PcScreen(pairing: Pairing) {
    val graph = LocalContext.current.tvGraph
    val flow = remember(pairing.id, pairing.baseUrl, pairing.token) { graph.live(pairing) }
    val state by flow.collectAsStateWithLifecycle(LiveState())
    var tab by rememberSaveable { mutableStateOf(PcTab.Now) }

    Column(Modifier.fillMaxSize().padding(horizontal = SafeHorizontal, vertical = SafeVertical), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(20.dp)) {
            Column(Modifier.weight(1f)) {
                ScreenTitle(pairing.label)
                PcStatus(state)
            }
            WakeButton(pairing)
            if (state.unreachable) Action(stringResource(R.string.reconnect), onClick = { graph.kick(pairing.id) })
        }
        ChoiceRow {
            PcTab.entries.forEach { t -> Choice(stringResource(t.label), selected = tab == t, onClick = { tab = t }) }
        }
        when (tab) {
            PcTab.Now -> NowTab(state)
            PcTab.Programs -> ProgramsTab(pairing, state)
            PcTab.Sensors -> SensorsTab(pairing)
            PcTab.History -> HistoryTab(pairing)
            PcTab.About -> AboutTab(pairing)
        }
    }
}

@Composable
private fun WakeButton(pairing: Pairing) {
    val resources = LocalResources.current
    var request by remember { mutableIntStateOf(0) }
    var message by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(request) {
        if (request == 0) return@LaunchedEffect
        val mac = pairing.mac
        message = resources.getString(
            when {
                mac == null -> R.string.wake_no_mac
                runCatching { WakeOnLan.send(mac, pairing.broadcast) }.getOrDefault(false) -> R.string.wake_sent
                else -> R.string.wake_failed
            },
        )
    }
    Column(horizontalAlignment = Alignment.End) {
        Action(stringResource(R.string.wake), onClick = { request++ })
        message?.let { Hint(it) }
    }
}

@Composable
private fun NowTab(state: LiveState) {
    val system = state.view?.system
    if (state.unreachable || system == null) {
        Panel(stringResource(if (state.unreachable) R.string.state_unreachable else R.string.state_connecting)) {
            if (state.unreachable) Hint(stringResource(R.string.state_unreachable_hint))
        }
        return
    }
    // Two columns: a 1080p TV is wide and short, and a single phone-style
    // column would leave two thirds of the screen empty.
    Row(horizontalArrangement = Arrangement.spacedBy(20.dp), modifier = Modifier.fillMaxSize()) {
        LazyColumn(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(16.dp), contentPadding = PaddingValues(bottom = 24.dp)) {
            if (state.alerts.isNotEmpty()) {
                item(key = "alerts") {
                    Panel(stringResource(R.string.alert_count_title), accent = Palette.Warn) {
                        state.alerts.forEach { a ->
                            Text(
                                stringResource(a.kind.title()) + if (a.subject.isNotEmpty()) " · ${a.subject}" else "",
                                style = MaterialTheme.typography.bodyLarge,
                                color = readable(if (a.severity == Severity.Critical) Palette.Danger else Palette.Warn),
                            )
                        }
                    }
                }
            }
            item(key = "hero") { Hero(state, system) }
            cpu(system)
            memory(system)
        }
        LazyColumn(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(16.dp), contentPadding = PaddingValues(bottom = 24.dp)) {
            gpus(system)
            disks(system)
            networks(system)
            extras(system)
        }
    }
}

@Composable
private fun Hero(state: LiveState, system: SystemMetrics) {
    val memory = Readings.memoryPercent(system)
    val gpu = Readings.primaryGpu(system)?.utilization
    val temp = Readings.cpuTemperature(system)
    Panel(stringResource(R.string.tab_now)) {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
            Ring(system.cpu.total / 100f, Palette.Cpu, Format.percentCompact(system.cpu.total), stringResource(R.string.metric_cpu), level = Thresholds.cpu(system.cpu.total))
            Ring(memory?.div(100f), Palette.Memory, Format.percentCompact(memory), stringResource(R.string.metric_memory_short), level = Thresholds.memory(memory))
            Ring(gpu?.div(100f), Palette.Gpu, Format.percentCompact(gpu), stringResource(R.string.metric_gpu))
            Ring(temp?.div(110f), Palette.Thermal, Format.celsiusCompact(temp), stringResource(R.string.metric_temp), level = Thresholds.cpuTemp(temp))
        }
        Spark(state.cpu, Palette.Cpu)
        Spark(state.memory, Palette.Memory)
        if (state.gpu.size > 0) Spark(state.gpu, Palette.Gpu)
        if (state.temperature.size > 0) Spark(state.temperature, Palette.Thermal, max = 110f)
    }
}

private fun LazyListScope.cpu(system: SystemMetrics) = item(key = "cpu") {
    val cpu = system.cpu
    Panel(stringResource(R.string.metric_cpu), accent = Palette.Cpu) {
        InfoRow(stringResource(R.string.cpu_clock), Format.ghz(cpu.effectiveClock))
        InfoRow(stringResource(R.string.cpu_temperature), Format.celsius(cpu.temperature), valueColour = Palette.of(Thresholds.cpuTemp(cpu.temperature)))
        InfoRow(stringResource(R.string.cpu_power), Format.watts(cpu.power))
        val throttle = cpu.throttled
        Hint(if (throttle == null) stringResource(R.string.cpu_not_throttled) else stringResource(R.string.cpu_throttled) + " " + stringResource(throttle.words()))
        InfoRow(stringResource(R.string.cpu_processes), cpu.processCount.toString())
        InfoRow(stringResource(R.string.cpu_uptime), Format.duration(cpu.uptimeSecs))
        Text(stringResource(R.string.cpu_cores), style = MaterialTheme.typography.titleSmall)
        // Four columns: a 32-thread CPU is eight short rows on a TV.
        cpu.perCore.chunked(4).forEach { row ->
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                row.forEach { load -> Bar(load / 100f, Palette.Cpu, Modifier.weight(1f)) }
                repeat(4 - row.size) { androidx.compose.foundation.layout.Spacer(Modifier.weight(1f)) }
            }
        }
    }
}

private fun LazyListScope.memory(system: SystemMetrics) = item(key = "memory") {
    val m = system.memory
    Panel(stringResource(R.string.metric_memory), accent = Palette.Memory) {
        Bar(Readings.memoryPercent(system)?.div(100f), Palette.Memory, height = 12.dp)
        Text(stringResource(R.string.memory_used_of, Format.bytes(m.used), Format.bytes(m.total)), style = MaterialTheme.typography.bodyLarge)
        InfoRow(stringResource(R.string.memory_available), Format.bytes(m.available))
        InfoRow(stringResource(R.string.memory_cached), Format.bytes(m.cached))
        InfoRow(stringResource(R.string.memory_committed), stringResource(R.string.memory_used_of, Format.bytes(m.committed), Format.bytes(m.commitLimit)))
    }
}

private fun LazyListScope.gpus(system: SystemMetrics) {
    system.gpus.forEach { g ->
        item(key = "gpu-${g.id}") {
            Panel(g.name, accent = Palette.Gpu) {
                Bar(g.utilization?.div(100f), Palette.Gpu, height = 12.dp)
                InfoRow(stringResource(R.string.metric_gpu), Format.percent(g.utilization))
                val vram = if (g.memoryUsed != null && g.memoryTotal != null) {
                    stringResource(R.string.memory_used_of, Format.bytes(g.memoryUsed), Format.bytes(g.memoryTotal))
                } else {
                    Format.bytes(g.memoryUsed)
                }
                InfoRow(stringResource(R.string.gpu_memory), vram)
                InfoRow(stringResource(R.string.gpu_temperature), Format.celsius(g.temperature), valueColour = Palette.of(Thresholds.gpuTemp(g.temperature)))
                InfoRow(stringResource(R.string.gpu_power), Format.watts(g.power))
                InfoRow(stringResource(R.string.gpu_fan), g.fanRpm?.let(Format::rpm) ?: Format.percent(g.fanPercent))
                g.throttled?.let { Text(stringResource(R.string.cpu_throttled) + " " + stringResource(it.words()), color = readable(Palette.Warn)) }
            }
        }
    }
}

private fun LazyListScope.disks(system: SystemMetrics) {
    if (system.disks.isEmpty()) return
    item(key = "disks") {
        Panel(stringResource(R.string.metric_disk), accent = Palette.Disk) {
            system.disks.forEach { d ->
                Text(listOfNotNull(d.mount, d.model ?: d.name).joinToString(" · "), style = MaterialTheme.typography.titleSmall)
                if (d.total > 0) {
                    Bar((d.total - d.free).toFloat() / d.total, Palette.Disk)
                    Hint(stringResource(R.string.disk_free, Format.bytes(d.free), Format.bytes(d.total)))
                }
                InfoRow(stringResource(R.string.disk_activity), Format.percent(d.activeTime))
                Hint(stringResource(R.string.disk_read_write, Format.rate(d.read), Format.rate(d.write)))
                d.temperature?.let { InfoRow(stringResource(R.string.cpu_temperature), Format.celsius(it), valueColour = Palette.of(Thresholds.diskTemp(it))) }
                d.health?.let { h ->
                    InfoRow(stringResource(R.string.disk_life), Format.percent(h.lifeRemaining))
                    if (h.failing) Text(stringResource(R.string.disk_health_failing), color = readable(Palette.Danger))
                }
            }
        }
    }
}

private fun LazyListScope.networks(system: SystemMetrics) {
    val nics = system.networks.filter { it.connected }
    item(key = "network") {
        Panel(stringResource(R.string.metric_network), accent = Palette.Network) {
            if (nics.isEmpty()) Hint(stringResource(R.string.net_disconnected))
            nics.forEach { n ->
                Text(n.adapter ?: n.name, style = MaterialTheme.typography.titleSmall)
                Text(stringResource(R.string.net_down_up, Format.rate(n.rx), Format.rate(n.tx)), style = MaterialTheme.typography.bodyLarge)
                n.ipv4?.let { Hint(it) }
            }
        }
    }
}

private fun LazyListScope.extras(system: SystemMetrics) {
    if (system.fans.isNotEmpty()) {
        item(key = "fans") {
            Panel(stringResource(R.string.metric_fans), accent = Palette.Power) {
                system.fans.forEach { InfoRow(it.name, Format.rpm(it.rpm)) }
            }
        }
    }
    system.battery?.let { b ->
        item(key = "battery") {
            Panel(stringResource(R.string.metric_battery), accent = Palette.Ok) {
                Bar(b.charge / 100f, Palette.Ok, height = 12.dp)
                InfoRow(stringResource(if (b.charging) R.string.battery_charging else R.string.battery_discharging), Format.percent(b.charge))
                InfoRow(stringResource(R.string.battery_time_left), Format.duration(b.timeRemainingSecs))
                InfoRow(stringResource(R.string.battery_health), Format.percent(b.health))
            }
        }
    }
    system.powerDraw?.let { w ->
        item(key = "power") {
            Panel(stringResource(R.string.metric_power), accent = Palette.Power) { InfoRow(stringResource(R.string.metric_power), Format.watts(w)) }
        }
    }
}
