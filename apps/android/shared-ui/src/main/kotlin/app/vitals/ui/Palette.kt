package app.vitals.ui

import androidx.compose.ui.graphics.Color

/**
 * The desktop's categorical palette (`packages/ui/src/styles/theme.css`),
 * converted from OKLCH to sRGB. A metric has one colour on every surface —
 * CPU is the same blue on the PC, the phone, the widget and the watch — and
 * it never follows the dynamic accent, which the user can set to anything.
 *
 * Chosen upstream to stay distinguishable under the three common
 * colour-vision deficiencies and to hold at least 3:1 on light and dark.
 */
object Palette {
    val Cpu = Color(0xFF0088F2)
    val Memory = Color(0xFF00B667)
    val Disk = Color(0xFFCB9400)
    val Network = Color(0xFFAC67EF)
    val Gpu = Color(0xFFF05653)
    val Thermal = Color(0xFFF6722B)
    val Power = Color(0xFF00C0C2)

    val Ok = Color(0xFF1EAB53)
    val Warn = Color(0xFFE89D00)
    val Danger = Color(0xFFEE343B)

    /** The brand accent, for surfaces where dynamic colour is unavailable (API < 31, the watch). */
    // Fern, from brand/mark.mjs: the desktop's `green` accent at its light and
    // dark lightness (4.57:1 on white, 8.3:1 under dark text; brand/contrast-pairs.json).
    val Accent = Color(0xFF22864A)
    val AccentOnDark = Color(0xFF6CB882)

    fun of(level: Level): Color = when (level) {
        Level.Ok -> Ok
        Level.Warn -> Warn
        Level.Danger -> Danger
        Level.Unknown -> Color(0xFF8A8F98)
    }
}

/** How worried to look. [Unknown] is an unmeasured value, never "fine". */
enum class Level { Ok, Warn, Danger, Unknown }

/**
 * Thresholds, matching the desktop's alert engine closely enough that the
 * watch face does not turn red while the PC says all is well.
 */
object Thresholds {
    fun cpu(percent: Float?): Level = band(percent, 85f, 95f)
    fun memory(percent: Float?): Level = band(percent, 85f, 95f)
    fun cpuTemp(celsius: Float?): Level = band(celsius, 85f, 95f)
    fun gpuTemp(celsius: Float?): Level = band(celsius, 80f, 90f)
    fun diskTemp(celsius: Float?): Level = band(celsius, 55f, 65f)

    fun temperature(key: String, celsius: Float?): Level = when {
        key.startsWith("gpu") || key.contains("nvml") -> gpuTemp(celsius)
        key.startsWith("drive") || key.startsWith("disk") || key.contains("storage") -> diskTemp(celsius)
        else -> cpuTemp(celsius)
    }

    private fun band(v: Float?, warn: Float, danger: Float): Level = when {
        v == null || v.isNaN() -> Level.Unknown
        v >= danger -> Level.Danger
        v >= warn -> Level.Warn
        else -> Level.Ok
    }
}
