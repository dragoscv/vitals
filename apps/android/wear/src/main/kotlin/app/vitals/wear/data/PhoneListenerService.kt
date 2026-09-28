package app.vitals.wear.data

import app.vitals.core.wear.WearContract
import app.vitals.wear.VitalsWearApp
import com.google.android.gms.wearable.DataEvent
import com.google.android.gms.wearable.DataEventBuffer
import com.google.android.gms.wearable.DataMapItem
import com.google.android.gms.wearable.MessageEvent
import com.google.android.gms.wearable.WearableListenerService
import kotlinx.coroutines.runBlocking

/**
 * Receives what the phone sends. Play services wakes this service only for
 * `/vitals/` paths (see the manifest), so it costs nothing between updates.
 */
class PhoneListenerService : WearableListenerService() {
    private val repository get() = VitalsWearApp.from(this).repository

    override fun onDataChanged(dataEvents: DataEventBuffer) {
        // The buffer is released when this returns, so it is read here and
        // not in a coroutine; the writes are small and this is a binder thread.
        val events = dataEvents.mapNotNull { event ->
            val path = event.dataItem.uri.path ?: return@mapNotNull null
            when (event.type) {
                DataEvent.TYPE_CHANGED -> {
                    val json = DataMapItem.fromDataItem(event.dataItem).dataMap.getString(WearContract.KEY_JSON)
                        ?: return@mapNotNull null
                    Change.Put(path, json)
                }
                DataEvent.TYPE_DELETED -> Change.Delete(path)
                else -> null
            }
        }
        runBlocking {
            for (change in events) {
                when (change) {
                    is Change.Put -> repository.applyDataItem(change.path, change.json)
                    is Change.Delete -> repository.removeDataItem(change.path)
                }
            }
        }
    }

    override fun onMessageReceived(messageEvent: MessageEvent) {
        when (messageEvent.path) {
            WearContract.PATH_ALERT -> repository.onAlertMessage(messageEvent.data)
            WearContract.PATH_CONTROL_REPLY -> repository.onControlReply(messageEvent.data)
        }
    }

    private sealed interface Change {
        data class Put(val path: String, val json: String) : Change
        data class Delete(val path: String) : Change
    }
}
