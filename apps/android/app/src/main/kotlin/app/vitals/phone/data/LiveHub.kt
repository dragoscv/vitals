package app.vitals.phone.data

import androidx.compose.runtime.Immutable
import app.vitals.core.MachineView
import app.vitals.core.model.Alert
import app.vitals.core.model.Summary
import app.vitals.core.net.ApiFailure
import app.vitals.core.net.ApiResult
import app.vitals.core.net.Backoff
import app.vitals.core.net.StreamClosed
import app.vitals.core.pairing.Pairing
import app.vitals.core.pairing.Scope
import app.vitals.core.wear.PcState
import app.vitals.ui.Readings
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.channelFlow
import kotlinx.coroutines.flow.conflate
import kotlinx.coroutines.flow.shareIn
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import java.util.concurrent.ConcurrentHashMap

/** The last [CAPACITY] readings of one metric. `NaN` marks a sample where it was not measured. */
@Immutable
class Series private constructor(private val values: FloatArray, val size: Int) {
    operator fun get(i: Int): Float = values[i]

    fun plus(v: Float?): Series {
        val next = FloatArray(CAPACITY)
        val keep = minOf(size, CAPACITY - 1)
        System.arraycopy(values, size - keep, next, 0, keep)
        next[keep] = v ?: Float.NaN
        return Series(next, keep + 1)
    }

    companion object {
        const val CAPACITY = 60
        val Empty = Series(FloatArray(CAPACITY), 0)
    }
}

@Immutable
data class LiveState(
    val view: MachineView? = null,
    val cpu: Series = Series.Empty,
    val memory: Series = Series.Empty,
    val gpu: Series = Series.Empty,
    val temperature: Series = Series.Empty,
    val alerts: List<Alert> = emptyList(),
    /** Consecutive failed connection attempts; three or more reads as "unreachable". */
    val failures: Int = 0,
    val unauthorised: Boolean = false,
    val incompatible: Boolean = false,
    val lastSeenMs: Long? = null,
) {
    val unreachable: Boolean get() = failures >= UNREACHABLE_AFTER

    companion object {
        const val UNREACHABLE_AFTER = 3
    }
}

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

    private fun stream(p: Pairing): Flow<LiveState> = channelFlow {
        val client = graph.client(p)
        val lock = Mutex()
        var state = LiveState()

        suspend fun update(f: (LiveState) -> LiveState) = lock.withLock {
            state = f(state)
            send(state)
        }

        send(state)

        // Alerts are a separate, slow poll: the stream carries metrics only.
        launch {
            while (true) {
                when (val r = client.alerts()) {
                    is ApiResult.Ok -> update { it.copy(alerts = r.value) }
                    is ApiResult.Err -> Unit
                }
                waitOrKick(p.id, ALERT_POLL_MS)
            }
        }

        val backoff = Backoff()
        while (true) {
            try {
                client.frames().collect { frame ->
                    backoff.reset()
                    update { s ->
                        val view = MachineView.fold(s.view, frame) ?: return@update s
                        val system = view.system
                        s.copy(
                            view = view,
                            cpu = s.cpu.plus(system.cpu.total),
                            memory = s.memory.plus(Readings.memoryPercent(system)),
                            gpu = s.gpu.plus(Readings.primaryGpu(system)?.utilization),
                            temperature = s.temperature.plus(Readings.cpuTemperature(system)),
                            failures = 0,
                            unauthorised = false,
                            lastSeenMs = System.currentTimeMillis(),
                        )
                    }
                    relay(p, state)
                    rememberMac(p, state)
                }
            } catch (e: CancellationException) {
                throw e
            } catch (e: StreamClosed) {
                handleClosed(client, e) { f -> update(f) }
            }
            waitOrKick(p.id, backoff.next())
        }
    }.conflate()

    private suspend fun handleClosed(
        client: app.vitals.core.net.VitalsClient,
        closed: StreamClosed,
        update: suspend ((LiveState) -> LiveState) -> Unit,
    ) {
        // A WebSocket upgrade refused with 401 surfaces as a failure with the
        // response code; ask /health to tell "off" from "wrong version".
        if (closed.code == 401) {
            update { it.copy(unauthorised = true, failures = it.failures + 1) }
            return
        }
        val health = client.health()
        val incompatible = health is ApiResult.Err && health.failure is ApiFailure.Incompatible
        update { it.copy(failures = it.failures + 1, incompatible = incompatible) }
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
        private const val ALERT_POLL_MS = 15_000L
        private const val RELAY_EVERY_MS = 15_000L
    }
}

/** Keeps the read-only discovery (`403 forbidden`) so control actions hide from then on. */
suspend fun AppGraph.noteFailure(p: Pairing, failure: ApiFailure) {
    if (failure is ApiFailure.Refused && failure.error == app.vitals.core.model.ControlError.Forbidden) {
        learnScope(p.id, Scope.Read)
    }
}
