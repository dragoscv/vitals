package app.vitals.device.internal

import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.os.BatteryManager
import android.os.Build
import app.vitals.device.model.BatteryState

/**
 * The sticky ACTION_BATTERY_CHANGED intent plus the fuel-gauge properties.
 * Reading the sticky intent costs one binder call and registers nothing, so
 * no receiver outlives the screen.
 */
internal class BatterySampler(private val context: Context) {
    private val manager = context.getSystemService(BatteryManager::class.java)

    fun sample(): BatteryState? {
        val intent = context.registerReceiver(null, IntentFilter(Intent.ACTION_BATTERY_CHANGED)) ?: return null
        if (!intent.getBooleanExtra(BatteryManager.EXTRA_PRESENT, true)) return null
        val level = intent.getIntExtra(BatteryManager.EXTRA_LEVEL, -1)
        val scale = intent.getIntExtra(BatteryManager.EXTRA_SCALE, 100)
        if (level < 0 || scale <= 0) return null
        val percent = level * 100f / scale
        val status = status(intent.getIntExtra(BatteryManager.EXTRA_STATUS, -1))
        val voltage = intent.getIntExtra(BatteryManager.EXTRA_VOLTAGE, -1).takeIf { it > 0 }?.let { it / 1000f }
        val current = currentMa(status)
        return BatteryState(
            percent = percent,
            status = status,
            plugged = plugged(intent.getIntExtra(BatteryManager.EXTRA_PLUGGED, 0)),
            health = health(intent.getIntExtra(BatteryManager.EXTRA_HEALTH, -1)),
            temperatureC = intent.getIntExtra(BatteryManager.EXTRA_TEMPERATURE, Int.MIN_VALUE)
                .takeIf { it != Int.MIN_VALUE }?.let { it / 10f },
            voltageV = voltage,
            currentMa = current,
            powerW = if (voltage != null && current != null) voltage * current / 1000f else null,
            cycleCount = cycles(intent),
            technology = intent.getStringExtra(BatteryManager.EXTRA_TECHNOLOGY)?.takeIf { it.isNotBlank() },
            capacityMah = capacity(percent),
            chargeTimeRemainingMs = if (status == "charging") {
                manager?.computeChargeTimeRemaining()?.takeIf { it > 0 }
            } else {
                null
            },
        )
    }

    /**
        * Microamps, as documented (see [currentToMa] for why no unit guessing).
        * The sign follows the status, because vendors disagree on it: positive
        * means into the battery.
     */
    private fun currentMa(status: String): Float? {
        val raw = manager?.getIntProperty(BatteryManager.BATTERY_PROPERTY_CURRENT_NOW) ?: return null
        if (raw == Int.MIN_VALUE) return null
        // Zero while charging or discharging is impossible, so it means the
        // gauge said nothing. Plugged in but held (notCharging, full) really
        // is 0 mA: the A51 on USB reads exactly that, 2026-09-29.
        if (raw == 0 && (status == "discharging" || status == "charging")) return null
        val ma = currentToMa(raw)
        return when {
            status == "discharging" && ma > 0 -> -ma
            status == "charging" && ma < 0 -> -ma
            else -> ma
        }
    }

    /** Full charge in mAh from the charge counter; the same µAh-or-mAh ambiguity as the current. */
    private fun capacity(percent: Float): Float? {
        if (percent < 5f) return null
        val raw = manager?.getIntProperty(BatteryManager.BATTERY_PROPERTY_CHARGE_COUNTER) ?: return null
        if (raw <= 0 || raw == Int.MIN_VALUE) return null
        val mah = if (raw > 50_000) raw / 1000f else raw.toFloat()
        return mah / (percent / 100f)
    }

    private fun cycles(intent: Intent): Int? {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.UPSIDE_DOWN_CAKE) return null
        // Samsung broadcasts cycle_count:0 on a year-old S25 and keeps its
        // real count elsewhere; zero cycles is not a fact about any battery
        // that has been charged, so it is unreported, not "new".
        return intent.getIntExtra(BatteryManager.EXTRA_CYCLE_COUNT, -1).takeIf { it > 0 }
    }

    private fun status(v: Int) = when (v) {
        BatteryManager.BATTERY_STATUS_CHARGING -> "charging"
        BatteryManager.BATTERY_STATUS_DISCHARGING -> "discharging"
        BatteryManager.BATTERY_STATUS_FULL -> "full"
        BatteryManager.BATTERY_STATUS_NOT_CHARGING -> "notCharging"
        else -> "unknown"
    }

    private fun plugged(v: Int) = when (v) {
        BatteryManager.BATTERY_PLUGGED_AC -> "ac"
        BatteryManager.BATTERY_PLUGGED_USB -> "usb"
        BatteryManager.BATTERY_PLUGGED_WIRELESS -> "wireless"
        BatteryManager.BATTERY_PLUGGED_DOCK -> "dock"
        else -> null
    }

    private fun health(v: Int) = when (v) {
        BatteryManager.BATTERY_HEALTH_GOOD -> "good"
        BatteryManager.BATTERY_HEALTH_OVERHEAT -> "overheat"
        BatteryManager.BATTERY_HEALTH_DEAD -> "dead"
        BatteryManager.BATTERY_HEALTH_OVER_VOLTAGE -> "overVoltage"
        BatteryManager.BATTERY_HEALTH_COLD -> "cold"
        else -> "unknown"
    }
}
