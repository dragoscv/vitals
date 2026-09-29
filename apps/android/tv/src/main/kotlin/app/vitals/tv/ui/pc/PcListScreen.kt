package app.vitals.tv.ui.pc

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Text
import app.vitals.core.pairing.Pairing
import app.vitals.tv.LocalNavigator
import app.vitals.tv.R
import app.vitals.tv.Route
import app.vitals.tv.data.tvGraph
import app.vitals.tv.ui.Action
import app.vitals.tv.ui.FocusCard
import app.vitals.tv.ui.Hint
import app.vitals.tv.ui.SafeHorizontal
import app.vitals.tv.ui.SafeVertical
import app.vitals.tv.ui.ScreenTitle
import app.vitals.tv.ui.overview.PcStatus
import app.vitals.ui.Format
import app.vitals.ui.LiveState
import app.vitals.ui.Readings

@Composable
fun PcListScreen() {
    val graph = LocalContext.current.tvGraph
    val navigator = LocalNavigator.current
    val pairings by graph.pairings.pairings.collectAsStateWithLifecycle()
    val ready by graph.ready.collectAsStateWithLifecycle()
    LazyColumn(
        contentPadding = PaddingValues(horizontal = SafeHorizontal, vertical = SafeVertical),
        verticalArrangement = Arrangement.spacedBy(16.dp),
        modifier = Modifier.fillMaxSize(),
    ) {
        item { ScreenTitle(stringResource(R.string.overview_pcs)) }
        if (ready && pairings.isEmpty()) {
            item { Hint(stringResource(R.string.overview_no_pcs)) }
            item { Action(stringResource(R.string.nav_add), onClick = { navigator.top(Route.Add) }) }
        }
        items(pairings, key = { it.id }) { p -> Row(p) { navigator.push(Route.Pc(p.id)) } }
    }
}

@Composable
private fun Row(pairing: Pairing, onOpen: () -> Unit) {
    val graph = LocalContext.current.tvGraph
    val flow = remember(pairing.id, pairing.baseUrl, pairing.token) { graph.live(pairing) }
    val state by flow.collectAsStateWithLifecycle(LiveState())
    FocusCard(onClick = onOpen) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(24.dp)) {
            Text(pairing.label, style = MaterialTheme.typography.titleLarge, modifier = Modifier.weight(1f), maxLines = 1)
            val s = state.view?.system
            if (s != null && !state.unreachable) {
                Text("CPU " + Format.percent(s.cpu.total), style = MaterialTheme.typography.titleMedium)
                Text(stringResource(R.string.metric_memory_short) + " " + Format.percent(Readings.memoryPercent(s)), style = MaterialTheme.typography.titleMedium)
                Text(Format.celsius(Readings.cpuTemperature(s)), style = MaterialTheme.typography.titleMedium)
            }
        }
        PcStatus(state)
    }
}
