package app.vitals.phone.ui.processes

import app.vitals.phone.ui.theme.readable
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Search
import androidx.compose.material3.FilterChip
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.vitals.core.model.Process
import app.vitals.core.model.ProcessState
import app.vitals.core.pairing.Pairing
import app.vitals.phone.R
import app.vitals.ui.LiveState
import app.vitals.phone.graph
import app.vitals.phone.ui.components.GlassScaffold
import app.vitals.ui.Format
import app.vitals.ui.Palette
import app.vitals.ui.Thresholds

private enum class Sort { Cpu, Memory, Name }

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

    GlassScaffold(title = stringResource(R.string.processes_title, pairing.label)) { padding ->
        Column(Modifier.fillMaxSize().padding(top = padding.calculateTopPadding())) {
            OutlinedTextField(
                value = query,
                onValueChange = { query = it },
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
                        onClick = { sort = s },
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
            val list = rows
            when {
                list == null -> Centred(
                    stringResource(if (state.unreachable) R.string.state_unreachable else R.string.processes_waiting),
                )
                list.isEmpty() -> Centred(stringResource(R.string.processes_none))
                else -> LazyColumn(
                    contentPadding = PaddingValues(bottom = padding.calculateBottomPadding() + 24.dp),
                    modifier = Modifier.fillMaxSize(),
                ) {
                    items(list, key = { "${it.key.pid}-${it.key.startTime}" }) { p ->
                        ProcessRow(p, onClick = { selectedPid = p.key.pid }, modifier = Modifier.animateItem())
                        HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.4f))
                    }
                }
            }
        }
    }

    val selected = selectedPid?.let { pid -> state.view?.processes?.get(pid) }
    if (selected != null) {
        ProcessSheet(pairing, selected, onDismiss = { selectedPid = null })
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
private fun ProcessRow(p: Process, onClick: () -> Unit, modifier: Modifier = Modifier) {
    Row(
        modifier.fillMaxWidth().clickable(onClick = onClick).padding(horizontal = 16.dp, vertical = 12.dp),
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
