package app.vitals.ui

import java.util.Locale
import kotlin.math.abs
import kotlin.math.roundToInt

/**
 * Number formatting shared by the phone and the watch.
 *
 * Every function takes a nullable and returns [DASH] for `null`: an
 * unmeasured reading is an em dash on every surface, never "0". Numbers use
 * the device locale's decimal separator, so Romanian shows `12,5 GB`.
 */
object Format {
    const val DASH = "\u2014"

    fun percent(v: Float?): String = v?.let { "${it.roundToInt()} %" } ?: DASH

    /** Without the space, for a tile or complication where width is scarce. */
    fun percentCompact(v: Float?): String = v?.let { "${it.roundToInt()}%" } ?: DASH

    fun celsius(v: Float?): String = v?.let { "${it.roundToInt()} °C" } ?: DASH

    fun celsiusCompact(v: Float?): String = v?.let { "${it.roundToInt()}°" } ?: DASH

    fun watts(v: Float?): String = v?.let { if (abs(it) < 10) "${one(it)} W" else "${it.roundToInt()} W" } ?: DASH

    fun rpm(v: Int?): String = v?.let { "$it RPM" } ?: DASH

    fun volts(v: Float?): String = v?.let { String.format(Locale.getDefault(), "%.2f V", it) } ?: DASH

    fun ghz(hz: Long?): String = hz?.let { "${one(it / 1e9f)} GHz" } ?: DASH

    /** Binary units, as the desktop uses: 1 GB = 1024³ bytes. */
    fun bytes(b: Long?): String {
        b ?: return DASH
        val units = arrayOf("B", "KB", "MB", "GB", "TB", "PB")
        var v = b.toDouble()
        var i = 0
        while (v >= 1024 && i < units.lastIndex) {
            v /= 1024; i++
        }
        return if (i == 0 || v >= 100) "${v.roundToInt()} ${units[i]}" else "${one(v.toFloat())} ${units[i]}"
    }

    fun rate(bps: Long?): String = bps?.let { "${bytes(it)}/s" } ?: DASH

    fun duration(secs: Long?): String {
        secs ?: return DASH
        val d = secs / 86_400
        val h = (secs % 86_400) / 3_600
        val m = (secs % 3_600) / 60
        return when {
            d > 0 -> "${d}d ${h}h"
            h > 0 -> "${h}h ${m}m"
            else -> "${m}m"
        }
    }

    /** A sensor line's value, by its unit key (`SensorLine.unit`). */
    fun sensor(unit: String, value: Float): String = when (unit) {
        "temperature" -> celsius(value)
        "power" -> watts(value)
        "voltage" -> volts(value)
        "fanSpeed" -> rpm(value.roundToInt())
        "charge", "percent" -> percent(value)
        else -> one(value)
    }

    private fun one(v: Float): String = String.format(Locale.getDefault(), "%.1f", v)
}
