package app.vitals.phone.ui.add

import androidx.annotation.StringRes
import app.vitals.core.net.ApiFailure
import app.vitals.core.net.ApiResult
import app.vitals.core.net.LanAddress
import app.vitals.core.net.VitalsClient
import app.vitals.core.pairing.Pairing
import app.vitals.core.pairing.PairingStore
import app.vitals.phone.R
import app.vitals.phone.data.AppGraph
import app.vitals.ui.Readings
import app.vitals.core.net.WakeOnLan
import java.net.URI

sealed interface PairOutcome {
    data class Saved(val pairing: Pairing) : PairOutcome
    data class Refused(@param:StringRes val message: Int, val status: Int? = null) : PairOutcome
}

/**
 * Checks a PC before it is saved. `/health` proves something that speaks
 * Vitals is there (it needs no token); `/summary` proves the token works.
 * Only then is the pairing written, so the list never holds a PC that was
 * wrong from the start.
 */
object Pairer {
    private val TOKEN = Regex("^[A-Za-z0-9_-]{43}$")

    fun normalise(raw: String): String? {
        val text = raw.trim().trimEnd('/')
        val withScheme = if (text.startsWith("http://") || text.startsWith("https://")) text else "http://$text"
        val uri = runCatching { URI(withScheme) }.getOrNull() ?: return null
        val host = uri.host ?: return null
        val port = if (uri.port == -1) 7331 else uri.port
        val hostPart = if (host.contains(':') && !host.startsWith('[')) "[$host]" else host
        return "${uri.scheme}://$hostPart:$port"
    }

    suspend fun pair(graph: AppGraph, rawUrl: String, rawToken: String, label: String?): PairOutcome {
        val baseUrl = normalise(rawUrl) ?: return PairOutcome.Refused(R.string.pairing_bad_address)
        if (!LanAddress.isPrivate(baseUrl)) return PairOutcome.Refused(R.string.pairing_not_private)
        val token = rawToken.trim()
        if (!TOKEN.matches(token)) return PairOutcome.Refused(R.string.pairing_bad_token)

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
        val existing = graph.pairings.load().firstOrNull { it.baseUrl == baseUrl }
        val wake = Readings.wakeMac(summary.system)
        val pairing = Pairing(
            id = existing?.id ?: PairingStore.newId(),
            label = name,
            baseUrl = baseUrl,
            token = token,
            mac = wake?.first ?: existing?.mac,
            broadcast = WakeOnLan.guessBroadcast(wake?.second) ?: existing?.broadcast,
            createdMs = existing?.createdMs ?: System.currentTimeMillis(),
        )
        graph.save(pairing)
        graph.widgetCache.put(pairing.id, pairing.label, summary)
        return PairOutcome.Saved(pairing)
    }

    private fun refusal(f: ApiFailure): PairOutcome.Refused = when (f) {
        is ApiFailure.Unreachable -> PairOutcome.Refused(R.string.pairing_unreachable)
        ApiFailure.Unauthorised -> PairOutcome.Refused(R.string.pairing_unauthorised)
        is ApiFailure.Incompatible -> PairOutcome.Refused(R.string.pairing_incompatible)
        ApiFailure.NotReady -> PairOutcome.Refused(R.string.pairing_not_ready)
        is ApiFailure.Refused -> PairOutcome.Refused(R.string.pairing_unauthorised)
        is ApiFailure.Http -> PairOutcome.Refused(R.string.pairing_failed, f.status)
    }
}
