package app.vitals.core.wear

import app.vitals.core.model.Alert
import app.vitals.core.model.SensorLine
import app.vitals.core.model.Summary
import kotlinx.serialization.Serializable

/**
 * What the phone and the watch say to each other over the Wearable Data
 * Layer (ADR-0033). Both apps share one application id and one signing key;
 * the Data Layer delivers nothing between apps that differ in either.
 *
 * Direction and transport:
 * - phone → watch, DataItem [PATH_PAIRINGS]: the paired PCs, so the watch can
 *   poll a PC directly over Wi-Fi when the phone is away. Tokens cross the
 *   Bluetooth link the Data Layer already encrypts, and are stored on the
 *   watch in its own Keystore-backed [app.vitals.core.pairing.PairingStore].
 * - phone → watch, DataItem [PATH_STATE_PREFIX]`<pairingId>`: the latest
 *   [PcState]. A DataItem, not a message, so a watch that was off-wrist gets
 *   the current state on reconnect without asking.
 * - phone → watch, Message [PATH_ALERT]: a new alert, for a vibration.
 * - watch → phone, Message [PATH_REFRESH]: "I am on screen, send fresh state".
 * - watch → phone, Message [PATH_CONTROL]: an end-task request the phone
 *   forwards with its own token, so a read-only pairing stays read-only.
 */
object WearContract {
    const val PATH_PAIRINGS = "/vitals/pairings"
    const val PATH_STATE_PREFIX = "/vitals/state/"
    const val PATH_ALERT = "/vitals/alert"
    const val PATH_REFRESH = "/vitals/refresh"
    const val PATH_CONTROL = "/vitals/control"
    const val PATH_CONTROL_REPLY = "/vitals/control-reply"

    /** DataMap key holding the JSON payload of every item above. */
    const val KEY_JSON = "json"

    /** The capability the watch app advertises, so the phone finds it. */
    const val CAPABILITY_WATCH = "vitals_watch"
    const val CAPABILITY_PHONE = "vitals_phone"
}

/** A PC as the watch sees it: the summary, the sensors, and what is wrong. */
@Serializable
data class PcState(
    val pairingId: String,
    val label: String,
    /** When the phone last reached the PC, ms since epoch. */
    val fetchedMs: Long,
    /** `null` while the PC cannot be reached; the watch then shows [fetchedMs] as "last seen". */
    val summary: Summary?,
    val sensors: List<SensorLine>,
    val alerts: List<Alert>,
    /** From the pairing: `false` hides end-task on the watch. */
    val canControl: Boolean,
)

@Serializable
data class WatchPairing(
    val id: String,
    val label: String,
    val baseUrl: String,
    val token: String,
    val canControl: Boolean,
)

@Serializable
data class WatchControl(
    val requestId: String,
    val pairingId: String,
    val pid: Int,
    val startTime: Long,
)

@Serializable
data class WatchControlReply(
    val requestId: String,
    val ok: Boolean,
    /** A `ControlError` kind (`forbidden`, `access-denied`, ...) or `unreachable`. */
    val error: String?,
)
