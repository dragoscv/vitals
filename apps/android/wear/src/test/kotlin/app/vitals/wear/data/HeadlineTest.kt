package app.vitals.wear.data

import app.vitals.core.WireJson
import app.vitals.core.model.Summary
import app.vitals.core.wear.PcState
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/** AND-20: a reading older than ten minutes is shown as "no data", never as current. */
class HeadlineTest {
    private val summary: Summary = WireJson.Lenient.decodeFromString(
        Summary.serializer(),
        File("../core/src/test/resources/contract/summary.json").readText(),
    )

    private fun state(fetchedMs: Long) = PcState("pc", "Desk", fetchedMs, summary, emptyList(), emptyList(), canControl = false)

    @Test
    fun a_fresh_reading_is_reachable_with_its_numbers() {
        val now = 1_000_000_000L
        val h = Headline.of(state(now - 60_000), nowMs = now)
        assertTrue(h.reachable)
        assertEquals(summary.system.cpu.total, h.cpu)
    }

    @Test
    fun a_reading_older_than_ten_minutes_is_unreachable_and_shows_no_numbers() {
        val now = 1_000_000_000L
        val h = Headline.of(state(now - Headline.STALE_AFTER_MS - 1), nowMs = now)
        assertFalse(h.reachable)
        assertNull(h.cpu)
        assertNull(h.cpuTemp)
        // The age a surface prints is still the real last answer.
        assertEquals(now - Headline.STALE_AFTER_MS - 1, h.fetchedMs)
    }
}
