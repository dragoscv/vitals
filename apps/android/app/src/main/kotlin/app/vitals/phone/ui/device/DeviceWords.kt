package app.vitals.phone.ui.device

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.provider.Settings
import androidx.annotation.StringRes
import app.vitals.phone.R

// Wire strings from :device mapped to resources. :device stays free of
// Android resources so the watch can reuse it with its own wording.

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
    "memory" -> R.string.metric_memory
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
fun batteryStatus(status: String): Int = when (status) {
    "charging" -> R.string.battery_charging
    "discharging" -> R.string.battery_discharging
    "full" -> R.string.device_battery_full
    "notCharging" -> R.string.device_battery_not_charging
    else -> R.string.device_unknown
}

@StringRes
fun batteryHealth(health: String): Int = when (health) {
    "good" -> R.string.device_health_good
    "overheat" -> R.string.device_health_overheat
    "dead" -> R.string.device_health_dead
    "overVoltage" -> R.string.device_health_over_voltage
    "cold" -> R.string.device_health_cold
    else -> R.string.device_unknown
}

@StringRes
fun plugged(p: String?): Int = when (p) {
    "ac" -> R.string.device_plug_ac
    "usb" -> R.string.device_plug_usb
    "wireless" -> R.string.device_plug_wireless
    "dock" -> R.string.device_plug_dock
    else -> R.string.device_plug_none
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

/** Opens the system screen for one special access; each has its own intent and none can be requested in-app. */
object SpecialAccess {
    fun usage(context: Context) = open(
        context,
        Intent(Settings.ACTION_USAGE_ACCESS_SETTINGS, Uri.parse("package:${context.packageName}")),
        Intent(Settings.ACTION_USAGE_ACCESS_SETTINGS),
    )

    fun allFiles(context: Context) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) return appInfo(context, context.packageName)
        open(
            context,
            Intent(Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION, Uri.parse("package:${context.packageName}")),
            Intent(Settings.ACTION_MANAGE_ALL_FILES_ACCESS_PERMISSION),
        )
    }

    fun appInfo(context: Context, pkg: String) = open(
        context,
        Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:$pkg")),
        Intent(Settings.ACTION_MANAGE_APPLICATIONS_SETTINGS),
    )

    /** Some OEM builds lack the per-app form of an intent; the list form always exists. */
    private fun open(context: Context, first: Intent, fallback: Intent) {
        val flags = Intent.FLAG_ACTIVITY_NEW_TASK
        runCatching { context.startActivity(first.addFlags(flags)) }
            .onFailure { runCatching { context.startActivity(fallback.addFlags(flags)) } }
    }
}
