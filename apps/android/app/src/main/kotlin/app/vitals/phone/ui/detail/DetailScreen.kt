package app.vitals.phone.ui.detail

import app.vitals.phone.ui.theme.readable
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.List
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.PrimaryTabRow
import androidx.compose.material3.Tab
import androidx.compose.material3.Text
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
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.vitals.core.model.SystemMetrics
import app.vitals.core.net.WakeOnLan
import app.vitals.core.pairing.Pairing
import app.vitals.phone.R
import app.vitals.ui.LiveState
import app.vitals.phone.graph
import app.vitals.phone.ui.LocalNavigator
import app.vitals.phone.ui.LocalSnackbar
import app.vitals.phone.ui.components.FillBar
import app.vitals.phone.ui.components.GlassScaffold
import app.vitals.phone.ui.components.InfoRow
import app.vitals.phone.ui.components.RingGauge
import app.vitals.phone.ui.components.SectionCard
import app.vitals.phone.ui.components.Sparkline
import app.vitals.phone.ui.nav.Processes
import app.vitals.phone.ui.relativeTime
import app.vitals.phone.ui.title
import app.vitals.phone.ui.words
import app.vitals.ui.Format
import app.vitals.ui.Palette
import app.vitals.ui.Readings
import app.vitals.ui.Thresholds

private enum class DetailTab { Now, Sensors, History }

@Composable
fun DetailScreen(pairing: Pairing) {
    val graph = LocalContext.current.graph
    val resources = LocalResources.current
    val navigator = LocalNavigator.current
    val snackbar = LocalSnackbar.current
    val flow = remember(pairing.id, pairing.baseUrl, pairing.token) { graph.live.observe(pairing) }
    val state by flow.collectAsStateWithLifecycle(LiveState())
    var tab by rememberSaveable { mutableStateOf(DetailTab.Now) }
    var wake by remember { mutableIntStateOf(0) }

    LaunchedEffect(wake) {
        if (wake == 0) return@LaunchedEffect
        val mac = pairing.mac
        val message = if (mac == null) {
            R.string.wake_no_mac
        } else if (runCatching { WakeOnLan.send(mac, pairing.broadcast) }.getOrDefault(false)) {
            R.string.wake_sent
        } else {
            R.string.wake_failed
        }
        snackbar.showSnackbar(resources.getString(message))
    }

    GlassScaffold(
        title = pairing.label,
        actions = {
            IconButton(onClick = { wake++ }) {
                Icon(painterResource(R.drawable.ic_power), stringResource(R.string.wake))
            }
            IconButton(onClick = { navigator.push(Processes(pairing.id)) }) {
                Icon(Icons.AutoMirrored.Filled.List, stringResource(R.string.processes))
            }
        },
    ) { padding ->
        Column(Modifier.fillMaxSize().padding(top = padding.calculateTopPadding())) {
            PrimaryTabRow(selectedTabIndex = tab.ordinal, containerColor = androidx.compose.ui.graphics.Color.Transparent) {
                DetailTab.entries.forEach { t ->
                    Tab(
                        selected = tab == t,
                        onClick = { tab = t },
                        text = {
                            Text(
                                stringResource(
                                    when (t) {
                                        DetailTab.Now -> R.string.tab_now
                                        DetailTab.Sensors -> R.string.tab_sensors
                                        DetailTab.History -> R.string.tab_history
                                    },
                                ),
                            )
                        },
                    )
                }
            }
            val bottom = PaddingValues(start = 16.dp, end = 16.dp, top = 12.dp, bottom = padding.calculateBottomPadding() + 24.dp)
            AnimatedContent(tab, transitionSpec = { fadeIn() togetherWith fadeOut() }, label = "tab") { t ->
                when (t) {
                    DetailTab.Now -> NowTab(state, bottom)
                    DetailTab.Sensors -> SensorsTab(pairing, bottom)
                    DetailTab.History -> HistoryTab(pairing, bottom)
                }
            }
        }
    }
}

@Composable
private fun NowTab(state: LiveState, padding: PaddingValues) {
    val system = state.view?.system
    LazyColumn(
        contentPadding = padding,
        verticalArrangement = Arrangement.spacedBy(12.dp),
        modifier = Modifier.fillMaxSize(),
    ) {
        if (state.unreachable || system == null) {
            item {
                SectionCard(stringResource(if (state.unreachable) R.string.state_unreachable else R.string.state_connecting)) {
                    state.lastSeenMs?.let { Text(stringResource(R.string.last_seen, relativeTime(it))) }
                    if (state.unreachable) Text(stringResource(R.string.state_unreachable_hint))
                }
            }
            return@LazyColumn
        }
        if (state.alerts.isNotEmpty()) {
            item(key = "alerts") {
                SectionCard(stringResource(R.string.alert_count_title), accent = Palette.Warn) {
                    state.alerts.forEach { a ->
                        Text(
                            stringResource(a.kind.title()) + if (a.subject.isNotEmpty()) " · ${a.subject}" else "",
                            color = readable(if (a.severity == app.vitals.core.model.Severity.Critical) Palette.Danger else Palette.Warn),
                        )
                    }
                }
            }
        }
        item(key = "hero") { Hero(state, system) }
        cpu(system)
        memory(system)
        gpus(system)
        disks(system)
        networks(system)
        extras(system)
    }
}

@Composable
private fun Hero(state: LiveState, system: SystemMetrics) {
    val memory = Readings.memoryPercent(system)
    val gpu = Readings.primaryGpu(system)?.utilization
    val temp = Readings.cpuTemperature(system)
    SectionCard(stringResource(R.string.tab_now)) {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
            RingGauge(
                system.cpu.total / 100f, Palette.Cpu, Format.percentCompact(system.cpu.total),
                stringResource(R.string.metric_cpu), diameter = 76.dp, level = Thresholds.cpu(system.cpu.total),
            )
            RingGauge(
                memory?.div(100f), Palette.Memory, Format.percentCompact(memory),
                stringResource(R.string.metric_memory_short), diameter = 76.dp, level = Thresholds.memory(memory),
            )
            RingGauge(gpu?.div(100f), Palette.Gpu, Format.percentCompact(gpu), stringResource(R.string.metric_gpu), diameter = 76.dp)
            RingGauge(
                temp?.div(110f), Palette.Thermal, Format.celsiusCompact(temp),
                stringResource(R.string.metric_temp), diameter = 76.dp, level = Thresholds.cpuTemp(temp),
            )
        }
        Sparkline(state.cpu, Palette.Cpu)
        Sparkline(state.memory, Palette.Memory)
        if (state.gpu.size > 0) Sparkline(state.gpu, Palette.Gpu)
        if (state.temperature.size > 0) Sparkline(state.temperature, Palette.Thermal, max = 110f)
    }
}

private fun LazyListScope.cpu(system: SystemMetrics) = item(key = "cpu") {
    val cpu = system.cpu
    SectionCard(stringResource(R.string.metric_cpu), accent = Palette.Cpu) {
        InfoRow(stringResource(R.string.cpu_clock), Format.ghz(cpu.effectiveClock))
        InfoRow(
            stringResource(R.string.cpu_temperature),
            Format.celsius(cpu.temperature),
            valueColour = Palette.of(Thresholds.cpuTemp(cpu.temperature)),
        )
        InfoRow(stringResource(R.string.cpu_power), Format.watts(cpu.power))
        val throttle = cpu.throttled
        Text(
            if (throttle == null) {
                stringResource(R.string.cpu_not_throttled)
            } else {
                stringResource(R.string.cpu_throttled) + " " + stringResource(throttle.words())
            },
            color = if (throttle == null) MaterialTheme.colorScheme.onSurfaceVariant else readable(Palette.Warn),
            style = MaterialTheme.typography.bodyMedium,
        )
        InfoRow(stringResource(R.string.cpu_processes), cpu.processCount.toString())
        InfoRow(stringResource(R.string.cpu_uptime), Format.duration(cpu.uptimeSecs))
        Text(stringResource(R.string.cpu_cores), style = MaterialTheme.typography.labelLarge)
        // Two columns keep a 32-thread CPU to sixteen short rows.
        cpu.perCore.chunked(2).forEach { pair ->
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
                pair.forEach { load ->
                    FillBar(load / 100f, Palette.Cpu, Modifier.weight(1f))
                }
                if (pair.size == 1) androidx.compose.foundation.layout.Spacer(Modifier.weight(1f))
            }
        }
    }
}

private fun LazyListScope.memory(system: SystemMetrics) = item(key = "memory") {
    val m = system.memory
    SectionCard(stringResource(R.string.metric_memory), accent = Palette.Memory) {
        FillBar(Readings.memoryPercent(system)?.div(100f), Palette.Memory, height = 10.dp)
        Text(stringResource(R.string.memory_used_of, Format.bytes(m.used), Format.bytes(m.total)))
        InfoRow(stringResource(R.string.memory_available), Format.bytes(m.available))
        InfoRow(stringResource(R.string.memory_cached), Format.bytes(m.cached))
        InfoRow(
            stringResource(R.string.memory_committed),
            stringResource(R.string.memory_used_of, Format.bytes(m.committed), Format.bytes(m.commitLimit)),
        )
    }
}

private fun LazyListScope.gpus(system: SystemMetrics) = items(system.gpus, key = { "gpu-${it.id}" }) { g ->
    SectionCard(g.name, accent = Palette.Gpu) {
        FillBar(g.utilization?.div(100f), Palette.Gpu, height = 10.dp)
        InfoRow(stringResource(R.string.metric_gpu), Format.percent(g.utilization))
        val vram = if (g.memoryUsed != null && g.memoryTotal != null) {
            stringResource(R.string.memory_used_of, Format.bytes(g.memoryUsed), Format.bytes(g.memoryTotal))
        } else {
            Format.bytes(g.memoryUsed)
        }
        InfoRow(stringResource(R.string.gpu_memory), vram)
        InfoRow(
            stringResource(R.string.gpu_temperature),
            Format.celsius(g.temperature),
            valueColour = Palette.of(Thresholds.gpuTemp(g.temperature)),
        )
        InfoRow(stringResource(R.string.gpu_power), Format.watts(g.power))
        InfoRow(stringResource(R.string.gpu_fan), g.fanRpm?.let(Format::rpm) ?: Format.percent(g.fanPercent))
        g.throttled?.let {
            Text(stringResource(R.string.cpu_throttled) + " " + stringResource(it.words()), color = readable(Palette.Warn))
        }
    }
}

private fun LazyListScope.disks(system: SystemMetrics) {
    if (system.disks.isEmpty()) return
    item(key = "disks") {
        SectionCard(stringResource(R.string.metric_disk), accent = Palette.Disk) {
            system.disks.forEach { d ->
                val name = listOfNotNull(d.mount, d.model ?: d.name).joinToString(" · ")
                Text(name, style = MaterialTheme.typography.labelLarge)
                if (d.total > 0) {
                    FillBar((d.total - d.free).toFloat() / d.total, Palette.Disk)
                    Text(
                        stringResource(R.string.disk_free, Format.bytes(d.free), Format.bytes(d.total)),
                        style = MaterialTheme.typography.bodySmall,
                    )
                }
                InfoRow(stringResource(R.string.disk_activity), Format.percent(d.activeTime))
                Text(
                    stringResource(R.string.disk_read_write, Format.rate(d.read), Format.rate(d.write)),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                d.temperature?.let {
                    InfoRow(
                        stringResource(R.string.cpu_temperature),
                        Format.celsius(it),
                        valueColour = Palette.of(Thresholds.diskTemp(it)),
                    )
                }
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
        SectionCard(stringResource(R.string.metric_network), accent = Palette.Network) {
            if (nics.isEmpty()) Text(stringResource(R.string.net_disconnected))
            nics.forEach { n ->
                Text(n.adapter ?: n.name, style = MaterialTheme.typography.labelLarge)
                Text(
                    stringResource(R.string.net_down_up, Format.rate(n.rx), Format.rate(n.tx)),
                    style = MaterialTheme.typography.bodySmall,
                )
                n.ipv4?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
            }
        }
    }
}

private fun LazyListScope.extras(system: SystemMetrics) {
    if (system.fans.isNotEmpty()) {
        item(key = "fans") {
            SectionCard(stringResource(R.string.metric_fans), accent = Palette.Power) {
                system.fans.forEach { InfoRow(it.name, Format.rpm(it.rpm)) }
            }
        }
    }
    system.battery?.let { b ->
        item(key = "battery") {
            SectionCard(stringResource(R.string.metric_battery), accent = Palette.Ok) {
                FillBar(b.charge / 100f, Palette.Ok, height = 10.dp)
                InfoRow(
                    stringResource(if (b.charging) R.string.battery_charging else R.string.battery_discharging),
                    Format.percent(b.charge),
                )
                InfoRow(stringResource(R.string.battery_time_left), Format.duration(b.timeRemainingSecs))
                InfoRow(stringResource(R.string.battery_health), Format.percent(b.health))
            }
        }
    }
    system.powerDraw?.let { w ->
        item(key = "power") {
            SectionCard(stringResource(R.string.metric_power), accent = Palette.Power) {
                InfoRow(stringResource(R.string.metric_power), Format.watts(w))
            }
        }
    }
}
