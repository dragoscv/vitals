package app.vitals.ui

import org.junit.Assert.assertEquals
import org.junit.Test

class FormatTest {
    @Test
    fun a_percentage_over_a_zero_total_is_a_dash_not_nan_or_a_crash() {
        val used = 0L
        val total = 0L
        assertEquals(Format.DASH, Format.percent(used * 100f / total))
        assertEquals(Format.DASH, Format.percent(Float.POSITIVE_INFINITY))
        assertEquals(Format.DASH, Format.percentCompact(Float.NaN))
        assertEquals("43 %", Format.percent(43.2f))
    }
}
