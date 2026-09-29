package app.vitals.tv.ui

import android.content.res.Resources
import android.text.format.DateUtils
import androidx.annotation.StringRes
import app.vitals.core.model.AlertKind
import app.vitals.core.model.ControlError
import app.vitals.core.model.Priority
import app.vitals.core.model.ThrottleReason
import app.vitals.core.net.ApiFailure
import app.vitals.core.pairing.PairRefusal
import app.vitals.tv.R

// Wire values mapped to this app's resources. The desktop sends alert titles
// as its own translation keys, which mean nothing here, and :device stays free
// of Android resources, so each surface words them itself.

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

/** "Read-only TV", "Windows said no" and "already gone" need different next steps, so they are never merged. */
fun ApiFailure.controlMessage(res: Resources): String = when (this) {
    is ApiFailure.Refused -> when (val e = error) {
        ControlError.Forbidden -> res.getString(R.string.control_forbidden)
        ControlError.AccessDenied -> res.getString(R.string.control_access_denied)
        ControlError.NotFound -> res.getString(R.string.control_not_found)
        is ControlError.Unsupported -> res.getString(R.string.control_unsupported, e.message)
        is ControlError.Internal -> res.getString(R.string.control_internal, e.message)
    }
    is ApiFailure.Unreachable -> res.getString(R.string.control_unreachable)
    ApiFailure.Unauthorised -> res.getString(R.string.state_unauthorised)
    is ApiFailure.Incompatible -> res.getString(R.string.state_incompatible)
    ApiFailure.NotReady -> res.getString(R.string.pairing_not_ready)
    is ApiFailure.Http -> res.getString(R.string.control_failed, status)
}

@StringRes
fun PairRefusal.message(): Int = when (this) {
    PairRefusal.BadAddress -> R.string.pairing_bad_address
    PairRefusal.NotPrivate -> R.string.pairing_not_private
    PairRefusal.BadToken -> R.string.pairing_bad_token
    PairRefusal.BadCode -> R.string.pairing_bad_code
    PairRefusal.Unreachable -> R.string.pairing_unreachable
    PairRefusal.Unauthorised -> R.string.pairing_unauthorised
    PairRefusal.Incompatible -> R.string.pairing_incompatible
    PairRefusal.NotReady -> R.string.pairing_not_ready
    PairRefusal.Failed -> R.string.pairing_failed
}

@StringRes
fun deviceAlertTitle(kind: String): Int = when (kind) {
    "thermal" -> R.string.device_alert_thermal
    "batteryHot" -> R.string.device_alert_battery_hot
    "batteryLow" -> R.string.alert_battery_low
    "memoryPressure" -> R.string.alert_memory_pressure
    "storageLow" -> R.string.device_alert_storage_low
    "cpuSustained" -> R.string.alert_cpu_sustained
    else -> R.string.alert_count_title
}

@StringRes
fun clusterRole(role: String): Int = when (role) {
    "prime" -> R.string.device_cluster_prime
    "performance" -> R.string.device_cluster_performance
    "efficiency" -> R.string.device_cluster_efficiency
    else -> R.string.metric_cpu
}

@StringRes
fun zoneGroupName(group: String): Int = when (group) {
    "cpu" -> R.string.metric_cpu
    "gpu" -> R.string.metric_gpu
    "battery" -> R.string.metric_battery
    "skin" -> R.string.device_zone_skin
    "modem" -> R.string.device_zone_modem
    "camera" -> R.string.device_zone_camera
    "memory" -> R.string.device_zone_memory
    "npu" -> R.string.device_zone_npu
    else -> R.string.sensors_group_other
}

@StringRes
fun thermalStatus(status: String): Int = when (status) {
    "light" -> R.string.device_thermal_light
    "moderate" -> R.string.device_thermal_moderate
    "severe" -> R.string.device_thermal_severe
    "critical", "emergency", "shutdown" -> R.string.device_thermal_critical
    else -> R.string.device_thermal_none
}

@StringRes
fun transport(t: String?): Int = when (t) {
    "wifi" -> R.string.device_net_wifi
    "cellular" -> R.string.device_net_cellular
    "ethernet" -> R.string.device_net_ethernet
    "vpn" -> R.string.device_net_vpn
    "none", null -> R.string.net_disconnected
    else -> R.string.sensors_group_other
}

@StringRes
fun cleanupKind(kind: String): Int = when (kind) {
    "appCache" -> R.string.device_clean_app_cache
    "largeFile" -> R.string.device_clean_large_file
    "apk" -> R.string.device_clean_apk
    "emptyFolder" -> R.string.device_clean_empty_folder
    "thumbnails" -> R.string.device_clean_thumbnails
    else -> R.string.sensors_group_other
}

fun relativeTime(ms: Long): String =
    DateUtils.getRelativeTimeSpanString(ms, System.currentTimeMillis(), DateUtils.MINUTE_IN_MILLIS).toString()
