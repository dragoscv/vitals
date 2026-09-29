package app.vitals.core.pairing

import app.vitals.core.model.Summary
import app.vitals.core.net.ApiFailure
import app.vitals.core.net.ApiResult
import app.vitals.core.net.LanAddress
import app.vitals.core.net.VitalsClient
import app.vitals.core.net.WakeOnLan
import java.net.URI

/** Why a PC was not saved. Each surface words these in its own resources. */
enum class PairRefusal { BadAddress, NotPrivate, BadToken, BadCode, Unreachable, Unauthorised, Incompatible, NotReady, Failed }

sealed interface PairCheckResult {
    /** [summary] is the first reading, so a widget or a list can show numbers at once. */
    data class Ok(val pairing: Pairing, val summary: Summary) : PairCheckResult
    data class Refused(val reason: PairRefusal, val status: Int? = null) : PairCheckResult
}

/**
 * Checks a PC before it is saved. `/health` proves something that speaks
 * Vitals is there (it needs no token); `/summary` proves the token works.
 * Only then does the caller write the pairing, so a list never holds a PC
 * that was wrong from the start. Shared by the phone and the TV so the two
 * cannot disagree about what a valid pairing is.
 */
object PairCheck {
    private val TOKEN = Regex("^[A-Za-z0-9_-]{43}$")
    private val CODE = Regex("^[0-9]{6}$")

    fun normalise(raw: String): String? {
        val text = raw.trim().trimEnd('/')
        val withScheme = if (text.startsWith("http://") || text.startsWith("https://")) text else "http://$text"
        val uri = runCatching { URI(withScheme) }.getOrNull() ?: return null
        val host = uri.host ?: return null
        val port = if (uri.port == -1) 7331 else uri.port
        val hostPart = if (host.contains(':') && !host.startsWith('[')) "[$host]" else host
        return "${uri.scheme}://$hostPart:$port"
    }

    /** Digits only, as typed on a remote: spaces and the 3+3 grouping the desktop shows are ignored. */
    fun cleanCode(raw: String): String = raw.filter { it.isDigit() }

    fun isCode(raw: String): Boolean = CODE.matches(cleanCode(raw))

    /**
     * Exchanges a six-digit code shown on the PC for a token, then checks it
     * like any other. The PC decides the scope when it shows the code, and
     * says which in the reply, so this pairing starts knowing it.
     */
    suspend fun withCode(rawUrl: String, rawCode: String, deviceLabel: String, pcLabel: String?, existing: List<Pairing>): PairCheckResult {
        val baseUrl = normalise(rawUrl) ?: return PairCheckResult.Refused(PairRefusal.BadAddress)
        if (!LanAddress.isPrivate(baseUrl)) return PairCheckResult.Refused(PairRefusal.NotPrivate)
        val code = cleanCode(rawCode)
        if (!CODE.matches(code)) return PairCheckResult.Refused(PairRefusal.BadCode)
        val grant = when (val r = VitalsClient.redeem(baseUrl, code, deviceLabel)) {
            is ApiResult.Ok -> r.value
            is ApiResult.Err -> return refusal(r.failure, codeRefused = true)
        }
        return withToken(baseUrl, grant.token, pcLabel, existing, grant.scope)
    }

    suspend fun withToken(
        rawUrl: String,
        rawToken: String,
        label: String?,
        existing: List<Pairing>,
        scope: Scope = Scope.Unknown,
    ): PairCheckResult {
        val baseUrl = normalise(rawUrl) ?: return PairCheckResult.Refused(PairRefusal.BadAddress)
        if (!LanAddress.isPrivate(baseUrl)) return PairCheckResult.Refused(PairRefusal.NotPrivate)
        val token = rawToken.trim()
        if (!TOKEN.matches(token)) return PairCheckResult.Refused(PairRefusal.BadToken)

        val client = VitalsClient(baseUrl, token)
        when (val h = client.health()) {
            is ApiResult.Err -> return refusal(h.failure)
            is ApiResult.Ok -> Unit
        }
        val summary = when (val s = client.summary(top = 1)) {
            is ApiResult.Err -> return refusal(s.failure)
            is ApiResult.Ok -> s.value
        }
        val host = client.host()
        val name = label?.takeIf { it.isNotBlank() }
            ?: (host as? ApiResult.Ok)?.value?.hostname
            ?: URI(baseUrl).host

        // Re-pairing the same PC replaces its old entry rather than listing it twice.
        val previous = existing.firstOrNull { it.baseUrl == baseUrl }
        val wake = wakeTarget(summary)
        val pairing = Pairing(
            id = previous?.id ?: PairingStore.newId(),
            label = name,
            baseUrl = baseUrl,
            token = token,
            scope = scope,
            mac = wake?.first ?: previous?.mac,
            broadcast = WakeOnLan.guessBroadcast(wake?.second) ?: previous?.broadcast,
            createdMs = previous?.createdMs ?: System.currentTimeMillis(),
        )
        return PairCheckResult.Ok(pairing, summary)
    }

    private fun wakeTarget(summary: Summary): Pair<String, String?>? {
        val candidates = summary.system.networks.filter { it.connected && it.mac != null }
        val nic = candidates.firstOrNull { it.kind == app.vitals.core.model.NetworkKind.Ethernet } ?: candidates.firstOrNull()
        val mac = nic?.mac ?: return null
        return mac to nic.ipv4
    }

    private fun refusal(f: ApiFailure, codeRefused: Boolean = false): PairCheckResult.Refused = when (f) {
        is ApiFailure.Unreachable -> PairCheckResult.Refused(PairRefusal.Unreachable)
        ApiFailure.Unauthorised -> PairCheckResult.Refused(PairRefusal.Unauthorised)
        is ApiFailure.Incompatible -> PairCheckResult.Refused(PairRefusal.Incompatible)
        ApiFailure.NotReady -> PairCheckResult.Refused(PairRefusal.NotReady)
        is ApiFailure.Refused -> PairCheckResult.Refused(PairRefusal.Unauthorised)
        // 403 from /pair is the one answer for wrong, expired and burnt codes.
        is ApiFailure.Http -> if (codeRefused && f.status == 403) {
            PairCheckResult.Refused(PairRefusal.BadCode)
        } else {
            PairCheckResult.Refused(PairRefusal.Failed, f.status)
        }
    }
}
