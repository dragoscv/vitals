package app.vitals.ui

import androidx.compose.runtime.Immutable
import app.vitals.core.MachineView
import app.vitals.core.model.Alert
import app.vitals.core.net.ApiFailure
import app.vitals.core.net.ApiResult
import app.vitals.core.net.Backoff
import app.vitals.core.net.StreamClosed
import app.vitals.core.net.VitalsClient
import kotlinx.coroutines.CancellationException
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
        launch {
            while (true) {
                when (val r = client.alerts()) {
                    is ApiResult.Ok -> update { it.copy(alerts = r.value) }
                    is ApiResult.Err -> Unit
                }
                wait(ALERT_POLL_MS)
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
                    onFrame(state)
                }
            } catch (e: CancellationException) {
                throw e
            } catch (e: StreamClosed) {
                handleClosed(client, e) { f -> update(f) }
            }
            wait(backoff.next())
        }
    }.conflate()

    private suspend fun handleClosed(
        client: VitalsClient,
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
}
