package app.vitals.core.net

import app.vitals.core.WireJson
import app.vitals.core.model.Alert
import app.vitals.core.model.ControlError
import app.vitals.core.model.ControlRequest
import app.vitals.core.model.Frame
import app.vitals.core.model.Health
import app.vitals.core.model.HostInfo
import app.vitals.core.model.MachineSample
import app.vitals.core.model.SUPPORTED_MODEL_VERSION
import app.vitals.core.model.SensorLine
import app.vitals.core.model.Summary
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.serialization.KSerializer
import kotlinx.serialization.builtins.ListSerializer
import okhttp3.Call
import okhttp3.Callback
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import java.io.IOException
import java.util.concurrent.TimeUnit
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException

/** What a call to a PC can end in, besides success. */
sealed interface ApiFailure {
    /** Nothing answered: PC off, wrong network, firewall. */
    data class Unreachable(val cause: String) : ApiFailure
    /** `401`: the pairing was revoked on the PC. */
    data object Unauthorised : ApiFailure
    /** The PC speaks a frame shape this app does not know. */
    data class Incompatible(val modelVersion: Int) : ApiFailure
    /** `204`: the PC is up but has not sampled yet. */
    data object NotReady : ApiFailure
    data class Refused(val error: ControlError) : ApiFailure
    data class Http(val status: Int) : ApiFailure
}

sealed interface ApiResult<out T> {
    data class Ok<T>(val value: T) : ApiResult<T>
    data class Err(val failure: ApiFailure) : ApiResult<Nothing>
}

inline fun <T, R> ApiResult<T>.map(f: (T) -> R): ApiResult<R> = when (this) {
    is ApiResult.Ok -> ApiResult.Ok(f(value))
    is ApiResult.Err -> this
}

fun <T> ApiResult<T>.getOrNull(): T? = (this as? ApiResult.Ok)?.value

/**
 * The LAN API of one PC (`docs/api/openapi.yaml`).
 *
 * One shared [OkHttpClient] per process: it owns the connection pool and the
 * dispatcher threads, and a client per PC would multiply both. The token is
 * always sent in the `Authorization` header — the `?token=` fallback exists
 * for browsers only and would put the secret in a request line.
 */
class VitalsClient(
    private val baseUrl: String,
    private val token: String,
    private val http: OkHttpClient = shared,
) {
    suspend fun health(): ApiResult<Health> =
        get("/api/v1/health", Health.serializer(), authorised = false).let { r ->
            if (r is ApiResult.Ok && r.value.modelVersion != SUPPORTED_MODEL_VERSION) {
                ApiResult.Err(ApiFailure.Incompatible(r.value.modelVersion))
            } else {
                r
            }
        }

    suspend fun summary(top: Int = 5): ApiResult<Summary> =
        get("/api/v1/summary?top=$top", Summary.serializer())

    suspend fun snapshot(): ApiResult<Frame> = get("/api/v1/snapshot", Frame.serializer())

    suspend fun host(): ApiResult<HostInfo> = get("/api/v1/host", HostInfo.serializer())

    suspend fun alerts(): ApiResult<List<Alert>> =
        get("/api/v1/alerts", ListSerializer(Alert.serializer()))

    suspend fun sensors(): ApiResult<List<SensorLine>> =
        get("/api/v1/sensors", ListSerializer(SensorLine.serializer()))

    suspend fun history(seconds: Int): ApiResult<List<MachineSample>> =
        get("/api/v1/history?seconds=$seconds", ListSerializer(MachineSample.serializer()))

    /** `POST /api/v1/control`. `204` is success; anything else carries a [ControlError]. */
    suspend fun control(request: ControlRequest): ApiResult<Unit> {
        val body = WireJson.Lenient.encodeToString(ControlRequest.serializer(), request)
            .toRequestBody(JSON)
        val req = Request.Builder().url(baseUrl + "/api/v1/control")
            .header("Authorization", "Bearer $token")
            .post(body)
            .build()
        return execute(req) { response ->
            when (response.code) {
                204 -> ApiResult.Ok(Unit)
                401 -> ApiResult.Err(ApiFailure.Unauthorised)
                else -> {
                    val text = response.body.string()
                    val error = runCatching {
                        WireJson.Lenient.decodeFromString(ControlError.serializer(), text)
                    }.getOrNull()
                    ApiResult.Err(error?.let(ApiFailure::Refused) ?: ApiFailure.Http(response.code))
                }
            }
        }
    }

    /**
     * Live frames over the WebSocket. The first is always a complete, current
     * keyframe. The flow ends with [StreamClosed] so a caller can back off
     * and reconnect; cancelling the collector closes the socket.
     */
    fun frames(): Flow<Frame> = callbackFlow {
        val wsUrl = baseUrl.replaceFirst("http", "ws") + "/api/v1/ws"
        val req = Request.Builder().url(wsUrl).header("Authorization", "Bearer $token").build()
        val socket = streaming.newWebSocket(req, object : WebSocketListener() {
            override fun onMessage(webSocket: WebSocket, text: String) {
                // Control replies share the socket; they have no `payload`.
                if (!text.contains("\"payload\"")) return
                runCatching { WireJson.Lenient.decodeFromString(Frame.serializer(), text) }
                    .onSuccess { trySend(it) }
            }

            override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                close(StreamClosed(code, null))
            }

            override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                close(StreamClosed(response?.code ?: 0, t.message))
            }
        })
        awaitClose { socket.close(1000, null) }
    }.flowOn(Dispatchers.IO)

    private suspend fun <T> get(
        path: String,
        serializer: KSerializer<T>,
        authorised: Boolean = true,
    ): ApiResult<T> {
        val builder = Request.Builder().url(baseUrl + path).get()
        if (authorised) builder.header("Authorization", "Bearer $token")
        return execute(builder.build()) { response ->
            when (response.code) {
                200 -> {
                    val text = response.body.string()
                    runCatching { ApiResult.Ok(WireJson.Lenient.decodeFromString(serializer, text)) }
                        .getOrElse { ApiResult.Err(ApiFailure.Incompatible(-1)) }
                }
                204 -> ApiResult.Err(ApiFailure.NotReady)
                401 -> ApiResult.Err(ApiFailure.Unauthorised)
                else -> ApiResult.Err(ApiFailure.Http(response.code))
            }
        }
    }

    private suspend fun <T> execute(request: Request, handle: (Response) -> ApiResult<T>): ApiResult<T> =
        suspendCancellableCoroutine { cont ->
            val call = http.newCall(request)
            cont.invokeOnCancellation { call.cancel() }
            call.enqueue(object : Callback {
                override fun onFailure(call: Call, e: IOException) {
                    if (cont.isActive) cont.resume(ApiResult.Err(ApiFailure.Unreachable(e.javaClass.simpleName)))
                }

                override fun onResponse(call: Call, response: Response) {
                    response.use {
                        val result = runCatching { handle(it) }
                        if (!cont.isActive) return
                        result.fold(cont::resume) { e -> cont.resumeWithException(e) }
                    }
                }
            })
        }

    companion object {
        private val JSON = "application/json".toMediaType()

        /**
         * Short timeouts: on a LAN a PC either answers in milliseconds or is
         * not there, and a widget waiting 30 s for a sleeping PC holds a
         * WorkManager slot for nothing.
         */
        val shared: OkHttpClient by lazy {
            OkHttpClient.Builder()
                .connectTimeout(3, TimeUnit.SECONDS)
                .readTimeout(10, TimeUnit.SECONDS)
                .callTimeout(15, TimeUnit.SECONDS)
                .retryOnConnectionFailure(false)
                .build()
        }

        /** The stream: no read timeout (frames can be 20 s apart), a ping to spot a dead link. */
        private val streaming: OkHttpClient by lazy {
            shared.newBuilder()
                .readTimeout(0, TimeUnit.SECONDS)
                .callTimeout(0, TimeUnit.SECONDS)
                .pingInterval(20, TimeUnit.SECONDS)
                .build()
        }
    }
}

class StreamClosed(val code: Int, message: String?) : IOException(message ?: "stream closed ($code)")
