package app.vitals.phone.ui.theme

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import app.vitals.ui.Palette
import dev.chrisbanes.haze.HazeInput
import dev.chrisbanes.haze.HazeState
import dev.chrisbanes.haze.blur.HazeBlurStyle
import dev.chrisbanes.haze.blur.HazeColorEffect
import dev.chrisbanes.haze.blur.hazeBlur
import dev.chrisbanes.haze.hazeSource
import androidx.compose.ui.unit.dp

/** How the bars and sheets are drawn. [state] is null when blur is off, which is the cheap path. */
@Immutable
class Glass(val state: HazeState?)

val LocalGlass = staticCompositionLocalOf { Glass(null) }

@Composable
fun ProvideGlass(blur: Boolean, content: @Composable () -> Unit) {
    val state = remember(blur) { if (blur) HazeState() else null }
    CompositionLocalProvider(LocalGlass provides Glass(state), content = content)
}

/** Marks the content a glass surface frosts. A no-op without blur. */
@Composable
fun Modifier.glassSource(): Modifier {
    val state = LocalGlass.current.state ?: return this
    return this.hazeSource(state)
}

/**
 * A frosted surface when blur is on, else the same surface tinted at 86 %
 * opacity. The tint is kept in the blurred style too so both paths share a
 * colour and a screenshot of either reads as the same design.
 */
@Composable
fun Modifier.glassSurface(): Modifier {
    val tint = MaterialTheme.colorScheme.surfaceContainer
    val state = LocalGlass.current.state
        ?: return this.background(tint.copy(alpha = 0.86f))
    val style = remember(tint) {
        HazeBlurStyle {
            blurRadius(24.dp)
            backgroundColor(tint)
            colorEffects(listOf(HazeColorEffect.tint(tint.copy(alpha = 0.62f))))
            noiseFactor(0.12f)
        }
    }
    return this.hazeBlur(HazeInput.Backdrop(state), style)
}

/**
 * The soft mesh behind every screen. A static brush built once per scheme:
 * an animated background would repaint the whole window every frame.
 */
@Composable
fun MeshBackground(modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    val base = MaterialTheme.colorScheme.background
    val primary = MaterialTheme.colorScheme.primary
    val layers = remember(base, primary) {
        listOf(
            Brush.radialGradient(
                listOf(primary.copy(alpha = 0.16f), Color.Transparent),
                center = Offset(0f, 0f),
                radius = 1400f,
            ),
            Brush.radialGradient(
                listOf(Palette.Memory.copy(alpha = 0.08f), Color.Transparent),
                center = Offset(1400f, 900f),
                radius = 1200f,
            ),
            Brush.radialGradient(
                listOf(Palette.Network.copy(alpha = 0.08f), Color.Transparent),
                center = Offset(200f, 2400f),
                radius = 1300f,
            ),
        )
    }
    Box(
        modifier
            .fillMaxSize()
            .background(base)
            .background(layers[0])
            .background(layers[1])
            .background(layers[2]),
    ) { content() }
}
