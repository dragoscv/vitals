package app.vitals.wear.data

import android.content.Context
import app.vitals.core.wear.WearContract
import com.google.android.gms.wearable.CapabilityClient
import com.google.android.gms.wearable.DataMapItem
import com.google.android.gms.wearable.Wearable
import kotlinx.coroutines.tasks.await
import kotlin.coroutines.cancellation.CancellationException

/** The watch's side of the Wearable Data Layer. */
class PhoneLink(context: Context) {
    private val capabilities = Wearable.getCapabilityClient(context)
    private val messages = Wearable.getMessageClient(context)
    private val data = Wearable.getDataClient(context)

    /**
     * A reachable phone running the Vitals app, preferring one on the direct
     * Bluetooth link: a node reached through the cloud relay answers in
     * seconds rather than milliseconds and would eat the whole 3 s wait.
     */
    suspend fun nearestPhone(): String? = gms {
        val nodes = capabilities
            .getCapability(WearContract.CAPABILITY_PHONE, CapabilityClient.FILTER_REACHABLE)
            .await()
            .nodes
        (nodes.firstOrNull { it.isNearby } ?: nodes.firstOrNull())?.id
    }

    suspend fun send(nodeId: String, path: String, payload: ByteArray): Boolean =
        gms { messages.sendMessage(nodeId, path, payload).await(); true } ?: false

    /**
     * Every DataItem the phone has put, as `(path, json)`. The listener
     * service only hears about changes; a watch installed after the phone
     * paired would otherwise wait for the next change to see anything.
     */
    suspend fun readAll(): List<Pair<String, String>> = gms {
        val buffer = data.dataItems.await()
        try {
            buffer.mapNotNull { item ->
                val path = item.uri.path ?: return@mapNotNull null
                if (!path.startsWith(PREFIX)) return@mapNotNull null
                DataMapItem.fromDataItem(item).dataMap.getString(WearContract.KEY_JSON)?.let { path to it }
            }
        } finally {
            buffer.release()
        }
    } ?: emptyList()

    private inline fun <T> gms(block: () -> T): T? = try {
        block()
    } catch (e: CancellationException) {
        throw e
    } catch (_: Exception) {
        // No Play services, no paired phone, or the node vanished mid-call:
        // every caller has a direct-to-PC fallback, so absence is not an error.
        null
    }

    companion object {
        const val PREFIX = "/vitals/"
    }
}
