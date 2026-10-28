package app.vitals.core.health

import app.vitals.core.WireJson
import app.vitals.core.net.VitalsClient
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.serialization.KSerializer
import kotlinx.serialization.Serializable
import okhttp3.Call
import okhttp3.Callback
import okhttp3.HttpUrl
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.Response
import java.io.IOException
import java.util.concurrent.TimeUnit
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException

/** What a call to vmui can end in, besides success (docs/api/vmui-health.md). */
sealed interface HealthFailure {
    /** `401`: the token is unknown or was revoked in vmui. The user pairs again; the base URL stays. */
    data object SignedOut : HealthFailure

    /** `403`: this vmui account is not the owner's. Retrying cannot help. */
    data object NotOwner : HealthFailure

    /** Nothing answered, or vmui answered with something other than the contract (5xx, 404, 429). */
    data class Unreachable(val cause: String) : HealthFailure

    /** A base URL this app refuses, or a body this build cannot read. */
    data object Invalid : HealthFailure
}

sealed interface HealthResult<out T> {
    data class Ok<T>(val value: T) : HealthResult<T>
    data class Err(val failure: HealthFailure) : HealthResult<Nothing>
}

/** `POST /api/devices/pair` answered: show [code] until the owner approves it in vmui. */
data class PairPending(val id: String, val token: String, val code: String?)

/** One answer to `GET /api/devices/pair?id=`. */
enum class PairState { Pending, Approved, Rejected, Expired, SlowDown }

/**
 * Which base URLs this app will send a health token to. Health data crosses
 * the internet, unlike the LAN API, so only `https://` is accepted — except
 * plain `http://` to this phone itself, for someone developing vmui. The
 * cleartext permission in `network_security_config.xml` exists for the LAN
 * and is deliberately not what guards this.
 */
fun interface SchemePolicy {
    fun allows(url: HttpUrl): Boolean

    companion object {
        val Secure = SchemePolicy { url ->
            url.isHttps || (url.scheme == "http" && (url.host == "localhost" || url.host == "127.0.0.1"))
        }
    }
}

object HealthUrl {
    /**
     * The base URL as it will be stored, or null when it is refused: not a
     * URL, a scheme [policy] rejects, or a user name or password in it (a
     * credential in a URL ends up in logs). A trailing slash is dropped so
     * paths join cleanly; a path prefix (vmui behind a reverse proxy) is kept.
     */
    fun normalise(raw: String, policy: SchemePolicy = SchemePolicy.Secure): String? {
        val url = raw.trim().toHttpUrlOrNull() ?: return null
        if (!policy.allows(url)) return null
        if (url.username.isNotEmpty() || url.password.isNotEmpty()) return null
        if (url.query != null || url.fragment != null) return null
        return url.toString().trimEnd('/')
    }
}

@Serializable
private data class PairRequest(
    val name: String,
    val platform: String,
    val email: String? = null,
    val password: String? = null,
)

@Serializable
private data class PairReply(
    val id: String? = null,
    val token: String? = null,
    val code: String? = null,
    val status: String? = null,
)

/**
 * vmui's device pairing and `GET /api/v1/me/health`.
 *
 * The token goes only in the `Authorization` header — never in a URL, a log
 * or a crash report. The password of the sign-in path is sent once and
 * not kept anywhere, not even in this object.
 */
class HealthClient private constructor(
    private val base: String,
    private val http: OkHttpClient,
) {
    suspend fun pairStart(name: String): HealthResult<PairPending> =
        pair(PairRequest(name.take(64), PLATFORM)).let { r ->
            when (r) {
                is HealthResult.Err -> r
                is HealthResult.Ok -> {
                    val id = r.value.id
                    val token = r.value.token
                    if (id.isNullOrBlank() || token.isNullOrBlank()) {
                        HealthResult.Err(HealthFailure.Invalid)
                    } else {
                        HealthResult.Ok(PairPending(id, token, r.value.code))
                    }
                }
            }
        }

    suspend fun pairPoll(id: String): HealthResult<PairState> {
        val url = "$base/api/devices/pair".toHttpUrlOrNull()?.newBuilder()?.addQueryParameter("id", id)?.build()
            ?: return HealthResult.Err(HealthFailure.Invalid)
        return execute(Request.Builder().url(url).get().build()) { response ->
            when (response.code) {
                200 -> decode(response, PairReply.serializer()).let { r ->
                    when (r) {
                        is HealthResult.Err -> r
                        is HealthResult.Ok -> HealthResult.Ok(
                            when (r.value.status) {
                                "approved" -> PairState.Approved
                                "rejected" -> PairState.Rejected
                                "expired" -> PairState.Expired
                                else -> PairState.Pending
                            },
                        )
                    }
                }
                404 -> HealthResult.Ok(PairState.Expired)
                429 -> HealthResult.Ok(PairState.SlowDown)
                else -> HealthResult.Err(HealthFailure.Unreachable("HTTP ${response.code}"))
            }
        }
    }

    /** The instant path: approved at once, or [HealthFailure.SignedOut] for wrong credentials. */
    suspend fun signIn(name: String, email: String, password: String): HealthResult<String> =
        pair(PairRequest(name.take(64), PLATFORM, email.trim(), password)).let { r ->
            when (r) {
                is HealthResult.Err -> r
                is HealthResult.Ok -> {
                    val token = r.value.token
                    if (token.isNullOrBlank() || r.value.status != "approved") {
                        HealthResult.Err(HealthFailure.Invalid)
                    } else {
                        HealthResult.Ok(token)
                    }
                }
            }
        }

    suspend fun fetch(token: String): HealthResult<MeHealth> {
        val req = Request.Builder().url("$base/api/v1/me/health")
            .header("Authorization", "Bearer $token")
            .header("Accept", "application/json")
            .get()
            .build()
        return execute(req) { response ->
            when (response.code) {
                200 -> decode(response, MeHealth.serializer())
                401 -> HealthResult.Err(HealthFailure.SignedOut)
                403 -> HealthResult.Err(HealthFailure.NotOwner)
                else -> HealthResult.Err(HealthFailure.Unreachable("HTTP ${response.code}"))
            }
        }
    }

    private suspend fun pair(body: PairRequest): HealthResult<PairReply> {
        val json = WireJson.Lenient.encodeToString(PairRequest.serializer(), body).toRequestBody(JSON)
        val req = Request.Builder().url("$base/api/devices/pair").post(json).build()
        return execute(req) { response ->
            when (response.code) {
                200, 201 -> decode(response, PairReply.serializer())
                401 -> HealthResult.Err(HealthFailure.SignedOut)
                403 -> HealthResult.Err(HealthFailure.NotOwner)
                else -> HealthResult.Err(HealthFailure.Unreachable("HTTP ${response.code}"))
            }
        }
    }

    private fun <T> decode(response: Response, serializer: KSerializer<T>): HealthResult<T> {
        val text = response.body.string()
        return runCatching { HealthResult.Ok(WireJson.Lenient.decodeFromString(serializer, text)) }
            .getOrElse { HealthResult.Err(HealthFailure.Invalid) }
    }

    // Same bridge as VitalsClient.execute: a body that fails half-way is
    // "nothing answered", never an exception on OkHttp's thread.
    private suspend fun <T> execute(request: Request, handle: (Response) -> HealthResult<T>): HealthResult<T> =
        suspendCancellableCoroutine { cont ->
            val call = http.newCall(request)
            cont.invokeOnCancellation { call.cancel() }
            call.enqueue(object : Callback {
                override fun onFailure(call: Call, e: IOException) {
                    if (cont.isActive) cont.resume(HealthResult.Err(HealthFailure.Unreachable(e.javaClass.simpleName)))
                }

                override fun onResponse(call: Call, response: Response) {
                    val result = try {
                        response.use { handle(it) }
                    } catch (e: IOException) {
                        HealthResult.Err(HealthFailure.Unreachable(e.javaClass.simpleName))
                    } catch (e: RuntimeException) {
                        if (cont.isActive) cont.resumeWithException(e)
                        return
                    }
                    if (cont.isActive) cont.resume(result)
                }
            })
        }

    companion object {
        const val PLATFORM = "vitals-android"
        private val JSON = "application/json".toMediaType()

        /**
         * vmui is across the internet, not on the LAN, so the timeouts are
         * longer than the LAN client's and [app.vitals.core.net.LanNetwork]
         * does not gate it. Redirects stay off: following one would replay
         * the token at whatever host the answer names.
         */
        val http: OkHttpClient by lazy {
            VitalsClient.shared.newBuilder()
                .connectTimeout(10, TimeUnit.SECONDS)
                .readTimeout(15, TimeUnit.SECONDS)
                .callTimeout(25, TimeUnit.SECONDS)
                .followRedirects(false)
                .followSslRedirects(false)
                .build()
        }

        /** A client for [rawBase], or null when [HealthUrl.normalise] refuses it. */
        fun forBase(
            rawBase: String,
            policy: SchemePolicy = SchemePolicy.Secure,
            client: OkHttpClient = http,
        ): HealthClient? = HealthUrl.normalise(rawBase, policy)?.let { HealthClient(it, client) }

        /** The part of a token that may be shown: the first eight characters. */
        fun tokenPrefix(token: String): String = token.take(8)
    }
}
