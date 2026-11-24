package app.vitals.phone

import app.vitals.core.WireJson
import app.vitals.core.model.AgentClient
import app.vitals.core.model.AgentsSnapshot
import app.vitals.core.model.SessionState
import app.vitals.core.model.activeSessions
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * How the phone's AI card reads `GET /api/v1/agents` from a desktop that may
 * be newer than the app. A value this build has never heard of must hide a
 * line of the card, never fail the whole answer and never be counted as work
 * that is happening.
 */
class AgentsWireTest {
    @Test
    fun the_gateway_client_decodes_to_its_own_variant_rather_than_to_other() {
        val snapshot = decode("""{"sessions":[{"id":"s1","client":"codwen","state":"working"}]}""")
        assertEquals(AgentClient.Codwen, snapshot.sessions.single().client)
    }

    @Test
    fun a_client_renamed_since_this_build_was_installed_is_unknown_not_a_wrong_product() {
        // The product was called kodwen until 2026-11. A desktop still on that
        // release sends the old value; this build must not invent a client for
        // it, and must not fail the answer either.
        val snapshot = decode("""{"sessions":[{"id":"s1","client":"kodwen","state":"working"}]}""")
        assertEquals(AgentClient.Unknown, snapshot.sessions.single().client)
    }

    @Test
    fun a_client_and_a_state_this_build_has_never_heard_of_decode_to_unknown_instead_of_failing() {
        val snapshot = decode("""{"sessions":[{"id":"s1","client":"someFutureAgent","state":"thinking"}]}""")
        assertEquals(AgentClient.Unknown, snapshot.sessions.single().client)
        assertEquals(SessionState.Unknown, snapshot.sessions.single().state)
    }

    @Test
    fun a_state_this_build_cannot_name_is_not_counted_as_a_session_that_is_running() {
        val snapshot = decode("""{"sessions":[{"id":"s1","client":"codwen","state":"thinking"}]}""")
        assertTrue("an unreadable state was reported as running", snapshot.activeSessions.isEmpty())
    }

    @Test
    fun a_snapshot_from_a_pc_that_does_not_share_is_still_a_snapshot() {
        // 404 means "sharing is off", which the card reads as a Hide rather
        // than an error; the decode of the empty body must not throw first.
        val snapshot = decode("{}")
        assertTrue(snapshot.sessions.isEmpty())
        assertTrue(snapshot.resources.isEmpty())
        assertTrue(!snapshot.agentqInstalled)
    }

    private fun decode(json: String): AgentsSnapshot =
        WireJson.Lenient.decodeFromString(AgentsSnapshot.serializer(), json)
}
