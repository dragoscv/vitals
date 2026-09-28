package app.vitals.phone.wear

import app.vitals.core.WireJson
import app.vitals.core.model.ControlError
import app.vitals.core.model.ControlRequest
import app.vitals.core.model.ProcessKey
import app.vitals.core.net.ApiFailure
import app.vitals.core.net.ApiResult
import app.vitals.core.net.getOrNull
import app.vitals.core.wear.PcState
import app.vitals.core.wear.WatchControl
import app.vitals.core.wear.WatchControlReply
import app.vitals.core.wear.WearContract
import app.vitals.phone.data.canControl
import app.vitals.phone.data.noteFailure
import app.vitals.phone.graph
import com.google.android.gms.wearable.MessageEvent
import com.google.android.gms.wearable.WearableListenerService
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.cancel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.launch

/**
 * The watch's requests, answered even when the phone app is closed: Play
 * services binds this service on demand and unbinds it when idle, so it
 * costs nothing between messages.
 */
class PhoneListenerService : WearableListenerService() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    override fun onMessageReceived(event: MessageEvent) {
        when (event.path) {
            WearContract.PATH_REFRESH -> scope.launch { refreshAll() }
            WearContract.PATH_CONTROL -> scope.launch { control(event.sourceNodeId, event.data) }
        }
    }

    private suspend fun refreshAll() {
        val graph = applicationContext.graph
        val pairings = graph.pairings.load()
        coroutineScope {
            pairings.map { p ->
                async {
                    val client = graph.client(p)
                    val summary = async { client.summary(top = 5) }
                    val sensors = async { client.sensors() }
                    val alerts = async { client.alerts() }
                    val state = PcState(
                        pairingId = p.id,
                        label = p.label,
                        fetchedMs = System.currentTimeMillis(),
                        summary = summary.await().getOrNull(),
                        sensors = sensors.await().getOrNull().orEmpty(),
                        alerts = alerts.await().getOrNull().orEmpty(),
                        canControl = p.canControl(),
                    )
                    // Urgent: the watch asked because it is on screen right now.
                    graph.wear.putState(state, urgent = true)
                }
            }.awaitAll()
        }
    }

    private suspend fun control(nodeId: String, data: ByteArray) {
        val graph = applicationContext.graph
        val request = runCatching {
            WireJson.Lenient.decodeFromString(WatchControl.serializer(), data.decodeToString())
        }.getOrNull() ?: return
        val pairing = graph.pairings.load().firstOrNull { it.id == request.pairingId }
        val reply = when {
            pairing == null -> WatchControlReply(request.requestId, ok = false, error = "not-found")
            // Checked here too: the phone holds the token, so the phone is
            // what keeps a read-only pairing read-only.
            !pairing.canControl() -> WatchControlReply(request.requestId, ok = false, error = "forbidden")
            else -> {
                val key = ProcessKey(request.pid, request.startTime)
                when (val r = graph.client(pairing).control(ControlRequest.Terminate(key))) {
                    is ApiResult.Ok -> WatchControlReply(request.requestId, ok = true, error = null)
                    is ApiResult.Err -> {
                        graph.noteFailure(pairing, r.failure)
                        WatchControlReply(request.requestId, ok = false, error = kindOf(r.failure))
                    }
                }
            }
        }
        val bytes = WireJson.Lenient.encodeToString(WatchControlReply.serializer(), reply).encodeToByteArray()
        graph.wear.reply(nodeId, WearContract.PATH_CONTROL_REPLY, bytes)
    }

    private fun kindOf(f: ApiFailure): String = when (f) {
        is ApiFailure.Refused -> when (f.error) {
            ControlError.Forbidden -> "forbidden"
            ControlError.AccessDenied -> "access-denied"
            ControlError.NotFound -> "not-found"
            is ControlError.Unsupported -> "unsupported"
            is ControlError.Internal -> "internal"
        }
        is ApiFailure.Unreachable -> "unreachable"
        ApiFailure.Unauthorised -> "unauthorised"
        is ApiFailure.Incompatible -> "incompatible"
        ApiFailure.NotReady -> "not-ready"
        is ApiFailure.Http -> "http-${f.status}"
    }

    override fun onDestroy() {
        scope.cancel()
        super.onDestroy()
    }
}
