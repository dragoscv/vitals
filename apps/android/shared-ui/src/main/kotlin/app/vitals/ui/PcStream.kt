package app.vitals.ui

import androidx.compose.runtime.Immutable
import app.vitals.core.MachineView
import app.vitals.core.model.Alert
import app.vitals.core.net.ApiFailure
import app.vitals.core.net.ApiResult
import app.vitals.core.net.Backoff
import app.vitals.core.net.LanNetwork
import app.vitals.core.net.StreamClosed
import app.vitals.core.net.VitalsClient
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.async
import kotlinx.coroutines.cancelChildren
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.selects.select
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.channelFlow
import kotlinx.coroutines.flow.conflate
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

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
    /** The PC refused the token (401/403). Reconnecting cannot fix it; only pairing again can. */
    val unauthorised: Boolean = false,
    val incompatible: Boolean = false,
    /** This device is on mobile data or offline, so nothing was sent ([LanNetwork]). */
    val offLan: Boolean = false,
    val lastSeenMs: Long? = null,
) {
    val unreachable: Boolean get() = failures >= UNREACHABLE_AFTER || offLan

    companion object {
        const val UNREACHABLE_AFTER = 3
    }
}

/**
 * One PC's live state: the WebSocket folded into a [MachineView], a minute of
 * headline readings for the sparklines, and a slow alerts poll beside it.
 * Reconnects with backoff; [wait] is how the caller lets a user skip the
 * backoff (pull to refresh on the phone, a button on the TV).
 *
 * The phone and the TV share this so they cannot disagree about when a PC is
 * unreachable or what its CPU was a second ago. [onFrame] is the phone's hook
 * for relaying to the watch and the widgets; the TV passes nothing.
 */
object PcStream {
    private const val ALERT_POLL_MS = 15_000L

    fun stream(
        client: VitalsClient,
        wait: suspend (ms: Long) -> Unit,
        onFrame: suspend (LiveState) -> Unit = {},
    ): Flow<LiveState> = channelFlow {
        val lock = Mutex()
        var state = LiveState()

        suspend fun update(f: (LiveState) -> LiveState) = lock.withLock {
            state = f(state)
            send(state)
        }

        send(state)

        // Alerts are a separate, slow poll: the stream carries metrics only.
        // It sleeps while the stream knows asking is pointless (off the LAN,
        // or a revoked token) rather than knocking every fifteen seconds.
        launch {
            while (true) {
                if (!state.unauthorised && !state.offLan) {
                    when (val r = client.alerts()) {
                        is ApiResult.Ok -> update { it.copy(alerts = r.value) }
                        is ApiResult.Err -> Unit
                    }
                }
                wait(ALERT_POLL_MS)
            }
        }

        val backoff = Backoff()
        while (true) {
            if (!LanNetwork.onLan) {
                update { it.copy(offLan = true) }
                // Reconnect the moment Wi-Fi returns, or when the user asks.
                waitForEither(wait) { LanNetwork.local.first { it } }
                update { it.copy(offLan = false, failures = 0) }
                backoff.reset()
                continue
            }
            if (state.unauthorised) {
                // A revoked token stays revoked: no backoff loop hammering
                // the PC with a secret it refuses. Only an explicit retry
                // (pull to refresh, the TV's button) tries again.
                wait(Long.MAX_VALUE)
                update { it.copy(unauthorised = false) }
                continue
            }
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
                    onFrame(state)
                }
            } catch (e: CancellationException) {
                throw e
            } catch (e: StreamClosed) {
                handleClosed(client, e) { f -> update(f) }
            }
            if (state.unauthorised || !LanNetwork.onLan) continue
            // Wi-Fi dropping mid-stream ends the backoff early when it returns.
            val delayMs = backoff.next()
            waitForEither({ wait(delayMs) }) { LanNetwork.local.first { !it }; LanNetwork.local.first { it } }
        }
    }.conflate()

    /** Returns when [a] (given [Long.MAX_VALUE] as its wait) or [b] returns, whichever is first. */
    private suspend fun waitForEither(a: suspend (Long) -> Unit, b: suspend () -> Unit) = coroutineScope {
        val first = async { a(Long.MAX_VALUE) }
        val second = async { b() }
        select<Unit> {
            first.onAwait {}
            second.onAwait {}
        }
        coroutineContext.cancelChildren()
    }

    private suspend fun handleClosed(
        client: VitalsClient,
        closed: StreamClosed,
        update: suspend ((LiveState) -> LiveState) -> Unit,
    ) {
        // A WebSocket upgrade refused with 401 surfaces as a failure with the
        // response code; ask /health to tell "off" from "wrong version".
        if (closed.code == VitalsClient.CLOSE_OFF_LAN) return
        if (closed.code == VitalsClient.CLOSE_UNREADABLE) {
            update { it.copy(failures = it.failures + 1, incompatible = true) }
            return
        }
        if (closed.code == 401 || closed.code == 403) {
            update { it.copy(unauthorised = true, failures = it.failures + 1) }
            return
        }
        val health = client.health()
        val incompatible = health is ApiResult.Err && health.failure is ApiFailure.Incompatible
        update { it.copy(failures = it.failures + 1, incompatible = incompatible) }
    }
}
