package app.vitals.phone.ui.machines

import app.vitals.phone.ui.theme.readable
import androidx.compose.animation.Crossfade
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.AssistChip
import androidx.compose.material3.AssistChipDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExtendedFloatingActionButton
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.vitals.core.model.Severity
import app.vitals.core.net.WakeOnLan
import app.vitals.core.pairing.Pairing
import app.vitals.phone.R
import app.vitals.ui.LiveState
import app.vitals.phone.graph
import app.vitals.phone.ui.LocalNavigator
import app.vitals.phone.ui.LocalSnackbar
import app.vitals.phone.ui.components.GlassScaffold
import app.vitals.phone.ui.components.RingGauge
import app.vitals.phone.ui.components.Sparkline
import app.vitals.phone.ui.components.Tab
import app.vitals.phone.ui.nav.AddPc
import app.vitals.phone.ui.nav.Detail
import app.vitals.phone.ui.relativeTime
import app.vitals.ui.Format
import app.vitals.ui.Palette
import app.vitals.ui.Readings
import app.vitals.ui.Thresholds
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

@Composable
fun MachinesScreen() {
    val context = LocalContext.current
    val graph = context.graph
    val navigator = LocalNavigator.current
    val pairings by graph.pairings.pairings.collectAsStateWithLifecycle()
    val ready by graph.ready.collectAsStateWithLifecycle()
    var refreshing by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()

    GlassScaffold(
        title = stringResource(R.string.machines_title),
        tab = Tab.Machines,
        floatingActionButton = {
            ExtendedFloatingActionButton(
                onClick = { navigator.push(AddPc()) },
                icon = { Icon(Icons.Filled.Add, null) },
                text = { Text(stringResource(R.string.add_pc)) },
            )
        },
    ) { padding ->
        PullToRefreshBox(
            isRefreshing = refreshing,
            onRefresh = {
                refreshing = true
                pairings.forEach { graph.live.kick(it.id) }
                scope.launch {
                    // The stream answers within a frame or two; the spinner
                    // only needs to show the gesture registered.
                    delay(800)
                    refreshing = false
                }
            },
            modifier = Modifier.fillMaxSize(),
        ) {
            Crossfade(targetState = ready && pairings.isEmpty(), label = "empty") { empty ->
                if (empty) {
                    EmptyState(padding)
                } else {
                    LazyColumn(
                        contentPadding = PaddingValues(
                            start = 16.dp,
                            end = 16.dp,
                            top = padding.calculateTopPadding() + 8.dp,
                            bottom = padding.calculateBottomPadding() + 88.dp,
                        ),
                        verticalArrangement = Arrangement.spacedBy(12.dp),
                        modifier = Modifier.fillMaxSize(),
                    ) {
                        items(pairings, key = { it.id }) { pairing ->
                            MachineCard(
                                pairing,
                                onOpen = { navigator.push(Detail(pairing.id)) },
                                modifier = Modifier.animateItem(),
                            )
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun EmptyState(padding: PaddingValues) {
    Column(
        Modifier.fillMaxSize().padding(padding).padding(32.dp),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(stringResource(R.string.machines_empty_title), style = MaterialTheme.typography.headlineSmall)
        Spacer(Modifier.height(12.dp))
        Text(
            stringResource(R.string.machines_empty_body),
            style = MaterialTheme.typography.bodyLarge,
            textAlign = TextAlign.Center,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

@Composable
private fun MachineCard(pairing: Pairing, onOpen: () -> Unit, modifier: Modifier = Modifier) {
    val graph = LocalContext.current.graph
    val flow = remember(pairing.id, pairing.baseUrl, pairing.token) { graph.live.observe(pairing) }
    val state by flow.collectAsStateWithLifecycle(LiveState())

    Card(
        modifier = modifier.fillMaxWidth().clickable(onClick = onOpen),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surfaceContainer.copy(alpha = 0.92f),
        ),
        shape = MaterialTheme.shapes.extraLarge,
    ) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text(pairing.label, style = MaterialTheme.typography.titleLarge, maxLines = 1)
                    StatusLine(state)
                }
                AlertBadge(state)
            }
            Crossfade(targetState = state.view != null && !state.unreachable, label = "live") { live ->
                if (live) LiveBody(state) else OfflineBody(pairing, state)
            }
        }
    }
}

@Composable
private fun StatusLine(state: LiveState) {
    val text = when {
        state.unauthorised -> stringResource(R.string.state_unauthorised)
        state.incompatible -> stringResource(R.string.state_incompatible)
        state.unreachable -> state.lastSeenMs?.let { stringResource(R.string.last_seen, relativeTime(it)) }
            ?: stringResource(R.string.state_unreachable)
        state.view == null -> stringResource(R.string.state_connecting)
        else -> null
    }
    if (text != null) {
        Text(text, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

@Composable
private fun AlertBadge(state: LiveState) {
    val serious = state.alerts.count { it.severity != Severity.Info }
    if (serious == 0) return
    val critical = state.alerts.any { it.severity == Severity.Critical }
    val colour = readable(if (critical) Palette.Danger else Palette.Warn)
    AssistChip(
        onClick = {},
        label = { Text(pluralStringResource(R.plurals.alert_count, serious, serious)) },
        leadingIcon = { Icon(Icons.Filled.Warning, null, tint = colour) },
        colors = AssistChipDefaults.assistChipColors(labelColor = colour),
    )
}

@Composable
private fun LiveBody(state: LiveState) {
    val system = state.view?.system ?: return
    val memory = Readings.memoryPercent(system)
    val gpu = Readings.primaryGpu(system)?.utilization
    val temp = Readings.cpuTemperature(system)
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
            RingGauge(
                system.cpu.total / 100f, Palette.Cpu, Format.percentCompact(system.cpu.total),
                stringResource(R.string.metric_cpu), level = Thresholds.cpu(system.cpu.total),
            )
            RingGauge(
                memory?.div(100f), Palette.Memory, Format.percentCompact(memory),
                stringResource(R.string.metric_memory_short), level = Thresholds.memory(memory),
            )
            RingGauge(gpu?.div(100f), Palette.Gpu, Format.percentCompact(gpu), stringResource(R.string.metric_gpu))
            RingGauge(
                temp?.div(110f), Palette.Thermal, Format.celsiusCompact(temp),
                stringResource(R.string.metric_temp), level = Thresholds.cpuTemp(temp),
            )
        }
        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Box(Modifier.weight(1f)) { Sparkline(state.cpu, Palette.Cpu) }
            Box(Modifier.weight(1f)) { Sparkline(state.memory, Palette.Memory) }
        }
    }
}

@Composable
private fun OfflineBody(pairing: Pairing, state: LiveState) {
    if (!state.unreachable) {
        Spacer(Modifier.height(4.dp))
        return
    }
    val resources = LocalResources.current
    val snackbar = LocalSnackbar.current
    val mac = pairing.mac
    var sending by remember { mutableStateOf(false) }
    var request by remember { mutableIntStateOf(0) }
    LaunchedEffect(request) {
        if (request == 0 || mac == null) return@LaunchedEffect
        sending = true
        val ok = runCatching { WakeOnLan.send(mac, pairing.broadcast) }.getOrDefault(false)
        sending = false
        snackbar.showSnackbar(resources.getString(if (ok) R.string.wake_sent else R.string.wake_failed))
    }
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(
            stringResource(R.string.state_unreachable_hint),
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        if (mac != null) {
            OutlinedButton(onClick = { request++ }, enabled = !sending) { Text(stringResource(R.string.wake)) }
        }
    }
}
