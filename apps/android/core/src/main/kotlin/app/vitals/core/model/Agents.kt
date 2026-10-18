package app.vitals.core.model

import app.vitals.core.net.ApiFailure
import app.vitals.core.net.ApiResult
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
// The PC serves an allowlist, not the snapshot: sharing is off unless the user
// turns on "Share AI activity with paired devices" (404 until then), and titles,
// commands, purposes and journal text never leave it. So every free-text field
// is optional, the lists default to empty, and the enums are tolerant — a field
// the PC stops sending must hide a line, never fail the whole answer.

@Serializable
data class AgentsSnapshot(
    val takenAtMs: Long = 0,
    val agentqInstalled: Boolean = false,
    val resources: List<AgentqResource> = emptyList(),
    val sessions: List<AgentSession> = emptyList(),
)

@Serializable
data class AgentqResource(
    val name: String,
    val tickets: List<AgentqTicket> = emptyList(),
) {
    val holding: Int get() = tickets.count { it.held }
    val waiting: Int get() = tickets.count { !it.held }
}

@Serializable
data class AgentqTicket(
    val held: Boolean,
    val purpose: String? = null,
    val liveness: Liveness = Liveness.Unknown,
)

// Tolerant like every enum in Wire.kt (AND2-1): a liveness, client or state
// a newer desktop adds decodes to `Unknown` instead of failing the answer,
// which with the generated serializer hid the whole card.

@Serializable(with = Liveness.Serializer::class)
enum class Liveness(override val wire: String) : WireEnum {
    Alive("alive"),
    Stale("stale"),
    Dead("dead"),
    Unknown("unknown");

    object Serializer : TolerantEnumSerializer<Liveness>("Liveness", Liveness.entries.toTypedArray(), Liveness.Unknown)
}

@Serializable
data class AgentSession(
    val id: String = "",
    val client: AgentClient = AgentClient.Unknown,
    val host: String? = null,
    val title: String? = null,
    val repo: String? = null,
    val state: SessionState = SessionState.Unknown,
    val currentTool: String? = null,
)

@Serializable(with = AgentClient.Serializer::class)
enum class AgentClient(override val wire: String) : WireEnum {
    VscodeCopilot("vscodeCopilot"),
    CopilotCli("copilotCli"),
    Codai("codai"),
    Codex("codex"),
    ClaudeCode("claudeCode"),
    Other("other"),

    /** A client this build has never heard of; worded like [Other]. */
    Unknown("unknown");

    object Serializer : TolerantEnumSerializer<AgentClient>("AgentClient", AgentClient.entries.toTypedArray(), AgentClient.Unknown)
}

@Serializable(with = SessionState.Serializer::class)
enum class SessionState(override val wire: String) : WireEnum {
    Working("working"),
    Waiting("waiting"),
    Idle("idle"),
    Ended("ended"),

    /** A state this build has never heard of: shown neutrally, never as "working". */
    Unknown("unknown");

    object Serializer : TolerantEnumSerializer<SessionState>("SessionState", SessionState.entries.toTypedArray(), SessionState.Unknown)
}

/** Sessions a person would call "running": a turn is open. */
val AgentsSnapshot.activeSessions: List<AgentSession>
    get() = sessions.filter { it.state == SessionState.Working || it.state == SessionState.Waiting }

/** Queues worth showing on a phone: someone holds or waits. Busiest first. */
val AgentsSnapshot.busyQueues: List<AgentqResource>
    get() = resources.filter { it.tickets.isNotEmpty() }.sortedByDescending { it.waiting }

/**
 * What the phone's AI card does with one answer, and how long it waits before
 * asking again. Pure, so the polling policy is tested without a screen.
 *
 * Each poll keeps the PC's agent reader awake for thirty seconds, so the
 * interval is the PC's cost as much as the phone's: ten seconds only while
 * something is happening, a minute when nothing is (AND2-5), five minutes
 * when the PC does not share at all — sharing is off by default and the user
 * may turn it on later, so the card keeps listening, slowly.
 */
sealed interface AgentsPoll {
    val delayMs: Long

    /** Show this answer. */
    data class Show(val snapshot: AgentsSnapshot, override val delayMs: Long) : AgentsPoll

    /** Hide the card: the PC does not share, or has nothing worth a card. */
    data class Hide(override val delayMs: Long) : AgentsPoll

    /** A blip (PC asleep, off Wi-Fi): keep whatever is on screen. */
    data class Keep(override val delayMs: Long) : AgentsPoll

    companion object {
        const val BUSY_MS = 10_000L
        const val QUIET_MS = 60_000L
        const val NOT_SHARED_MS = 300_000L

        fun after(result: ApiResult<AgentsSnapshot>): AgentsPoll = when (result) {
            is ApiResult.Ok -> {
                val s = result.value
                when {
                    // Most machines: no agentq, no session. A card that only
                    // ever reads "none" is noise.
                    !s.agentqInstalled && s.sessions.isEmpty() -> Hide(QUIET_MS)
                    s.activeSessions.isEmpty() && s.busyQueues.isEmpty() -> Show(s, QUIET_MS)
                    else -> Show(s, BUSY_MS)
                }
            }
            is ApiResult.Err -> when (val f = result.failure) {
                // 404: sharing is off, or the PC has no AI page. 401/403: this
                // pairing may not read it. Either way not an error for the card;
                // the rest of the screen reports a revoked pairing on its own.
                ApiFailure.Unauthorised -> Hide(NOT_SHARED_MS)
                is ApiFailure.Http -> if (f.status == 404 || f.status == 403) Hide(NOT_SHARED_MS) else Keep(BUSY_MS)
                // A shape this build cannot read will not fix itself in ten seconds.
                is ApiFailure.Incompatible -> Hide(NOT_SHARED_MS)
                else -> Keep(BUSY_MS)
            }
        }
    }
}
