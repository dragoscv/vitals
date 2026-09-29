package app.vitals.wear.data

import android.content.Context
import app.vitals.core.WireJson
import app.vitals.core.model.Alert
import app.vitals.core.model.ControlError
import app.vitals.core.model.ControlRequest
import app.vitals.core.model.ProcessKey
import app.vitals.core.net.ApiFailure
import app.vitals.core.net.ApiResult
import app.vitals.core.net.LanAddress
import app.vitals.core.net.VitalsClient
import app.vitals.core.net.getOrNull
import app.vitals.core.pairing.Pairing
import app.vitals.core.pairing.PairingStore
import app.vitals.core.pairing.Scope
import app.vitals.core.wear.PcState
import app.vitals.core.wear.WatchControl
import app.vitals.core.wear.WatchControlReply
import app.vitals.core.wear.WatchPairing
import app.vitals.core.wear.WearContract
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonPrimitive
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap

/** How an end-task request ended, as the watch can explain it. */
sealed interface ControlOutcome {
    data object Done : ControlOutcome
    data class Failed(val reason: Reason) : ControlOutcome

    enum class Reason { ReadOnly, AccessDenied, AlreadyGone, Unreachable, Unknown }
}

/**
 * Everything the watch knows about its PCs, and the only thing that fetches.
 *
 * Two sources, in order of preference: the phone over the Data Layer (the
 * Bluetooth link is already up, so it costs the watch almost nothing), then
 * the PC itself over Wi-Fi. Direct polling only happens when something asks —
 * the visible app every five seconds, or a tile or complication being drawn —
 * so a watch in a drawer does no network work at all.
 */
class WearRepository(private val context: Context, private val scope: CoroutineScope) {
    private val store = PairingStore(context)
    private val cache = StateCache(context)
    private val phone = PhoneLink(context)
    private val surfaces = SurfaceUpdater(context, scope)

    private val _states = MutableStateFlow<Map<String, PcState>>(emptyMap())
    val states: StateFlow<Map<String, PcState>> = _states.asStateFlow()

    val pairings: StateFlow<List<Pairing>> = store.pairings

    private val _loaded = MutableStateFlow(false)
    val loaded: StateFlow<Boolean> = _loaded.asStateFlow()

    private val loadLock = Mutex()
    private val refreshLock = Mutex()
    private val writeLock = Mutex()
    private val replies = ConcurrentHashMap<String, CompletableDeferred<WatchControlReply>>()

    init {
        scope.launch { load() }
    }

    /** Reads the disk and the Data Layer once per process. */
    suspend fun load() = loadLock.withLock {
        if (_loaded.value) return@withLock
        store.load()
        _states.value = withContext(Dispatchers.IO) { cache.read() }
        // Items the phone put while this process was not running: the
        // listener service only hears about changes, not the current state.
        for ((path, json) in phone.readAll()) applyDataItem(path, json, notifySurfaces = false)
        _loaded.value = true
    }

    fun stateFor(id: String): PcState? = _states.value[id]

    fun pairingFor(id: String): Pairing? = store.pairings.value.firstOrNull { it.id == id }

    // ---- Inbound from the phone -------------------------------------------------

    suspend fun applyDataItem(path: String, json: String, notifySurfaces: Boolean = true) {
        when {
            path == WearContract.PATH_PAIRINGS -> applyPairings(json)
            path.startsWith(WearContract.PATH_STATE_PREFIX) -> {
                val state = runCatching { WireJson.Lenient.decodeFromString(PcState.serializer(), json) }
                    .getOrNull() ?: return
                putState(state, notifySurfaces)
            }
        }
    }

    /** A DataItem the phone deleted: a PC unpaired there. */
    suspend fun removeDataItem(path: String) {
        if (!path.startsWith(WearContract.PATH_STATE_PREFIX)) return
        val id = path.removePrefix(WearContract.PATH_STATE_PREFIX)
        writeLock.withLock {
            _states.update { it - id }
            withContext(Dispatchers.IO) { cache.write(_states.value) }
        }
        surfaces.request()
    }

    fun onAlertMessage(payload: ByteArray) {
        val text = payload.decodeToString()
        val (alert, label) = decodeAlert(text) ?: return
        Alerter.notify(context, alert, label)
    }

    fun onControlReply(payload: ByteArray) {
        val reply = runCatching {
            WireJson.Lenient.decodeFromString(WatchControlReply.serializer(), payload.decodeToString())
        }.getOrNull() ?: return
        replies.remove(reply.requestId)?.complete(reply)
    }

    private suspend fun applyPairings(json: String) {
        val incoming = runCatching {
            WireJson.Lenient.decodeFromString(ListSerializer(WatchPairing.serializer()), json)
        }.getOrNull() ?: return
        val now = System.currentTimeMillis()
        store.update { current ->
            incoming.map { p ->
                Pairing(
                    id = p.id,
                    label = p.label,
                    baseUrl = p.baseUrl,
                    token = p.token,
                    // The phone knows whether the token can control; the watch
                    // must never offer an action the PC will refuse.
                    scope = if (p.canControl) Scope.Control else Scope.Read,
                    createdMs = current.firstOrNull { it.id == p.id }?.createdMs ?: now,
                )
            }
        }
        val kept = incoming.map { it.id }.toSet()
        writeLock.withLock {
            _states.update { states -> states.filterKeys { it in kept } }
            withContext(Dispatchers.IO) { cache.write(_states.value) }
        }
        surfaces.request()
    }

    private suspend fun putState(state: PcState, notifySurfaces: Boolean) {
        writeLock.withLock {
            val previous = _states.value[state.pairingId]
            // DataItems can arrive out of order after a reconnect; an older
            // reading must not replace a newer one the watch fetched itself.
            if (previous != null && previous.fetchedMs > state.fetchedMs) return
            _states.update { it + (state.pairingId to state.after(previous)) }
            withContext(Dispatchers.IO) { cache.write(_states.value) }
        }
        if (notifySurfaces) surfaces.request()
    }

    private fun decodeAlert(text: String): Pair<Alert, String?>? {
        val json = WireJson.Lenient
        runCatching { json.decodeFromString(Alert.serializer(), text) }.getOrNull()?.let { return it to null }
        // A wrapped alert that names its PC: `{ "pairingId": ..., "alert": {...} }`.
        val obj = runCatching { json.parseToJsonElement(text) as? JsonObject }.getOrNull() ?: return null
        val inner = obj["alert"] ?: return null
        val alert = runCatching { json.decodeFromJsonElement(Alert.serializer(), inner) }.getOrNull() ?: return null
        val label = obj["label"]?.jsonPrimitive?.contentOrNull
            ?: obj["pairingId"]?.jsonPrimitive?.contentOrNull?.let { id -> pairingFor(id)?.label }
        return alert to label
    }

    // ---- Refreshing -------------------------------------------------------------

    /** For a tile or complication: refresh after answering, for the next request. */
    fun refreshInBackground() {
        scope.launch { refresh() }
    }

    /**
     * Brings every PC up to date. At most one refresh runs at a time; a
     * second caller simply shares the result of the first.
     */
    suspend fun refresh() {
        if (!refreshLock.tryLock()) return
        try {
            load()
            val pcs = store.pairings.value
            if (pcs.isEmpty()) return
            val started = System.currentTimeMillis()
            val node = phone.nearestPhone()
            if (node != null && phone.send(node, WearContract.PATH_REFRESH, ByteArray(0))) {
                awaitStatesNewerThan(started - FRESH_MS, pcs.map { it.id }.toSet())
            }
            val now = System.currentTimeMillis()
            val stale = pcs.filter { p ->
                val fetched = _states.value[p.id]?.fetchedMs ?: 0L
                // With a phone in reach its relay is the source; only a PC it has
                // not reported for a minute is worth the watch's own Wi-Fi radio.
                if (node != null) now - fetched > FRESH_MS else now - fetched > DIRECT_MIN_AGE_MS
            }
            coroutineScope { stale.map { async { pollDirect(it) } }.awaitAll() }
        } finally {
            refreshLock.unlock()
        }
    }

    private suspend fun awaitStatesNewerThan(sinceMs: Long, ids: Set<String>) {
        withTimeoutOrNull(PHONE_WAIT_MS) {
            states.map { s -> ids.all { id -> (s[id]?.fetchedMs ?: 0L) >= sinceMs } }.first { it }
        }
    }

    private suspend fun pollDirect(p: Pairing) {
        if (!LanAddress.isPrivate(p.baseUrl)) return
        val client = VitalsClient(p.baseUrl, p.token)
        val (summary, sensors, alerts) = coroutineScope {
            val s = async { client.summary(top = 5) }
            val n = async { client.sensors() }
            val a = async { client.alerts() }
            Triple(s.await(), n.await(), a.await())
        }
        val fresh = summary.getOrNull() ?: return
        val previous = _states.value[p.id]
        val newAlerts = alerts.getOrNull().orEmpty()
        val state = PcState(
            pairingId = p.id,
            label = p.label,
            fetchedMs = System.currentTimeMillis(),
            summary = fresh,
            // A sensors or alerts failure keeps the last list rather than
            // pretending the PC suddenly has no sensors and nothing wrong.
            sensors = sensors.getOrNull() ?: previous?.sensors.orEmpty(),
            alerts = alerts.getOrNull() ?: previous?.alerts.orEmpty(),
            canControl = p.scope != Scope.Read,
        )
        putState(state, notifySurfaces = true)
        // The phone buzzes the watch for alerts it sees; when the watch is
        // fetching for itself, nobody else will.
        val seen = previous?.alerts.orEmpty().map { it.kind to it.subject }.toSet()
        newAlerts.filter { (it.kind to it.subject) !in seen }.forEach { Alerter.notify(context, it, p.label) }
    }

    // ---- Control ----------------------------------------------------------------

    suspend fun endTask(pairingId: String, key: ProcessKey): ControlOutcome {
        val pairing = pairingFor(pairingId) ?: return ControlOutcome.Failed(ControlOutcome.Reason.Unknown)
        if (pairing.scope == Scope.Read) return ControlOutcome.Failed(ControlOutcome.Reason.ReadOnly)
        val outcome = viaPhone(pairingId, key) ?: direct(pairing, key)
        if (outcome == ControlOutcome.Done) scope.launch { refresh() }
        return outcome
    }

    /** `null` when no phone could take the request, so the caller goes direct. */
    private suspend fun viaPhone(pairingId: String, key: ProcessKey): ControlOutcome? {
        val node = phone.nearestPhone() ?: return null
        val request = WatchControl(UUID.randomUUID().toString(), pairingId, key.pid, key.startTime)
        val waiter = CompletableDeferred<WatchControlReply>()
        replies[request.requestId] = waiter
        try {
            val body = WireJson.Lenient.encodeToString(WatchControl.serializer(), request).encodeToByteArray()
            if (!phone.send(node, WearContract.PATH_CONTROL, body)) return null
            val reply = withTimeoutOrNull(CONTROL_WAIT_MS) { waiter.await() }
                ?: return ControlOutcome.Failed(ControlOutcome.Reason.Unreachable)
            return if (reply.ok) ControlOutcome.Done else ControlOutcome.Failed(reasonOf(reply.error))
        } finally {
            replies.remove(request.requestId)
        }
    }

    private suspend fun direct(pairing: Pairing, key: ProcessKey): ControlOutcome {
        if (!LanAddress.isPrivate(pairing.baseUrl)) return ControlOutcome.Failed(ControlOutcome.Reason.Unreachable)
        return when (val r = VitalsClient(pairing.baseUrl, pairing.token).control(ControlRequest.Terminate(key))) {
            is ApiResult.Ok -> ControlOutcome.Done
            is ApiResult.Err -> ControlOutcome.Failed(
                when (val f = r.failure) {
                    is ApiFailure.Refused -> when (f.error) {
                        ControlError.Forbidden -> ControlOutcome.Reason.ReadOnly
                        ControlError.AccessDenied -> ControlOutcome.Reason.AccessDenied
                        ControlError.NotFound -> ControlOutcome.Reason.AlreadyGone
                        else -> ControlOutcome.Reason.Unknown
                    }
                    ApiFailure.Unauthorised -> ControlOutcome.Reason.ReadOnly
                    is ApiFailure.Unreachable -> ControlOutcome.Reason.Unreachable
                    else -> ControlOutcome.Reason.Unknown
                },
            )
        }
    }

    private fun reasonOf(error: String?): ControlOutcome.Reason = when (error) {
        "forbidden" -> ControlOutcome.Reason.ReadOnly
        "access-denied" -> ControlOutcome.Reason.AccessDenied
        "not-found" -> ControlOutcome.Reason.AlreadyGone
        "unreachable" -> ControlOutcome.Reason.Unreachable
        else -> ControlOutcome.Reason.Unknown
    }

    companion object {
        /** Older than this, a relayed state is not trusted to be current. */
        const val FRESH_MS = 60_000L

        /**
         * Without a phone, the app's five-second loop polls on every tick; a
         * tile asking twice in one second does not need two requests.
         */
        private const val DIRECT_MIN_AGE_MS = 4_000L
        private const val PHONE_WAIT_MS = 3_000L
        private const val CONTROL_WAIT_MS = 8_000L
    }
}
