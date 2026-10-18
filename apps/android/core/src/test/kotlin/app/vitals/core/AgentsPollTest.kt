package app.vitals.core

import app.vitals.core.model.AgentClient
import app.vitals.core.model.AgentSession
import app.vitals.core.model.AgentqResource
import app.vitals.core.model.AgentqTicket
import app.vitals.core.model.AgentsPoll
import app.vitals.core.model.AgentsSnapshot
import app.vitals.core.model.SessionState
import app.vitals.core.net.ApiFailure
import app.vitals.core.net.ApiResult
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** The AI card's polling policy: what it shows and how hard it leans on the PC. */
class AgentsPollTest {
    private fun session(state: SessionState) = AgentSession(id = "s", client = AgentClient.Codex, state = state)

    @Test
    fun a_pc_that_does_not_share_hides_the_card_and_is_asked_only_every_five_minutes() {
        // Sharing is off by default on the desktop: 404 is the common answer.
        val p = AgentsPoll.after(ApiResult.Err(ApiFailure.Http(404)))
        assertTrue(p is AgentsPoll.Hide)
        assertEquals(AgentsPoll.NOT_SHARED_MS, p.delayMs)
    }

    @Test
    fun a_read_refused_with_403_hides_the_card_too_rather_than_hammering() {
        assertEquals(AgentsPoll.Hide(AgentsPoll.NOT_SHARED_MS), AgentsPoll.after(ApiResult.Err(ApiFailure.Unauthorised)))
        assertEquals(AgentsPoll.Hide(AgentsPoll.NOT_SHARED_MS), AgentsPoll.after(ApiResult.Err(ApiFailure.Http(403))))
    }

    @Test
    fun nothing_installed_and_no_session_hides_the_card_and_backs_off_to_a_minute() {
        val p = AgentsPoll.after(ApiResult.Ok(AgentsSnapshot(agentqInstalled = false)))
        assertEquals(AgentsPoll.Hide(AgentsPoll.QUIET_MS), p)
    }

    @Test
    fun an_empty_but_installed_answer_is_shown_and_polled_once_a_minute() {
        val s = AgentsSnapshot(agentqInstalled = true, sessions = listOf(session(SessionState.Ended)))
        assertEquals(AgentsPoll.Show(s, AgentsPoll.QUIET_MS), AgentsPoll.after(ApiResult.Ok(s)))
    }

    @Test
    fun something_working_or_queued_is_polled_every_ten_seconds() {
        val working = AgentsSnapshot(sessions = listOf(session(SessionState.Working)))
        assertEquals(AgentsPoll.BUSY_MS, AgentsPoll.after(ApiResult.Ok(working)).delayMs)
        val queued = AgentsSnapshot(
            agentqInstalled = true,
            resources = listOf(AgentqResource("build:x", listOf(AgentqTicket(held = false)))),
        )
        assertEquals(AgentsPoll.BUSY_MS, AgentsPoll.after(ApiResult.Ok(queued)).delayMs)
    }

    @Test
    fun a_blip_keeps_the_last_answer_on_screen() {
        assertTrue(AgentsPoll.after(ApiResult.Err(ApiFailure.Unreachable("x"))) is AgentsPoll.Keep)
        assertTrue(AgentsPoll.after(ApiResult.Err(ApiFailure.OffLan)) is AgentsPoll.Keep)
        assertTrue(AgentsPoll.after(ApiResult.Err(ApiFailure.Http(500))) is AgentsPoll.Keep)
    }
}
