package app.vitals.phone.data

import android.app.Application
import app.vitals.core.net.VitalsClient
import app.vitals.core.pairing.Pairing
import app.vitals.core.pairing.PairingStore
import app.vitals.core.pairing.Scope
import app.vitals.core.wear.WatchPairing
import app.vitals.device.AndroidDeviceMonitor
import app.vitals.device.DeviceMonitor
import app.vitals.phone.service.DeviceWork
import app.vitals.phone.wear.WearBridge
import app.vitals.phone.widget.WidgetCache
import app.vitals.phone.widget.WidgetWork
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch
import java.util.concurrent.ConcurrentHashMap

class AppGraph(private val app: Application) {
    val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    val pairings = PairingStore(app)
    val settings = SettingsStore(app)
    val widgetCache = WidgetCache(app)
    val wear = WearBridge(app)
    val live = LiveHub(this)
    val device: DeviceMonitor = AndroidDeviceMonitor(app)
    val deviceLive = DeviceHub(device, settings, scope)

    private val readyState = MutableStateFlow(false)

    /** False until the encrypted pairing file has been read, so "no PCs yet" is never shown by mistake. */
    val ready: StateFlow<Boolean> = readyState.asStateFlow()

    private val clients = ConcurrentHashMap<String, VitalsClient>()

    fun client(p: Pairing): VitalsClient = clients.getOrPut(clientKey(p)) { VitalsClient(p.baseUrl, p.token) }

    fun start() {
        scope.launch {
            pairings.load()
            readyState.value = true
            // Mapped before distinct so a MAC learnt in the background does
            // not re-send every token across the Bluetooth link.
            pairings.pairings
                .map { list -> list.map { it.toWatch() } }
                .distinctUntilChanged()
                .collect { wear.syncPairings(it) }
        }
        scope.launch { WidgetWork.schedule(app) }
        scope.launch {
            settings.deviceHistory.distinctUntilChanged().collect { on ->
                if (on) DeviceWork.schedule(app) else DeviceWork.cancel(app)
            }
        }
    }

    suspend fun learnScope(id: String, scope: Scope) {
        val current = pairings.load().firstOrNull { it.id == id } ?: return
        if (current.scope == scope) return
        pairings.update { list -> list.map { if (it.id == id) it.copy(scope = scope) else it } }
    }

    suspend fun rememberMac(id: String, mac: String, broadcast: String?) {
        pairings.update { list ->
            list.map { if (it.id == id) it.copy(mac = mac, broadcast = broadcast ?: it.broadcast) else it }
        }
    }

    suspend fun rename(id: String, label: String) {
        pairings.update { list -> list.map { if (it.id == id) it.copy(label = label) else it } }
    }

    suspend fun forget(id: String) {
        pairings.remove(id)
        settings.forget(id)
        clients.keys.removeAll { it.startsWith("$id|") }
    }

    /** Replaces a pairing to the same PC in place, so re-pairing keeps its position and id. */
    suspend fun save(pairing: Pairing) {
        pairings.update { list ->
            if (list.any { it.id == pairing.id }) list.map { if (it.id == pairing.id) pairing else it } else list + pairing
        }
    }

    companion object {
        fun clientKey(p: Pairing) = "${p.id}|${p.baseUrl}|${p.token.hashCode()}"
    }
}

/** Unknown counts as able: the PC is the authority and answers 403 if it is not. */
fun Pairing.canControl(): Boolean = scope != Scope.Read

fun Pairing.toWatch() = WatchPairing(id, label, baseUrl, token, canControl())
