package app.vitals.tv.ui.pc

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.tv.material3.ListItem
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Switch
import androidx.tv.material3.Text
import app.vitals.core.model.ControlRequest
import app.vitals.core.model.Priority
import app.vitals.core.model.Process
import app.vitals.core.model.ProcessState
import app.vitals.core.net.ApiResult
import app.vitals.core.pairing.Pairing
import app.vitals.tv.R
import app.vitals.tv.data.canControl
import app.vitals.tv.data.tvGraph
import app.vitals.tv.ui.Action
import app.vitals.tv.ui.Choice
import app.vitals.tv.ui.ChoiceRow
import app.vitals.tv.ui.Hint
import app.vitals.tv.ui.InfoRow
import app.vitals.tv.ui.Panel
import app.vitals.tv.ui.controlMessage
import app.vitals.tv.ui.readable
import app.vitals.tv.ui.words
import app.vitals.ui.Format
import app.vitals.ui.LiveState
import app.vitals.ui.Palette
import app.vitals.ui.Thresholds
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

private enum class Sort(val label: Int) { Cpu(R.string.sort_cpu), Memory(R.string.sort_memory), Name(R.string.sort_name) }

/** Enough rows to find anything that matters; a thousand focus stops would make the list unusable with a remote. */
private const val SHOWN = 150

/**
 * The process list on the left, the chosen one on the right with the same
 * actions the phone offers. No search box: typing on a TV keyboard to find a
 * program is slower than sorting and scrolling, and CPU order puts the one a
 * person is looking for at the top.
 */
@Composable
internal fun ProgramsTab(pairing: Pairing, state: LiveState) {
    var sort by rememberSaveable { mutableStateOf(Sort.Cpu) }
    var selectedPid by rememberSaveable { mutableStateOf<Int?>(null) }
    val rows by remember {
        derivedStateOf {
            val all = state.view?.processes?.values ?: return@derivedStateOf null
            when (sort) {
                Sort.Cpu -> all.sortedByDescending { it.cpu }
                Sort.Memory -> all.sortedByDescending { it.memoryPrivate }
                Sort.Name -> all.sortedBy { it.name.lowercase() }
            }
        }
    }
    val list = rows
    Row(Modifier.fillMaxSize(), horizontalArrangement = Arrangement.spacedBy(20.dp)) {
        Column(Modifier.weight(1.2f), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            ChoiceRow {
                Sort.entries.forEach { s -> Choice(stringResource(s.label), selected = sort == s, onClick = { sort = s }) }
            }
            when {
                list == null -> Hint(stringResource(if (state.unreachable) R.string.state_unreachable else R.string.processes_waiting))
                list.isEmpty() -> Hint(stringResource(R.string.processes_none))
                else -> {
                    Hint(stringResource(R.string.processes_shown, minOf(SHOWN, list.size), list.size))
                    LazyColumn(contentPadding = PaddingValues(bottom = 24.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        items(list.take(SHOWN), key = { "${it.key.pid}-${it.key.startTime}" }) { p ->
                            ProcessRow(p, selected = p.key.pid == selectedPid) { selectedPid = p.key.pid }
                        }
                    }
                }
            }
        }
        val selected = selectedPid?.let { pid -> state.view?.processes?.get(pid) }
        Column(Modifier.weight(1f)) {
            if (selected != null) ProcessPanel(pairing, selected)
        }
    }
}

@Composable
private fun ProcessRow(p: Process, selected: Boolean, onClick: () -> Unit) {
    ListItem(
        selected = selected,
        onClick = onClick,
        headlineContent = { Text(p.name, maxLines = 1, overflow = TextOverflow.Ellipsis) },
        supportingContent = when (p.state) {
            ProcessState.Suspended -> ({ Text(stringResource(R.string.process_state_suspended), color = readable(Palette.Warn)) })
            ProcessState.NotResponding -> ({ Text(stringResource(R.string.process_state_not_responding), color = readable(Palette.Warn)) })
            else -> null
        },
        trailingContent = {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(Format.percent(p.cpu), modifier = Modifier.width(80.dp), textAlign = TextAlign.End)
                Text(Format.bytes(p.memoryPrivate), modifier = Modifier.width(110.dp), textAlign = TextAlign.End)
            }
        },
    )
}

@Composable
private fun ProcessPanel(pairing: Pairing, process: Process) {
    val graph = LocalContext.current.tvGraph
    val resources = LocalResources.current
    val scope = rememberCoroutineScope()
    var busy by remember { mutableStateOf(false) }
    var armed by remember(process.key) { mutableStateOf(false) }
    var outcome by remember(process.key) { mutableStateOf<Pair<Boolean, String>?>(null) }
    val current = graph.pairings.pairings.value.firstOrNull { it.id == pairing.id } ?: pairing

    // The confirm press must come soon after the first, or the arming lapses:
    // a panel left open must not be one stray OK from ending a program.
    LaunchedEffect(armed) {
        if (armed) {
            delay(4_000)
            armed = false
        }
    }

    fun run(request: ControlRequest) {
        if (busy) return
        busy = true
        scope.launch {
            val result = graph.client(current).control(request)
            busy = false
            outcome = when (result) {
                is ApiResult.Ok -> true to resources.getString(R.string.control_done)
                is ApiResult.Err -> {
                    graph.noteFailure(current, result.failure)
                    false to result.failure.controlMessage(resources)
                }
            }
        }
    }

    Panel(process.name, accent = Palette.Cpu, modifier = Modifier.verticalScroll(rememberScrollState())) {
        InfoRow(stringResource(R.string.sort_cpu), Format.percent(process.cpu), valueColour = Palette.of(Thresholds.cpu(process.cpu)).takeIf { process.cpu >= 85f } ?: androidx.compose.ui.graphics.Color.Unspecified)
        InfoRow(stringResource(R.string.metric_memory), Format.bytes(process.memoryPrivate))
        InfoRow(stringResource(R.string.process_disk), stringResource(R.string.disk_read_write, Format.rate(process.diskRead), Format.rate(process.diskWrite)))
        InfoRow(stringResource(R.string.metric_gpu), Format.percent(process.gpu))
        InfoRow(stringResource(R.string.process_threads), process.threadCount.toString())
        InfoRow(stringResource(R.string.process_uptime), Format.duration(process.uptimeSecs))
        InfoRow(stringResource(R.string.process_user), process.user ?: Format.DASH)
        InfoRow(stringResource(R.string.process_pid), process.key.pid.toString())
        when {
            !current.canControl() -> Hint(stringResource(R.string.process_read_only))
            !process.isSafelyTerminable -> Hint(stringResource(R.string.process_protected))
            else -> {
                Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    Action(
                        stringResource(if (armed) R.string.action_end_confirm else R.string.action_end),
                        onClick = { if (armed) run(ControlRequest.Terminate(process.key)) else armed = true },
                        enabled = !busy,
                        danger = true,
                    )
                    val suspended = process.state == ProcessState.Suspended
                    Action(
                        stringResource(if (suspended) R.string.action_resume else R.string.action_suspend),
                        onClick = { run(if (suspended) ControlRequest.Resume(process.key) else ControlRequest.Suspend(process.key)) },
                        enabled = !busy,
                    )
                }
                Text(stringResource(R.string.action_priority), style = MaterialTheme.typography.titleSmall)
                // The desktop refuses real-time from a remote, so it is not offered.
                Priority.entries.filter { it != Priority.Realtime }.chunked(3).forEach { rowItems ->
                    ChoiceRow {
                        rowItems.forEach { p ->
                            // The wire does not report the current priority, so none is shown as selected.
                            Choice(stringResource(p.words()), selected = false, onClick = { run(ControlRequest.SetPriority(process.key, p)) })
                        }
                    }
                }
                Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                    Column(Modifier.weight(1f)) {
                        Text(stringResource(R.string.action_efficiency), style = MaterialTheme.typography.bodyLarge)
                        Hint(stringResource(R.string.action_efficiency_body))
                    }
                    Switch(
                        checked = process.inEfficiencyMode,
                        onCheckedChange = { run(ControlRequest.SetEfficiencyMode(process.key, it)) },
                        enabled = !busy,
                    )
                }
            }
        }
        outcome?.let { (ok, text) ->
            Text(text, style = MaterialTheme.typography.bodyLarge, color = if (ok) readable(Palette.Ok) else MaterialTheme.colorScheme.error)
        }
    }
}
