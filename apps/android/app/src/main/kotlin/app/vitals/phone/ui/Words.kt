package app.vitals.phone.ui

import android.content.res.Resources
import android.text.format.DateUtils
import androidx.annotation.StringRes
import app.vitals.core.model.AlertKind
import app.vitals.core.model.ControlError
import app.vitals.core.model.Priority
import app.vitals.core.model.ThrottleReason
import app.vitals.core.net.ApiFailure
import app.vitals.phone.R

// The desktop sends alert titles as its own translation keys, which mean
// nothing here, so the phone words each kind itself in both languages.

@StringRes
fun AlertKind.title(): Int = when (this) {
    AlertKind.CpuSustained -> R.string.alert_cpu_sustained
    AlertKind.CpuThrottled -> R.string.alert_cpu_throttled
    AlertKind.MemoryPressure -> R.string.alert_memory_pressure
    AlertKind.MemoryCommit -> R.string.alert_memory_commit
    AlertKind.DiskSaturated -> R.string.alert_disk_saturated
    AlertKind.DiskLatency -> R.string.alert_disk_latency
    AlertKind.DiskSpace -> R.string.alert_disk_space
    AlertKind.DiskHealth -> R.string.alert_disk_health
    AlertKind.GpuThrottled -> R.string.alert_gpu_throttled
    AlertKind.ThermalCpu -> R.string.alert_thermal_cpu
    AlertKind.NetworkErrors -> R.string.alert_network_errors
    AlertKind.BatteryLow -> R.string.alert_battery_low
    AlertKind.BatteryHealth -> R.string.alert_battery_health
}

@StringRes
fun ThrottleReason.words(): Int = when (this) {
    ThrottleReason.Thermal -> R.string.throttle_thermal
    ThrottleReason.PowerLimit -> R.string.throttle_power
    ThrottleReason.CurrentLimit -> R.string.throttle_current
    ThrottleReason.VoltageDrop -> R.string.throttle_voltage
    ThrottleReason.PowerPolicy -> R.string.throttle_policy
    ThrottleReason.Unknown -> R.string.throttle_unknown
}

@StringRes
fun Priority.words(): Int = when (this) {
    Priority.Idle -> R.string.priority_idle
    Priority.BelowNormal -> R.string.priority_below_normal
    Priority.Normal -> R.string.priority_normal
    Priority.AboveNormal -> R.string.priority_above_normal
    Priority.High -> R.string.priority_high
    Priority.Realtime -> R.string.priority_realtime
}

/**
 * "Read-only phone", "Windows said no" and "already gone" need different
 * next steps from the user, so they are never collapsed into one message.
 */
fun ApiFailure.controlMessage(context: Resources): String = when (this) {
    is ApiFailure.Refused -> when (val e = error) {
        ControlError.Forbidden -> context.getString(R.string.control_forbidden)
        ControlError.AccessDenied -> context.getString(R.string.control_access_denied)
        ControlError.NotFound -> context.getString(R.string.control_not_found)
        is ControlError.Unsupported -> context.getString(R.string.control_unsupported, e.message)
        is ControlError.Internal -> context.getString(R.string.control_internal, e.message)
    }
    is ApiFailure.Unreachable -> context.getString(R.string.control_unreachable)
    ApiFailure.Unauthorised -> context.getString(R.string.state_unauthorised)
    is ApiFailure.Incompatible -> context.getString(R.string.state_incompatible)
    ApiFailure.NotReady -> context.getString(R.string.pairing_not_ready)
    is ApiFailure.Http -> context.getString(R.string.control_failed, status)
}

fun relativeTime(ms: Long): String =
    DateUtils.getRelativeTimeSpanString(ms, System.currentTimeMillis(), DateUtils.MINUTE_IN_MILLIS).toString()
