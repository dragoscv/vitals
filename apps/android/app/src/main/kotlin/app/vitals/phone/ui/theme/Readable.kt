package app.vitals.phone.ui.theme

import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.ui.graphics.Color
import app.vitals.ui.Contrast

/**
 * [colour] adjusted so it reads as text on every surface this app puts text
 * on: the window background, cards, glass bars and sheets. Checking against
 * all of them, not only the card, is what makes it hold under dynamic
 * colour, where the user's wallpaper decides how far apart those are.
 * Use for words and numbers only; arcs and bars keep the raw palette.
 */
@Composable
@ReadOnlyComposable
fun readable(colour: Color): Color {
    if (colour == Color.Unspecified) return colour
    val s = MaterialTheme.colorScheme
    return listOf(s.background, s.surfaceContainer, s.surfaceContainerHigh, s.surfaceContainerHighest)
        .fold(colour) { c, bg -> Contrast.readable(c, bg) }
}
