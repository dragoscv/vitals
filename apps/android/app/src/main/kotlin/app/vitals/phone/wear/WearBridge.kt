package app.vitals.phone.wear

import android.content.Context
import app.vitals.core.WireJson
import app.vitals.core.model.Alert
import app.vitals.core.wear.PcState
import app.vitals.core.wear.WatchPairing
import app.vitals.core.wear.WearContract
import com.google.android.gms.common.ConnectionResult
import com.google.android.gms.common.GoogleApiAvailability
import com.google.android.gms.wearable.CapabilityClient
import com.google.android.gms.wearable.PutDataMapRequest
import com.google.android.gms.wearable.Wearable
import kotlinx.coroutines.tasks.await
import kotlinx.serialization.builtins.ListSerializer

/**
 * The phone's half of the Wearable Data Layer (see [WearContract]).
 *
 * Every call swallows failure: a phone without Play services, or without a
 * watch, is the common case, and the rest of the app must not notice.
 */
class WearBridge(context: Context) {
    private val app = context.applicationContext
    private val available: Boolean by lazy {
        GoogleApiAvailability.getInstance().isGooglePlayServicesAvailable(app) == ConnectionResult.SUCCESS
    }
    private val data by lazy { Wearable.getDataClient(app) }
    private val messages by lazy { Wearable.getMessageClient(app) }
    private val capabilities by lazy { Wearable.getCapabilityClient(app) }

    suspend fun syncPairings(pairings: List<WatchPairing>) {
        val json = WireJson.Lenient.encodeToString(ListSerializer(WatchPairing.serializer()), pairings)
        put(WearContract.PATH_PAIRINGS, json, urgent = true)
    }

    /** Urgent only when something changed that the wearer should feel now; otherwise the Data Layer batches. */
    suspend fun putState(state: PcState, urgent: Boolean) {
        val json = WireJson.Lenient.encodeToString(PcState.serializer(), state)
        put(WearContract.PATH_STATE_PREFIX + state.pairingId, json, urgent)
    }

    /** The payload is the [Alert] itself, so the watch can word it without a second round trip. */
    suspend fun sendAlert(alert: Alert) {
        val bytes = WireJson.Lenient.encodeToString(Alert.serializer(), alert).encodeToByteArray()
        watchNodes().forEach { node ->
            runCatching { messages.sendMessage(node, WearContract.PATH_ALERT, bytes).await() }
        }
    }

    suspend fun reply(nodeId: String, path: String, payload: ByteArray) {
        if (!available) return
        runCatching { messages.sendMessage(nodeId, path, payload).await() }
    }

    private suspend fun watchNodes(): List<String> {
        if (!available) return emptyList()
        return runCatching {
            capabilities.getCapability(WearContract.CAPABILITY_WATCH, CapabilityClient.FILTER_REACHABLE)
                .await().nodes.map { it.id }
        }.getOrDefault(emptyList())
    }

    private suspend fun put(path: String, json: String, urgent: Boolean) {
        if (!available) return
        val request = PutDataMapRequest.create(path).apply {
            dataMap.putString(WearContract.KEY_JSON, json)
        }.asPutDataRequest()
        if (urgent) request.setUrgent()
        runCatching { data.putDataItem(request).await() }
    }
}
