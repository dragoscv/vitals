package app.vitals.phone.ui.add

import androidx.annotation.StringRes
import app.vitals.core.pairing.PairCheck
import app.vitals.core.pairing.PairCheckResult
import app.vitals.core.pairing.PairRefusal
import app.vitals.core.pairing.Pairing
import app.vitals.phone.R
import app.vitals.phone.data.AppGraph

sealed interface PairOutcome {
    data class Saved(val pairing: Pairing) : PairOutcome
    data class Refused(@param:StringRes val message: Int, val status: Int? = null) : PairOutcome
}

/**
 * Checks a PC with [PairCheck] (shared with the TV) and saves it only if it
 * passes, so the list never holds a PC that was wrong from the start.
 */
object Pairer {
    suspend fun pair(graph: AppGraph, rawUrl: String, rawToken: String, label: String?): PairOutcome {
        return when (val r = PairCheck.withToken(rawUrl, rawToken, label, graph.pairings.load())) {
            is PairCheckResult.Ok -> {
                graph.save(r.pairing)
                graph.widgetCache.put(r.pairing.id, r.pairing.label, r.summary)
                PairOutcome.Saved(r.pairing)
            }
            is PairCheckResult.Refused -> PairOutcome.Refused(message(r.reason), r.status)
        }
    }

    @StringRes
    private fun message(reason: PairRefusal): Int = when (reason) {
        PairRefusal.BadAddress -> R.string.pairing_bad_address
        PairRefusal.NotPrivate -> R.string.pairing_not_private
        PairRefusal.BadToken, PairRefusal.BadCode -> R.string.pairing_bad_token
        PairRefusal.Unreachable -> R.string.pairing_unreachable
        PairRefusal.Unauthorised -> R.string.pairing_unauthorised
        PairRefusal.Incompatible -> R.string.pairing_incompatible
        PairRefusal.NotReady -> R.string.pairing_not_ready
        PairRefusal.Failed -> R.string.pairing_failed
    }
}
