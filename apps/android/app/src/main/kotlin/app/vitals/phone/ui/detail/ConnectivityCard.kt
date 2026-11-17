package app.vitals.phone.ui.detail

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import app.vitals.core.model.ConnectivityReport
import app.vitals.core.model.TargetProbe
import app.vitals.core.net.getOrNull
import app.vitals.core.pairing.Pairing
import app.vitals.phone.R
import app.vitals.phone.graph
import app.vitals.phone.ui.components.SectionCard
import kotlinx.coroutines.launch

/**
 * "Is it my PC or my internet?" — the PC's own network probe, on request.
 *
 * A button rather than something the screen does on open: the PC sends four
 * echo requests per target when asked, and a phone that probed the network
 * every time a detail screen appeared would be spending someone's data plan on
 * a question they did not ask. Nothing is polled here.
 *
 * The verdict arrives as a string rather than an enum, so a PC newer than this
 * build renders the "could not tell" sentence instead of nothing.
 */
@Composable
fun ConnectivityCard(pairing: Pairing, modifier: Modifier = Modifier) {
    val graph = LocalContext.current.graph
    val scope = rememberCoroutineScope()
    var running by remember(pairing.id) { mutableStateOf(false) }
    var report by remember(pairing.id) { mutableStateOf<ConnectivityReport?>(null) }
    var failed by remember(pairing.id) { mutableStateOf(false) }

    SectionCard(title = stringResource(R.string.connectivity_title), modifier = modifier) {
        Text(
            text = stringResource(R.string.connectivity_hint),
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Row(
            modifier = Modifier.fillMaxWidth().padding(top = 8.dp),
            horizontalArrangement = Arrangement.spacedBy(12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Button(
                enabled = !running,
                onClick = {
                    running = true
                    failed = false
                    scope.launch {
                        val outcome = graph.client(pairing).connectivity()
                        report = outcome.getOrNull()
                        failed = report == null
                        running = false
                    }
                },
            ) {
                Text(
                    stringResource(
                        if (running) R.string.connectivity_running else R.string.connectivity_run,
                    ),
                )
            }
            if (failed) {
                Text(
                    text = stringResource(R.string.connectivity_failed),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.error,
                )
            }
        }

        val current = report
        if (current != null) {
            Text(
                text = stringResource(verdictText(current.verdict)),
                style = MaterialTheme.typography.bodyMedium,
                modifier = Modifier.padding(top = 8.dp),
            )
            val rows = buildList {
                current.gateway?.let { add(Pair(stringResource(R.string.connectivity_gateway), it)) }
                current.internet.forEach {
                    add(Pair(stringResource(R.string.connectivity_targets), it))
                }
            }
            Column(modifier = Modifier.padding(top = 4.dp)) {
                rows.forEach { (label, probe) ->
                    Text(
                        text = "$label · ${probe.describe()}",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                current.dnsMs?.let {
                    Text(
                        text = stringResource(R.string.connectivity_dns, it.toInt()),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
        }
    }
}

/** The sentence for a verdict, with "could not tell" as the fallback. */
private fun verdictText(verdict: String): Int = when (verdict) {
    "healthy" -> R.string.connectivity_healthy
    "internet" -> R.string.connectivity_internet
    "localLink" -> R.string.connectivity_local_link
    else -> R.string.connectivity_unknown
}

/** `192.168.1.1 · 12 ms · 0% lost`, or `no reply` where nothing came back. */
private fun TargetProbe.describe(): String {
    val rtt = rttMs?.let { "${it.toInt()} ms" } ?: "—"
    return "$target · $rtt · ${lossPercent.toInt()}%"
}
