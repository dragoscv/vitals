package app.vitals.core.model

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

// `GET /api/v1/agents` (ADR-0039, S18-07), as far as the phone's card reads it.
//
// Deliberately partial, unlike Wire.kt. The full `AgentsSnapshot` is thirteen
// types that change with agentq's file format; the phone shows a summary —
// who is working, which queues have waiters — and a field-for-field mirror
// would be a second drift surface for numbers no screen here renders. These
// classes are decoded with `WireJson.Lenient`, and `ContractTest` decodes the
// JSON Rust writes so a renamed field the card *does* read still goes red.
//
// Paths and anything credential-like arrive redacted by the server.

@Serializable
data class AgentsSnapshot(
    val takenAtMs: Long,
    val agentqInstalled: Boolean,
    val resources: List<AgentqResource>,
    val sessions: List<AgentSession>,
)

@Serializable
data class AgentqResource(
    val name: String,
    val tickets: List<AgentqTicket>,
) {
    val holding: Int get() = tickets.count { it.held }
    val waiting: Int get() = tickets.count { !it.held }
}

@Serializable
data class AgentqTicket(
    val held: Boolean,
    val purpose: String? = null,
    val liveness: Liveness,
)

@Serializable
enum class Liveness {
    @SerialName("alive") Alive,
    @SerialName("stale") Stale,
    @SerialName("dead") Dead,
    @SerialName("unknown") Unknown,
}

@Serializable
data class AgentSession(
    val id: String,
    val client: AgentClient,
    val host: String,
    val title: String? = null,
    val repo: String? = null,
    val state: SessionState,
    val currentTool: String? = null,
)

@Serializable
enum class AgentClient {
    @SerialName("vscodeCopilot") VscodeCopilot,
    @SerialName("copilotCli") CopilotCli,
    @SerialName("codai") Codai,
    @SerialName("codex") Codex,
    @SerialName("claudeCode") ClaudeCode,
    @SerialName("other") Other,
}

@Serializable
enum class SessionState {
    @SerialName("working") Working,
    @SerialName("waiting") Waiting,
    @SerialName("idle") Idle,
    @SerialName("ended") Ended,
}

/** Sessions a person would call "running": a turn is open. */
val AgentsSnapshot.activeSessions: List<AgentSession>
    get() = sessions.filter { it.state == SessionState.Working || it.state == SessionState.Waiting }

/** Queues worth showing on a phone: someone holds or waits. Busiest first. */
val AgentsSnapshot.busyQueues: List<AgentqResource>
    get() = resources.filter { it.tickets.isNotEmpty() }.sortedByDescending { it.waiting }
