package app.vitals.core.model

import kotlinx.serialization.ExperimentalSerializationApi
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonClassDiscriminator

// `crates/vitals-server/src/control.rs`: tagged by `action`, kebab-case.
// Every request carries a ProcessKey, never a bare PID, so a tap that took a
// second cannot end whatever recycled the PID in the meantime.

@OptIn(ExperimentalSerializationApi::class)
@Serializable
@JsonClassDiscriminator("action")
sealed interface ControlRequest {
    val key: ProcessKey

    @Serializable @SerialName("terminate")
    data class Terminate(override val key: ProcessKey) : ControlRequest

    @Serializable @SerialName("suspend")
    data class Suspend(override val key: ProcessKey) : ControlRequest

    @Serializable @SerialName("resume")
    data class Resume(override val key: ProcessKey) : ControlRequest

    @Serializable @SerialName("set-priority")
    data class SetPriority(override val key: ProcessKey, val priority: Priority) : ControlRequest

    @Serializable @SerialName("set-efficiency-mode")
    data class SetEfficiencyMode(override val key: ProcessKey, val enabled: Boolean) : ControlRequest
}

@Serializable
enum class Priority {
    @SerialName("idle") Idle,
    @SerialName("below-normal") BelowNormal,
    @SerialName("normal") Normal,
    @SerialName("above-normal") AboveNormal,
    @SerialName("high") High,
    @SerialName("realtime") Realtime,
}

/** Why a request was refused. `Forbidden` = read-only token; `AccessDenied` = Windows said no. */
@OptIn(ExperimentalSerializationApi::class)
@Serializable
@JsonClassDiscriminator("kind")
sealed interface ControlError {
    @Serializable @SerialName("forbidden") data object Forbidden : ControlError
    @Serializable @SerialName("not-found") data object NotFound : ControlError
    @Serializable @SerialName("access-denied") data object AccessDenied : ControlError
    @Serializable @SerialName("unsupported") data class Unsupported(val message: String) : ControlError
    @Serializable @SerialName("internal") data class Internal(val message: String) : ControlError
}
