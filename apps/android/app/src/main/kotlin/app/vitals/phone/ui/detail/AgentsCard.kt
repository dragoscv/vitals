package app.vitals.phone.ui.detail

import androidx.annotation.StringRes
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.repeatOnLifecycle
import app.vitals.core.model.AgentClient
import app.vitals.core.model.AgentsSnapshot
import app.vitals.core.model.SessionState
import app.vitals.core.model.activeSessions
import app.vitals.core.model.busyQueues
import app.vitals.core.net.ApiFailure
import app.vitals.core.net.ApiResult
import app.vitals.core.pairing.Pairing
import app.vitals.phone.R
import app.vitals.phone.graph
import app.vitals.phone.ui.components.InfoRow
import app.vitals.phone.ui.components.SectionCard
import app.vitals.ui.Palette
import kotlinx.coroutines.delay

/** How many rows of each list a phone shows; the desktop has the full page. */
private const val MAX_ROWS = 4

/**
 * The PC's AI page, read-only (ADR-0038, S18-07): which agents are working
 * and which agentq queues have someone waiting.
 *
 * Polled every ten seconds while the screen is in front, and each poll keeps
 * the PC's reader awake for thirty — so nothing is read on the PC once the
 * phone looks away. Hidden entirely when the PC has no AI page (`404`), or
 * when it has nothing to say and agentq is not installed: a card that only
 * ever reads "none" on most machines is noise.
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
    var offered by remember(pairing.id) { mutableStateOf(true) }

    LaunchedEffect(pairing.id, lifecycle) {
        lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) {
            val client = graph.client(pairing)
            while (offered) {
                when (val r = client.agents()) {
                    is ApiResult.Ok -> snapshot = r.value
                    // Keep the last answer through a blip; only "not here" hides.
                    is ApiResult.Err -> if (r.failure == ApiFailure.Http(404)) offered = false
                }
                delay(10_000)
            }
        }
    }

    val s = snapshot ?: return
    if (!offered) return
    val active = s.activeSessions
    val queues = s.busyQueues
    if (!s.agentqInstalled && s.sessions.isEmpty()) return

    SectionCard(stringResource(R.string.agents_title), accent = Palette.Network, modifier = modifier) {
        if (active.isEmpty()) {
            Text(stringResource(R.string.agents_none_working))
        }
        active.take(MAX_ROWS).forEach { session ->
            val what = session.title ?: session.repo ?: clientName(session.client)
            InfoRow(
                "${clientName(session.client)} · $what",
                stringResource(stateLabel(session.state)),
                valueColour = if (session.state == SessionState.Working) Palette.Ok else Palette.Warn,
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
            InfoRow(
                q.name,
                stringResource(R.string.agents_queue_value, q.holding, q.waiting),
                valueColour = if (q.waiting > 0) Palette.Warn else Color.Unspecified,
            )
        }
    }
}

@Composable
private fun clientName(client: AgentClient): String = when (client) {
    // Product names, the same in every language.
    AgentClient.VscodeCopilot -> "VS Code Copilot"
    AgentClient.CopilotCli -> "Copilot CLI"
    AgentClient.Codai -> "Codai"
    AgentClient.Codex -> "Codex"
    AgentClient.ClaudeCode -> "Claude Code"
    AgentClient.Other -> stringResource(R.string.agents_client_other)
}

@StringRes
private fun stateLabel(state: SessionState): Int = when (state) {
    SessionState.Working -> R.string.agents_state_working
    SessionState.Waiting -> R.string.agents_state_waiting
    SessionState.Idle -> R.string.agents_state_idle
    SessionState.Ended -> R.string.agents_state_ended
}
