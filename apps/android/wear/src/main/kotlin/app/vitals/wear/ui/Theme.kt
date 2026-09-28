package app.vitals.wear.ui

import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.wear.compose.material3.ColorScheme
import androidx.wear.compose.material3.MaterialTheme
import androidx.wear.compose.material3.dynamicColorScheme
import app.vitals.ui.Contrast
import app.vitals.ui.Palette

/**
 * The system's dynamic colours when the watch offers them (One UI 8 Watch
 * does, from the watch face), otherwise a scheme built on the brand accent.
 * Metric colours never follow either: CPU is the same blue on every surface.
 */
@Composable
fun VitalsTheme(content: @Composable () -> Unit) {
    val context = LocalContext.current
    val scheme = remember(context) { (dynamicColorScheme(context) ?: BrandScheme).withReadablePairs() }
    MaterialTheme(colorScheme = scheme, content = content)
}

/**
 * The watch-face palette on the Galaxy Watch 7 put button labels at 4.0:1 on
 * their fill ("Busiest apps", measured 2026-09-28). The fills keep the
 * wearer's colour; only the text on them moves until it reads at 4.5:1.
 */
private fun ColorScheme.withReadablePairs(): ColorScheme = copy(
    onPrimary = Contrast.readable(onPrimary, primary),
    onPrimaryContainer = Contrast.readable(onPrimaryContainer, primaryContainer),
    onSecondary = Contrast.readable(onSecondary, secondary),
    onSecondaryContainer = Contrast.readable(onSecondaryContainer, secondaryContainer),
    onTertiary = Contrast.readable(onTertiary, tertiary),
    onTertiaryContainer = Contrast.readable(onTertiaryContainer, tertiaryContainer),
    onSurface = Contrast.readable(onSurface, surfaceContainer),
    onSurfaceVariant = Contrast.readable(onSurfaceVariant, surfaceContainer),
)

private val BrandScheme = ColorScheme(
    primary = Palette.AccentOnDark,
    primaryDim = Palette.Accent,
    primaryContainer = Color(0xFF0B3A66),
    onPrimary = Color(0xFF002A52),
    onPrimaryContainer = Color(0xFFD3E6FF),
    secondary = Color(0xFFB7C8E0),
    secondaryDim = Color(0xFF93A6C0),
    secondaryContainer = Color(0xFF2A3748),
    onSecondary = Color(0xFF1B2838),
    onSecondaryContainer = Color(0xFFD6E3F7),
    tertiary = Palette.Power,
    onTertiary = Color(0xFF00363A),
    surfaceContainerLow = Color(0xFF15191F),
    surfaceContainer = Color(0xFF1D222A),
    surfaceContainerHigh = Color(0xFF272D36),
    onSurface = Color(0xFFE4E8EF),
    onSurfaceVariant = Color(0xFFB9C1CD),
    outline = Color(0xFF7D8694),
    outlineVariant = Color(0xFF444C58),
    background = Color.Black,
    onBackground = Color(0xFFE4E8EF),
    error = Palette.Danger,
    onError = Color(0xFF410002),
)
