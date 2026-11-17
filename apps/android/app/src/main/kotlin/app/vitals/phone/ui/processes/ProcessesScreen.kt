package app.vitals.phone.ui.processes

import app.vitals.phone.ui.theme.readable
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Search
import androidx.compose.material3.FilterChip
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.vitals.core.model.Process
import app.vitals.core.model.ProcessState
import app.vitals.core.pairing.Pairing
import app.vitals.phone.R
import app.vitals.ui.LiveState
import app.vitals.phone.graph
import app.vitals.phone.ui.LocalNavigator
import app.vitals.phone.ui.components.GlassScaffold
import app.vitals.phone.ui.nav.AddPc
import app.vitals.phone.ui.theme.glassSurface
import app.vitals.ui.Format
import app.vitals.ui.Palette
import app.vitals.ui.Thresholds

private enum class Sort { Cpu, Memory, Name }

/** Material's breakpoint for an expanded window: a tablet, or a phone opened flat. */
private val WIDE = 600.dp

@Composable
fun ProcessesScreen(pairing: Pairing) {
    val graph = LocalContext.current.graph
    val flow = remember(pairing.id, pairing.baseUrl, pairing.token) { graph.live.observe(pairing) }
    val state by flow.collectAsStateWithLifecycle(LiveState())
    var query by rememberSaveable { mutableStateOf("") }
    var sort by rememberSaveable { mutableStateOf(Sort.Cpu) }
    var selectedPid by rememberSaveable { mutableStateOf<Int?>(null) }

    // Sorting a thousand processes is cheap once per frame, but not once per
    // recomposition of every row; derivedStateOf recomputes only when the
    // view, the query or the sort actually change.
    val rows by remember {
        derivedStateOf {
            val all = state.view?.processes?.values ?: return@derivedStateOf null
            val q = query.trim()
            val filtered = if (q.isEmpty()) all.toList() else all.filter { it.name.contains(q, ignoreCase = true) || it.displayName.contains(q, ignoreCase = true) }
            when (sort) {
                Sort.Cpu -> filtered.sortedByDescending { it.cpu }
                Sort.Memory -> filtered.sortedByDescending { it.memoryPrivate }
                Sort.Name -> filtered.sortedBy { it.displayName.lowercase() }
            }
        }
    }
    val selected = selectedPid?.let { pid -> state.view?.processes?.get(pid) }
    // 600 dp is where Material's list-detail layout starts: the list keeps its
    // own column and the chosen program appears beside it rather than covering
    // it. Narrower than that, the sheet below is what a thumb can reach, and
    // nothing about it changes.
    // The window, not the configuration: a foldable that opens changes its
    // window without necessarily restarting the activity, and a configuration
    // read can lag that by a frame or not see it at all.
    val window = LocalWindowInfo.current.containerSize
    val wide = with(LocalDensity.current) { window.width.toDp() } >= WIDE

    GlassScaffold(title = stringResource(R.string.processes_title, pairing.label)) { padding ->
        Row(Modifier.fillMaxSize().padding(top = padding.calculateTopPadding())) {
            Column(Modifier.weight(1.2f)) {
                ProcessControls(query = query, onQuery = { query = it }, sort = sort, onSort = { sort = it })
                ProcessList(
                    state = state,
                    rows = rows,
                    selectedPid = selectedPid,
                    onPick = { selectedPid = it },
                    bottom = padding.calculateBottomPadding(),
                )
            }
            if (wide && selected != null) {
                ProcessDetail(
                    pairing = pairing,
                    process = selected,
                    onDismiss = { selectedPid = null },
                    modifier = Modifier
                        .weight(1f)
                        .fillMaxSize()
                        .padding(start = 8.dp, bottom = padding.calculateBottomPadding())
                        .clip(RoundedCornerShape(24.dp))
                        .glassSurface()
                        .verticalScroll(rememberScrollState())
                        .padding(20.dp),
                )
            }
        }
    }

    // Only where the detail cannot sit beside the list: there it opens over it.
    if (!wide && selected != null) {
        ProcessSheet(pairing, selected, onDismiss = { selectedPid = null })
    }
}

@Composable
private fun ProcessControls(query: String, onQuery: (String) -> Unit, sort: Sort, onSort: (Sort) -> Unit) {
    OutlinedTextField(
        value = query,
        onValueChange = onQuery,
        modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
        singleLine = true,
        leadingIcon = { Icon(Icons.Filled.Search, null) },
        placeholder = { Text(stringResource(R.string.processes_search)) },
        shape = MaterialTheme.shapes.extraLarge,
    )
    Row(Modifier.padding(horizontal = 16.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Sort.entries.forEach { s ->
            FilterChip(
                selected = sort == s,
                onClick = { onSort(s) },
                label = {
                    Text(
                        stringResource(
                            when (s) {
                                Sort.Cpu -> R.string.sort_cpu
                                Sort.Memory -> R.string.sort_memory
                                Sort.Name -> R.string.sort_name
                            },
                        ),
                    )
                },
            )
        }
    }
}

@Composable
private fun ProcessList(state: LiveState, rows: List<Process>?, selectedPid: Int?, onPick: (Int) -> Unit, bottom: Dp) {
    when {
        // A refused token is not "waiting for the PC": the stream has stopped
        // trying, so nothing arrives until the PC is paired again. Saying
        // "Waiting…" here left a revoked pairing looking like a slow one
        // forever (S24-09).
        state.unauthorised -> Refused()
        rows == null -> Centred(stringResource(if (state.unreachable) R.string.state_unreachable else R.string.processes_waiting))
        rows.isEmpty() -> Centred(stringResource(R.string.processes_none))
        else -> LazyColumn(
            contentPadding = PaddingValues(bottom = bottom + 24.dp),
            modifier = Modifier.fillMaxSize(),
        ) {
            items(rows, key = { "${it.key.pid}-${it.key.startTime}" }) { p ->
                ProcessRow(p, selected = p.key.pid == selectedPid, onClick = { onPick(p.key.pid) }, modifier = Modifier.animateItem())
                HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.4f))
            }
        }
    }
}

/** What the user can do about a pairing the PC has stopped accepting. */
@Composable
private fun Refused() {
    val navigator = LocalNavigator.current
    Column(
        Modifier.fillMaxWidth().padding(32.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(stringResource(R.string.state_unauthorised), textAlign = TextAlign.Center)
        Spacer(Modifier.height(8.dp))
        Text(
            stringResource(R.string.pair_again_hint),
            style = MaterialTheme.typography.bodySmall,
            textAlign = TextAlign.Center,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Spacer(Modifier.height(16.dp))
        OutlinedButton(onClick = { navigator.push(AddPc()) }) { Text(stringResource(R.string.pair_again)) }
    }
}

@Composable
private fun Centred(text: String) {
    Text(
        text,
        modifier = Modifier.fillMaxWidth().padding(32.dp),
        textAlign = TextAlign.Center,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
}

@Composable
private fun ProcessRow(p: Process, selected: Boolean, onClick: () -> Unit, modifier: Modifier = Modifier) {
    Row(
        modifier
            .fillMaxWidth()
            // Only ever true while the detail sits beside the list; it is what
            // says which row the right-hand pane is describing.
            .background(if (selected) MaterialTheme.colorScheme.secondaryContainer.copy(alpha = 0.35f) else Color.Transparent)
            .clickable(onClick = onClick)
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(p.displayName, style = MaterialTheme.typography.bodyLarge, maxLines = 1, overflow = TextOverflow.Ellipsis)
            val status = when (p.state) {
                ProcessState.Suspended -> stringResource(R.string.process_state_suspended)
                ProcessState.NotResponding -> stringResource(R.string.process_state_not_responding)
                else -> null
            }
            if (status != null) {
                Text(status, style = MaterialTheme.typography.bodySmall, color = readable(Palette.Warn))
            }
        }
        Text(
            Format.percent(p.cpu),
            modifier = Modifier.width(64.dp),
            textAlign = TextAlign.End,
            color = readable(Palette.of(Thresholds.cpu(p.cpu))).takeIf { p.cpu >= 85f } ?: MaterialTheme.colorScheme.onSurface,
            style = MaterialTheme.typography.bodyMedium,
        )
        Text(
            Format.bytes(p.memoryPrivate),
            modifier = Modifier.width(84.dp),
            textAlign = TextAlign.End,
            style = MaterialTheme.typography.bodyMedium,
        )
    }
}
