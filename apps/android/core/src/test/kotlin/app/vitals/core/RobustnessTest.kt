package app.vitals.core

import app.vitals.core.model.Alert
import app.vitals.core.model.AlertKind
import app.vitals.core.model.ProcessState
import app.vitals.core.model.Severity
import app.vitals.core.model.Summary
import app.vitals.core.net.ApiFailure
import app.vitals.core.net.ApiResult
import app.vitals.core.net.VitalsClient
import kotlinx.coroutines.test.runTest
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

/** AND-8 (tolerant enums), AND-16 (403 is a revoked pairing), AND-43 (no redirects). */
class RobustnessTest {
    private val server = MockWebServer()

    @Before
    fun start() = server.start()

    @After
    fun stop() = server.shutdown()

    private fun fixture(name: String): String =
        requireNotNull(javaClass.getResource("/contract/$name")).readText()

    @Test
    fun an_enum_value_a_newer_desktop_added_decodes_to_unknown_instead_of_failing_the_frame() {
        val raw = fixture("summary.json").replace("\"notResponding\"", "\"hibernating\"").let {
            // Whatever state the fixture's process is in, force an unknown one.
            Regex("\"state\":\"[a-zA-Z]+\"").replace(it, "\"state\":\"hibernating\"")
        }
        val summary = WireJson.Lenient.decodeFromString(Summary.serializer(), raw)
        assertEquals(ProcessState.Unknown, summary.top.single().state)
    }

    @Test
    fun an_unknown_alert_kind_and_severity_still_yield_an_alert_the_ui_can_word() {
        val raw = """{"kind":"psuOverload","severity":"catastrophic","subject":"PSU","title":"t","cause":"c",""" +
            """"values":{},"route":"cooling","sinceSample":3}"""
        val alert = WireJson.Lenient.decodeFromString(Alert.serializer(), raw)
        assertEquals(AlertKind.Unknown, alert.kind)
        assertEquals(Severity.Unknown, alert.severity)
        assertEquals(app.vitals.core.model.AlertRoute.Unknown, alert.route)
    }

    @Test
    fun known_enum_values_round_trip_with_the_exact_wire_spelling() {
        val json = WireJson.Lenient.encodeToString(ProcessState.serializer(), ProcessState.NotResponding)
        assertEquals("\"notResponding\"", json)
        assertEquals(ProcessState.NotResponding, WireJson.Lenient.decodeFromString(ProcessState.serializer(), json))
    }

    @Test
    fun a_frame_that_still_cannot_be_read_is_counted_and_reported_incompatible() = runTest {
        val before = WireJson.decodeFailures
        server.enqueue(MockResponse().setResponseCode(200).setBody("{\"seq\":\"not a number\"}"))
        val r = VitalsClient("http://127.0.0.1:${server.port}", "t").summary()
        assertTrue("got $r", r is ApiResult.Err && r.failure is ApiFailure.Incompatible)
        assertEquals(before + 1, WireJson.decodeFailures)
    }

    @Test
    fun a_read_refused_with_403_means_pair_again_like_401() = runTest {
        server.enqueue(MockResponse().setResponseCode(403))
        val r = VitalsClient("http://127.0.0.1:${server.port}", "t").summary()
        assertTrue("got $r", r is ApiResult.Err && r.failure == ApiFailure.Unauthorised)
    }

    @Test
    fun a_redirect_is_never_followed_so_the_token_cannot_be_replayed_elsewhere() = runTest {
        val elsewhere = MockWebServer().apply { start() }
        try {
            server.enqueue(MockResponse().setResponseCode(302).setHeader("Location", elsewhere.url("/steal")))
            val r = VitalsClient("http://127.0.0.1:${server.port}", "t").summary()
            assertTrue("got $r", r is ApiResult.Err && (r.failure as? ApiFailure.Http)?.status == 302)
            assertEquals(0, elsewhere.requestCount)
        } finally {
            elsewhere.shutdown()
        }
    }
}
