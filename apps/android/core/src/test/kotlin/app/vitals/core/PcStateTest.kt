package app.vitals.core

import app.vitals.core.model.Summary
import app.vitals.core.model.SensorLine
import app.vitals.core.wear.PcState
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
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

    @Test
    fun a_round_that_did_not_measure_the_sensors_keeps_the_last_list_instead_of_blanking_it() {
        // The phone's relay sends metrics only, so every relayed state would
        // otherwise arrive with no sensors and wipe the watch's screen.
        val measured = state(1_000, reached = true)
            .copy(sensors = listOf(SensorLine("cpu.package", "CPU Package", 81f, "temperature", "kernelDriver", "measured")))
        val relayed = state(2_000, reached = true).copy(sensors = null)
        assertEquals(measured.sensors, relayed.after(measured).sensors)
    }

    @Test
    fun a_measured_empty_sensor_list_is_kept_as_the_fact_it_is() {
        // A machine that really reports no sensors is not the same thing as
        // one nobody has asked yet, and the empty list must survive a round
        // that did not ask either.
        val reported = state(1_000, reached = true).copy(sensors = emptyList())
        val notAsked = state(2_000, reached = true).copy(sensors = null)
        assertEquals(emptyList<SensorLine>(), notAsked.after(reported).sensors)
    }

    @Test
    fun a_refused_token_stays_refused_until_the_pc_actually_answers_again() {
        val refused = state(1_000, reached = false).copy(unauthorised = true)
        val stillFailing = state(2_000, reached = false)
        assertTrue("a sleep or a dropped Wi-Fi is not a reason to forget a refusal", stillFailing.after(refused).unauthorised)
        val answered = state(3_000, reached = true)
        assertTrue("answering clears it", !answered.after(stillFailing.after(refused)).unauthorised)
    }
}
