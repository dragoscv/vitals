package app.vitals.wear.ui

import androidx.compose.runtime.Composable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.ui.graphics.Color
import androidx.wear.compose.material3.MaterialTheme
import app.vitals.ui.Contrast

/**
 * [colour] adjusted to read as text on the watch's black background and on
 * its cards. The raw palette's CPU blue and danger red fall below 4.5:1 on a
 * card; at arm's length on a 1.3-inch screen that is unreadable, not merely
 * pale. Words and numbers only — rings keep the raw palette.
 */
@Composable
@ReadOnlyComposable
fun readable(colour: Color): Color {
    if (colour == Color.Unspecified) return colour
    val s = MaterialTheme.colorScheme
    return listOf(s.background, s.surfaceContainerLow, s.surfaceContainer, s.surfaceContainerHigh)
        .fold(colour) { c, bg -> Contrast.readable(c, bg) }
}
