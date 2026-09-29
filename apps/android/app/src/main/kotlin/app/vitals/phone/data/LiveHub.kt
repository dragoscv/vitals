package app.vitals.phone.data

import app.vitals.core.model.Summary
import app.vitals.core.net.ApiFailure
import app.vitals.core.pairing.Pairing
import app.vitals.core.pairing.Scope
import app.vitals.core.wear.PcState
import app.vitals.ui.LiveState
import app.vitals.ui.PcStream
import app.vitals.ui.Readings
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.shareIn
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import java.util.concurrent.ConcurrentHashMap

/**
 * One live stream per paired PC, shared by every screen that shows it.
 *
 * `WhileSubscribed` with a short grace is the whole background policy: the
 * screens collect with the lifecycle, so when the app goes to the
 * background the last subscriber leaves, the socket closes five seconds
 * later, and nothing streams until the user comes back. The grace exists so
 * a rotation or a hop from list to detail does not tear the socket down and
 * pay for a 250 KB keyframe again.
 */
class LiveHub(private val graph: AppGraph) {
    private val flows = ConcurrentHashMap<String, SharedFlow<LiveState>>()
    private val relayed = ConcurrentHashMap<String, Long>()
    private val relayLock = Mutex()
    private val kicks = MutableSharedFlow<String>(extraBufferCapacity = 16)

    /** Pull-to-refresh: reconnect now instead of waiting out the backoff, and re-read alerts. */
    fun kick(pairingId: String) {
        kicks.tryEmit(pairingId)
    }

    private suspend fun waitOrKick(pairingId: String, ms: Long) {
        withTimeoutOrNull(ms) { kicks.first { it == pairingId } }
    }

    fun observe(p: Pairing): SharedFlow<LiveState> =
        flows.getOrPut(AppGraph.clientKey(p)) {
            stream(p).shareIn(graph.scope, SharingStarted.WhileSubscribed(STOP_GRACE_MS), replay = 1)
        }

    private fun stream(p: Pairing): Flow<LiveState> =
        PcStream.stream(graph.client(p), wait = { ms -> waitOrKick(p.id, ms) }) { state ->
            relay(p, state)
            rememberMac(p, state)
        }

    /** The PC is off exactly when Wake-on-LAN is needed, so its MAC is copied while it is on. */
    private suspend fun rememberMac(p: Pairing, s: LiveState) {
        val system = s.view?.system ?: return
        val (mac, ip) = Readings.wakeMac(system) ?: return
        val current = graph.pairings.pairings.value.firstOrNull { it.id == p.id } ?: return
        if (current.mac == mac) return
        graph.rememberMac(p.id, mac, app.vitals.core.net.WakeOnLan.guessBroadcast(ip))
    }

    /** While the app is open the phone already has fresh data; hand it to the watch and widgets. */
    private suspend fun relay(p: Pairing, s: LiveState) {
        val view = s.view ?: return
        val now = System.currentTimeMillis()
        relayLock.withLock {
            if (now - (relayed[p.id] ?: 0L) < RELAY_EVERY_MS) return
            relayed[p.id] = now
        }
        val summary = Summary(
            seq = view.seq,
            timestampMs = view.timestampMs,
            system = view.system,
            top = view.processes.values.sortedByDescending { it.cpu }.take(5),
            processCount = view.processes.size,
        )
        val current = graph.pairings.pairings.value.firstOrNull { it.id == p.id } ?: p
        graph.wear.putState(
            PcState(p.id, current.label, now, summary, emptyList(), s.alerts, current.scope != Scope.Read),
            urgent = false,
        )
        graph.widgetCache.put(p.id, current.label, summary.copy(top = summary.top.take(3)))
    }

    companion object {
        private const val STOP_GRACE_MS = 5_000L
        private const val RELAY_EVERY_MS = 15_000L
    }
}

/** Keeps the read-only discovery (`403 forbidden`) so control actions hide from then on. */
suspend fun AppGraph.noteFailure(p: Pairing, failure: ApiFailure) {
    if (failure is ApiFailure.Refused && failure.error == app.vitals.core.model.ControlError.Forbidden) {
        learnScope(p.id, Scope.Read)
    }
}
