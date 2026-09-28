package app.vitals.phone.ui.theme

import android.os.Build
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import app.vitals.ui.Contrast
import app.vitals.ui.Palette

private val BrandLight = lightColorScheme(
    primary = Palette.Accent,
    onPrimary = Color.White,
    primaryContainer = Color(0xFFD5E3FF),
    onPrimaryContainer = Color(0xFF001B3C),
    secondary = Color(0xFF545F71),
    tertiary = Color(0xFF006A6B),
    background = Color(0xFFF8F9FF),
    surface = Color(0xFFF8F9FF),
    surfaceContainer = Color(0xFFECEEF4),
    surfaceContainerHigh = Color(0xFFE6E8EE),
    surfaceContainerHighest = Color(0xFFE1E2E8),
)

private val BrandDark = darkColorScheme(
    primary = Palette.AccentOnDark,
    onPrimary = Color(0xFF003061),
    primaryContainer = Color(0xFF004788),
    onPrimaryContainer = Color(0xFFD5E3FF),
    secondary = Color(0xFFBCC7DC),
    tertiary = Color(0xFF4CDADB),
    background = Color(0xFF111318),
    surface = Color(0xFF111318),
    surfaceContainer = Color(0xFF1D2024),
    surfaceContainerHigh = Color(0xFF282A2F),
    surfaceContainerHighest = Color(0xFF33353A),
)

@Composable
fun VitalsTheme(content: @Composable () -> Unit) {
    val dark = isSystemInDarkTheme()
    val context = LocalContext.current
    val scheme: ColorScheme = remember(dark) {
        when {
            Build.VERSION.SDK_INT >= Build.VERSION_CODES.S ->
                if (dark) dynamicDarkColorScheme(context) else dynamicLightColorScheme(context)
            dark -> BrandDark
            else -> BrandLight
        }.withReadablePairs()
    }
    // MaterialExpressiveTheme and MotionScheme are still internal in
    // material3 1.4.0 stable; the expressive feel comes from the spring
    // specs used by the gauges and transitions instead.
    MaterialTheme(colorScheme = scheme, content = content)
}

/**
 * Wallpaper-derived schemes are not guaranteed to pair text and fill at
 * 4.5:1; the fills keep the user's colour and only the text on them moves.
 */
private fun ColorScheme.withReadablePairs(): ColorScheme = copy(
    onPrimary = Contrast.readable(onPrimary, primary),
    onPrimaryContainer = Contrast.readable(onPrimaryContainer, primaryContainer),
    onSecondary = Contrast.readable(onSecondary, secondary),
    onSecondaryContainer = Contrast.readable(onSecondaryContainer, secondaryContainer),
    onTertiary = Contrast.readable(onTertiary, tertiary),
    onTertiaryContainer = Contrast.readable(onTertiaryContainer, tertiaryContainer),
    onSurface = Contrast.readable(onSurface, surfaceContainerHighest),
    onSurfaceVariant = Contrast.readable(onSurfaceVariant, surfaceContainerHighest),
    onBackground = Contrast.readable(onBackground, background),
)
