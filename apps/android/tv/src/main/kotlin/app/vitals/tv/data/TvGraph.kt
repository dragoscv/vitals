package app.vitals.tv.data

import android.app.Application
import android.content.Context
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.preferencesDataStore
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import app.vitals.core.model.ControlError
import app.vitals.core.net.ApiFailure
import app.vitals.core.net.VitalsClient
import app.vitals.core.net.WakeOnLan
import app.vitals.core.pairing.Pairing
import app.vitals.core.pairing.PairingStore
import app.vitals.core.pairing.Scope
import app.vitals.device.AndroidDeviceMonitor
import app.vitals.device.DeviceMonitor
import app.vitals.ui.DeviceLive
import app.vitals.ui.LiveState
import app.vitals.ui.PcStream
import app.vitals.ui.Readings
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.runningFold
import kotlinx.coroutines.flow.shareIn
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.TimeUnit

private val Context.tvSettings by preferencesDataStore(name = "tv-settings")

/**
 * Everything the TV app holds, one per process: the paired PCs, one live
 * stream per PC however many screens show it, and this TV's own monitor.
 *
 * Every stream is `WhileSubscribed`: the screens collect with the lifecycle,
 * so when the user presses Home the sockets close a few seconds later and
 * the TV stops sampling itself. A TV is left on for hours; nothing here may
 * run just because the app was opened once.
 */
class TvGraph(private val app: Application) {
    val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    val pairings = PairingStore(app)
    val device: DeviceMonitor = AndroidDeviceMonitor(app)

    private val readyState = MutableStateFlow(false)
    val ready: StateFlow<Boolean> = readyState.asStateFlow()

    private val clients = ConcurrentHashMap<String, VitalsClient>()
    private val flows = ConcurrentHashMap<String, SharedFlow<LiveState>>()
    private val kicks = MutableSharedFlow<String>(extraBufferCapacity = 16)
    private var lastRecorded = 0L

    /** Record this TV's history: on by default, as on the phone, because a history that starts only once found is useless. */
    val deviceHistory: Flow<Boolean> = app.tvSettings.data.map { it[DEVICE_HISTORY] ?: true }

    val deviceLive: Flow<DeviceLive> = device.snapshots(1_000)
        .onEach { s ->
            // One history point a minute while someone is looking, so the
            // chart has detail for the time the TV was actually watched.
            if (s.timestampMs - lastRecorded >= 60_000 && s.cpu.load != null) {
                lastRecorded = s.timestampMs
                scope.launch { if (deviceHistory.first()) device.record(s) }
            }
        }
        .runningFold(DeviceLive()) { acc, s -> acc.next(s) }
        .shareIn(scope, SharingStarted.WhileSubscribed(2_000), replay = 1)

    fun start() {
        scope.launch {
            pairings.load()
            readyState.value = true
        }
        scope.launch {
            deviceHistory.distinctUntilChanged().collect { on -> if (on) HistoryWork.schedule(app) else HistoryWork.cancel(app) }
        }
    }

    fun client(p: Pairing): VitalsClient = clients.getOrPut(key(p)) { VitalsClient(p.baseUrl, p.token) }

    fun live(p: Pairing): SharedFlow<LiveState> = flows.getOrPut(key(p)) {
        PcStream.stream(client(p), wait = { ms -> withTimeoutOrNull(ms) { kicks.first { it == p.id } } }) { state ->
            rememberMac(p, state)
        }.shareIn(scope, SharingStarted.WhileSubscribed(5_000), replay = 1)
    }

    /** Reconnect now instead of waiting out the backoff. */
    fun kick(id: String) {
        kicks.tryEmit(id)
    }

    suspend fun setDeviceHistory(on: Boolean) {
        app.tvSettings.edit { it[DEVICE_HISTORY] = on }
    }

    suspend fun save(pairing: Pairing) = pairings.update { list ->
        if (list.any { it.id == pairing.id }) list.map { if (it.id == pairing.id) pairing else it } else list + pairing
    }

    suspend fun forget(id: String) {
        pairings.remove(id)
        clients.keys.removeAll { it.startsWith("$id|") }
        flows.keys.removeAll { it.startsWith("$id|") }
    }

    /** A `403 forbidden` from control means the pairing is read-only; remember it so the actions hide. */
    suspend fun noteFailure(p: Pairing, failure: ApiFailure) {
        if (failure is ApiFailure.Refused && failure.error == ControlError.Forbidden && p.scope != Scope.Read) {
            pairings.update { list -> list.map { if (it.id == p.id) it.copy(scope = Scope.Read) else it } }
        }
    }

    /** The PC is off exactly when Wake-on-LAN is needed, so its MAC is copied while it is on. */
    private suspend fun rememberMac(p: Pairing, s: LiveState) {
        val system = s.view?.system ?: return
        val (mac, ip) = Readings.wakeMac(system) ?: return
        val current = pairings.pairings.value.firstOrNull { it.id == p.id } ?: return
        if (current.mac == mac) return
        val broadcast = WakeOnLan.guessBroadcast(ip)
        pairings.update { list -> list.map { if (it.id == p.id) it.copy(mac = mac, broadcast = broadcast ?: it.broadcast) else it } }
    }

    private fun key(p: Pairing) = "${p.id}|${p.baseUrl}|${p.token.hashCode()}"

    private companion object {
        val DEVICE_HISTORY = booleanPreferencesKey("device_history")
    }
}

/** Unknown counts as able: the PC is the authority and answers 403 if it is not. */
fun Pairing.canControl(): Boolean = scope != Scope.Read

/**
 * One history point for this TV every 15 minutes (WorkManager's floor). A TV
 * has no notification shade worth the name, so unlike the phone this raises
 * nothing; the alerts show on the TV's own screen when it is opened.
 */
class HistoryWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
    override suspend fun doWork(): Result {
        val graph = applicationContext.tvGraph
        if (!graph.deviceHistory.first()) return Result.success()
        graph.device.recordSample()
        graph.device.pruneHistory()
        return Result.success()
    }
}

object HistoryWork {
    private const val NAME = "tv-history"

    fun schedule(context: Context) {
        val request = PeriodicWorkRequestBuilder<HistoryWorker>(15, TimeUnit.MINUTES).build()
        WorkManager.getInstance(context).enqueueUniquePeriodicWork(NAME, ExistingPeriodicWorkPolicy.KEEP, request)
    }

    fun cancel(context: Context) {
        WorkManager.getInstance(context).cancelUniqueWork(NAME)
    }
}

class VitalsTvApp : Application() {
    lateinit var graph: TvGraph
        private set

    override fun onCreate() {
        super.onCreate()
        graph = TvGraph(this)
        graph.start()
    }
}

val Context.tvGraph: TvGraph get() = (applicationContext as VitalsTvApp).graph
