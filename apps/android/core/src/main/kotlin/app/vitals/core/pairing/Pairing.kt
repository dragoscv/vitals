package app.vitals.core.pairing

import kotlinx.serialization.Serializable

/**
 * One paired PC.
 *
 * [scope] is learnt, not declared: no route reports a token's scope, so a
 * pairing starts as [Scope.Unknown] and becomes [Scope.Read] on the first
 * `403 forbidden` (as the PWA does). The UI hides control actions only for
 * [Scope.Read].
 *
 * [mac] is copied from the PC's connected adapter while it is reachable, so
 * Wake-on-LAN still works once it is asleep and cannot be asked.
 */
@Serializable
data class Pairing(
    val id: String,
    val label: String,
    val baseUrl: String,
    val token: String,
    val scope: Scope = Scope.Unknown,
    val mac: String? = null,
    val broadcast: String? = null,
    val createdMs: Long,
)

@Serializable
enum class Scope { Unknown, Read, Control }
