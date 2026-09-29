package app.vitals.core

import app.vitals.core.net.ApiFailure
import app.vitals.core.net.ApiResult
import app.vitals.core.net.VitalsClient
import app.vitals.core.pairing.PairCheck
import app.vitals.core.pairing.PairCheckResult
import app.vitals.core.pairing.PairRefusal
import app.vitals.core.pairing.Scope
import kotlinx.coroutines.test.runTest
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

class PairCodeTest {
    private val server = MockWebServer()

    @Before
    fun start() = server.start()

    @After
    fun stop() = server.shutdown()

    private fun base() = "http://127.0.0.1:${server.port}"

    @Test
    fun a_code_is_digits_only_so_the_desktops_three_plus_three_grouping_still_pairs() {
        assertEquals("482913", PairCheck.cleanCode("482 913"))
        assertTrue(PairCheck.isCode("482 913"))
        assertFalse(PairCheck.isCode("48291"))
        assertFalse(PairCheck.isCode("4829134"))
    }

    @Test
    fun redeem_posts_the_code_without_a_bearer_and_learns_the_scope_the_pc_chose() = runTest {
        server.enqueue(MockResponse().setResponseCode(200).setBody("""{"token":"${"a".repeat(43)}","scope":"control"}"""))
        val r = VitalsClient.redeem(base(), "482913", "Living room TV")
        val grant = (r as ApiResult.Ok).value
        assertEquals(Scope.Control, grant.scope)
        val req = server.takeRequest()
        assertEquals("/api/v1/pair", req.path)
        assertEquals(null, req.getHeader("Authorization"))
        val body = req.body.readUtf8()
        assertTrue(body.contains("\"code\":\"482913\""))
        assertTrue(body.contains("\"label\":\"Living room TV\""))
    }

    @Test
    fun a_refused_code_reads_as_a_bad_code_and_never_reaches_the_token_check() = runTest {
        server.enqueue(MockResponse().setResponseCode(403).setBody("""{"error":"invalid pairing code"}"""))
        val r = PairCheck.withCode(base(), "000000", "TV", null, emptyList())
        assertEquals(PairRefusal.BadCode, (r as PairCheckResult.Refused).reason)
        // Only the /pair request: a refused code must not be followed by /health or /summary.
        assertEquals(1, server.requestCount)
    }

    @Test
    fun a_malformed_code_is_refused_locally_without_spending_one_of_the_pcs_five_attempts() = runTest {
        val r = PairCheck.withCode(base(), "12a45", "TV", null, emptyList())
        assertEquals(PairRefusal.BadCode, (r as PairCheckResult.Refused).reason)
        assertEquals(0, server.requestCount)
    }

    @Test
    fun an_older_pc_without_the_route_is_a_failure_with_its_status_not_a_bad_code() = runTest {
        server.enqueue(MockResponse().setResponseCode(404))
        val r = VitalsClient.redeem(base(), "482913", "TV")
        assertEquals(ApiFailure.Http(404), (r as ApiResult.Err).failure)
        server.enqueue(MockResponse().setResponseCode(404))
        val c = PairCheck.withCode(base(), "482913", "TV", null, emptyList()) as PairCheckResult.Refused
        assertEquals(PairRefusal.Failed, c.reason)
        assertEquals(404, c.status)
    }
}
