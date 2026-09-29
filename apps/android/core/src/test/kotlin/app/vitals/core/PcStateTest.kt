package app.vitals.core

import app.vitals.core.model.Summary
import app.vitals.core.wear.PcState
import org.junit.Assert.assertEquals
import org.junit.Test

class PcStateTest {
    private val summary: Summary = WireJson.Lenient.decodeFromString(
        Summary.serializer(),
        javaClass.getResource("/contract/summary.json")!!.readText(),
    )

    private fun state(fetchedMs: Long, reached: Boolean) =
        PcState("pc", "Desktop", fetchedMs, if (reached) summary else null, emptyList(), emptyList(), canControl = false)

    @Test
    fun an_unreachable_state_keeps_the_time_the_pc_last_answered_not_the_time_the_phone_failed() {
        val seen = state(1_000, reached = true)
        val failed = state(9_000, reached = false).after(seen)
        assertEquals(1_000, failed.lastSeenMs)
    }

    @Test
    fun the_last_seen_time_survives_several_failures_in_a_row() {
        val seen = state(1_000, reached = true)
        val first = state(5_000, reached = false).after(seen)
        val second = state(9_000, reached = false).after(first)
        assertEquals(1_000, second.lastSeenMs)
    }

    @Test
    fun a_pc_that_never_answered_has_no_last_seen_time() {
        assertEquals(0, state(9_000, reached = false).after(null).lastSeenMs)
    }

    @Test
    fun a_reached_state_is_seen_when_it_was_fetched() {
        val failed = state(5_000, reached = false).after(state(1_000, reached = true))
        assertEquals(9_000, state(9_000, reached = true).after(failed).lastSeenMs)
    }
}
