package app.vitals.tv.ui

import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.tv.material3.ColorScheme
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.darkColorScheme
import app.vitals.ui.Contrast
import app.vitals.ui.Palette

/**
 * Always dark: a TV is watched in a dim room, and a light surface on a 65-inch
 * panel is a floodlight. The brand accent, not dynamic colour — Google TV has
 * no wallpaper to derive a scheme from. Text pairs are moved to 4.5:1 the same
 * way the phone does it, so a reading is legible from the sofa (WCAG 1.4.3).
 */
private val Scheme: ColorScheme = darkColorScheme(
    primary = Palette.AccentOnDark,
    onPrimary = Color(0xFF003061),
    primaryContainer = Color(0xFF004788),
    onPrimaryContainer = Color(0xFFD5E3FF),
    secondary = Color(0xFFBCC7DC),
    background = Color(0xFF111318),
    onBackground = Color(0xFFE2E2E9),
    surface = Color(0xFF1D2024),
    onSurface = Color(0xFFE2E2E9),
    surfaceVariant = Color(0xFF33353A),
    onSurfaceVariant = Color(0xFFC4C6D0),
    // The focused card inverts to near-white: the one thing on screen the
    // eye must find at three metres is where the D-pad is.
    inverseSurface = Color(0xFFE2E2E9),
    inverseOnSurface = Color(0xFF2E3036),
    border = Color(0xFF8E9099),
    error = Color(0xFFFFB4AB),
).let { s ->
    s.copy(
        onSurface = Contrast.readable(s.onSurface, s.surfaceVariant),
        onSurfaceVariant = Contrast.readable(s.onSurfaceVariant, s.surfaceVariant),
        onBackground = Contrast.readable(s.onBackground, s.background),
        inverseOnSurface = Contrast.readable(s.inverseOnSurface, s.inverseSurface),
    )
}

@Composable
fun VitalsTvTheme(content: @Composable () -> Unit) {
    MaterialTheme(colorScheme = Scheme, content = content)
}

/** A metric colour as text on a dark card: the palette is tuned for arcs, so text is lifted to 4.5:1. */
@Composable
fun readable(colour: Color): Color = Contrast.readable(colour, MaterialTheme.colorScheme.surface)
