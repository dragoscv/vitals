package app.vitals.phone.ui.detail

import androidx.annotation.StringRes
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.repeatOnLifecycle
import app.vitals.core.model.AgentClient
import app.vitals.core.model.AgentsPoll
import app.vitals.core.model.AgentsSnapshot
import app.vitals.core.model.SessionState
import app.vitals.core.model.activeSessions
import app.vitals.core.model.busyQueues
import app.vitals.core.pairing.Pairing
import app.vitals.phone.R
import app.vitals.phone.graph
import app.vitals.phone.ui.components.SectionCard
import app.vitals.phone.ui.theme.readable
import app.vitals.ui.Palette
import kotlinx.coroutines.delay

/** How many rows of each list a phone shows; the desktop has the full page. */
private const val MAX_ROWS = 4

/**
 * The PC's AI page, read-only (ADR-0039, S18-07): which agents are working
 * and which agentq queues have someone waiting.
 *
 * The PC shares this only when the user turns on "Share AI activity with
 * paired devices", and then only an allowlist of fields — no titles, commands
 * or purposes. Until then it answers 404, and the card stays hidden and asks
 * again every five minutes rather than every ten seconds. The polling policy
 * is [AgentsPoll]: each poll keeps the PC's reader awake for thirty seconds,
 * so nothing is read on the PC once the phone looks away or nothing is
 * happening.
 *
 * Read-only on purpose. Freezing a queue or breaking someone's ticket from a
 * phone in a pocket is the kind of control that needs the desktop's reason
 * prompt and confirmation; the LAN route exists for scripts that do it
 * deliberately.
 */
@Composable
fun AgentsCard(pairing: Pairing, modifier: Modifier = Modifier) {
    val graph = LocalContext.current.graph
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    var snapshot by remember(pairing.id) { mutableStateOf<AgentsSnapshot?>(null) }

    LaunchedEffect(pairing.id, lifecycle) {
        lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) {
            val client = graph.client(pairing)
            while (true) {
                val next = AgentsPoll.after(client.agents())
                when (next) {
                    is AgentsPoll.Show -> snapshot = next.snapshot
                    is AgentsPoll.Hide -> snapshot = null
                    is AgentsPoll.Keep -> Unit
                }
                delay(next.delayMs)
            }
        }
    }

    val s = snapshot ?: return
    val active = s.activeSessions
    val queues = s.busyQueues

    SectionCard(stringResource(R.string.agents_title), accent = Palette.Network, modifier = modifier) {
        if (active.isEmpty()) {
            Text(stringResource(R.string.agents_none_working))
        }
        active.take(MAX_ROWS).forEach { session ->
            val name = clientName(session.client)
            // Title and repo are free text the PC may no longer send; the
            // client name alone is a complete row.
            val what = session.title ?: session.repo
            AgentRow(
                label = if (what != null) "$name · $what" else name,
                value = stringResource(stateLabel(session.state)),
                valueColour = when (session.state) {
                    SessionState.Working -> Palette.Ok
                    SessionState.Waiting, SessionState.Idle -> Palette.Warn
                    else -> Color.Unspecified
                },
            )
        }
        if (active.size > MAX_ROWS) {
            Text(stringResource(R.string.agents_more, active.size - MAX_ROWS))
        }
        if (!s.agentqInstalled) return@SectionCard
        if (queues.isEmpty()) {
            Text(stringResource(R.string.agents_queues_idle))
        }
        queues.take(MAX_ROWS).forEach { q ->
            AgentRow(
                label = q.name,
                value = stringResource(R.string.agents_queue_value, q.holding, q.waiting),
                valueColour = if (q.waiting > 0) Palette.Warn else Color.Unspecified,
            )
        }
    }
}

/**
 * One row, read by TalkBack as one sentence (AND2-4) rather than a label and
 * a stray value two swipes apart. The label wraps to two lines: a repo name
 * after the client name is the part worth reading, and one line cut it off.
 */
@Composable
private fun AgentRow(label: String, value: String, valueColour: Color) {
    Row(
        Modifier.fillMaxWidth().semantics(mergeDescendants = true) {},
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            label,
            modifier = Modifier.weight(1f).padding(end = 8.dp),
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
        )
        Text(
            value,
            style = MaterialTheme.typography.bodyMedium,
            fontWeight = FontWeight.SemiBold,
            color = readable(valueColour),
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

@Composable
private fun clientName(client: AgentClient): String = when (client) {
    // Product names, the same in every language and the same as the desktop's
    // AI page (apps/desktop/src/features/agents/strings.ts).
    AgentClient.VscodeCopilot -> "VS Code Copilot Chat"
    AgentClient.CopilotCli -> "Copilot CLI"
    AgentClient.Codai -> "Codai"
    AgentClient.Codex -> "Codex"
    AgentClient.ClaudeCode -> "Claude Code"
    AgentClient.Other, AgentClient.Unknown -> stringResource(R.string.agents_client_other)
}

@StringRes
private fun stateLabel(state: SessionState): Int = when (state) {
    SessionState.Working -> R.string.agents_state_working
    SessionState.Waiting -> R.string.agents_state_waiting
    SessionState.Idle -> R.string.agents_state_idle
    SessionState.Ended -> R.string.agents_state_ended
    SessionState.Unknown -> R.string.agents_state_unknown
}
