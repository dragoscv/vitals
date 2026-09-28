package app.vitals.ui

import androidx.compose.ui.graphics.Color
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ContrastTest {
    // The real surfaces the text sits on: the brand light and dark cards, the
    // watch's card and its black background.
    private val surfaces = listOf(
        Color(0xFFECEEF4), Color(0xFFF8F9FF), Color(0xFF1D2024), Color(0xFF111318),
        Color(0xFF1D222A), Color(0xFF000000), Color.White,
    )
    private val palette = listOf(
        Palette.Cpu, Palette.Memory, Palette.Disk, Palette.Network, Palette.Gpu, Palette.Thermal,
        Palette.Power, Palette.Ok, Palette.Warn, Palette.Danger, Palette.of(Level.Unknown),
    )

    @Test
    fun every_palette_colour_reads_at_four_and_a_half_to_one_on_every_surface() {
        for (bg in surfaces) for (c in palette) {
            val r = Contrast.ratio(Contrast.readable(c, bg), bg)
            assertTrue("$c on $bg is $r:1", r >= Contrast.TEXT_MIN)
        }
    }

    @Test
    fun a_colour_that_already_reads_is_left_alone_so_dark_mode_keeps_its_hues() {
        val bg = Color(0xFF111318)
        assertEquals(Palette.Memory, Contrast.readable(Palette.Memory, bg))
    }

    @Test
    fun the_hue_survives_so_warning_amber_does_not_turn_into_plain_grey() {
        val fixed = Contrast.readable(Palette.Warn, Color(0xFFECEEF4))
        assertTrue("red channel should stay dominant: $fixed", fixed.red > fixed.blue * 2)
    }

    @Test
    fun unspecified_passes_through_so_default_text_colour_is_kept() {
        assertEquals(Color.Unspecified, Contrast.readable(Color.Unspecified, Color.White))
    }
}
