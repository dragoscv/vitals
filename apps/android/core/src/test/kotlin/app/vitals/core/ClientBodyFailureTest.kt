package app.vitals.core

import app.vitals.core.net.ApiFailure
import app.vitals.core.net.ApiResult
import app.vitals.core.net.VitalsClient
import kotlinx.coroutines.test.runTest
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import okhttp3.mockwebserver.SocketPolicy
import org.junit.After
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

/**
 * A PC that goes to sleep half-way through a response. The body is read in
 * the response callback; the IOException used to be handed to the
 * coroutine as an exception, and no scope in the apps expects one, so it
 * took the whole process down.
 */
class ClientBodyFailureTest {
    private val server = MockWebServer()

    @Before
    fun start() = server.start()

    @After
    fun stop() = server.shutdown()

    @Test
    fun a_connection_dropped_mid_body_is_unreachable_not_an_exception() = runTest {
        server.enqueue(
            MockResponse()
                .setResponseCode(200)
                .setBody("{\"ok\":true,\"version\":\"" + "x".repeat(64_000) + "\",\"modelVersion\":1}")
                .setSocketPolicy(SocketPolicy.DISCONNECT_DURING_RESPONSE_BODY),
        )
        val r = VitalsClient("http://127.0.0.1:${server.port}", "t").summary()
        assertTrue("got $r", r is ApiResult.Err && r.failure is ApiFailure.Unreachable)
    }
}
