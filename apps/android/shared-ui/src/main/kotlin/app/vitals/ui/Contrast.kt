package app.vitals.ui

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.graphics.luminance

/**
 * Text colours that stay readable on the surface they are drawn on.
 *
 * The categorical palette is tuned for arcs, bars and dots, where 3:1 is
 * enough (WCAG 1.4.11). Used as *text* it failed everywhere light: memory
 * green was 2.3:1 on a light card, warning amber 2.0:1, and a dark card made
 * danger red 4.0:1 — words people could not read (reported on the S25 and
 * the A51, 2026-09-28). The hue carries the meaning, so it is kept and only
 * the lightness moves: towards black on light surfaces, towards white on
 * dark ones, until the pair reaches 4.5:1 (WCAG 1.4.3).
 */
object Contrast {
    const val TEXT_MIN = 4.5f

    fun ratio(a: Color, b: Color): Float {
        val la = a.luminance()
        val lb = b.luminance()
        return (maxOf(la, lb) + 0.05f) / (minOf(la, lb) + 0.05f)
    }

    /**
     * [colour] moved just far enough towards black or white to read at
     * [min] against [background]. Unspecified passes through, so callers
     * can hand over "use the default text colour" unchanged.
     */
    fun readable(colour: Color, background: Color, min: Float = TEXT_MIN): Color {
        if (colour == Color.Unspecified || background == Color.Unspecified) return colour
        val opaque = colour.copy(alpha = 1f)
        if (ratio(opaque, background) >= min) return opaque
        val target = if (background.luminance() > 0.5f) Color.Black else Color.White
        // Twenty steps is finer than any visible difference and bounded, so
        // this costs nothing when called during composition.
        for (step in 1..20) {
            val candidate = lerp(opaque, target, step / 20f)
            if (ratio(candidate, background) >= min) return candidate
        }
        return target
    }
}
