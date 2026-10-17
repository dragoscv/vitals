package app.vitals.wear.ui

import androidx.annotation.StringRes
import app.vitals.core.model.Alert
import app.vitals.core.model.AlertKind
import app.vitals.wear.R

// The desktop sends an alert's `title` and `cause` as its own translation
// keys ("alert.cpu_hot.title"), which mean nothing on a watch. Showing them
// raw put those keys on the wrist, so the watch words each kind itself, as
// the phone and the TV do (`:app` and `:tv` Words.kt — same wording). Not
// shared: `:shared-ui` has no Android resources and each app owns its own.

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

/** What the watch shows for an alert: a worded title and the subject (a disk, a GPU), never the wire keys. */
data class AlertText(val title: String, val body: String?)

fun alertText(alert: Alert, string: (Int) -> String): AlertText =
    AlertText(string(alert.kind.title()), alert.subject.takeIf { it.isNotBlank() })
