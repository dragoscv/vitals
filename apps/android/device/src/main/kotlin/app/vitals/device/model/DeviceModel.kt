package app.vitals.device.model

import kotlinx.serialization.Serializable

// The device this app runs on. Every field the platform refuses to report is
// null, never 0: Android 16 blocks /proc/stat, so "CPU 0 %" would be a lie
// about a phone at full load (ADR-0035).

/** One sample of the fast-changing readings, taken about once a second while a screen shows it. */
@Serializable
data class DeviceSnapshot(
    val timestampMs: Long,
    val cpu: CpuState,
    val gpu: GpuState?,
    val memory: MemoryState,
    val battery: BatteryState?,
    val thermal: ThermalState,
    val network: NetworkState,
)

/**
 * CPU as far as an app may see it. [load] is estimated from how much time each
 * cluster spent at each frequency since the previous sample (cpufreq
 * `time_in_state`), because the counters that give true utilisation are
 * closed to apps. It tracks real load closely on phones, and is labelled as an
 * estimate in the UI.
 */
@Serializable
data class CpuState(
    val cores: List<CoreState>,
    val clusters: List<ClusterState>,
    /** 0..100, or null when no cluster exposes time_in_state. */
    val load: Float?,
    /** Hottest CPU thermal zone, °C. */
    val temperature: Float?,
    /** True when [load] is measured from cpuidle residency; false when estimated from frequency. */
    val measured: Boolean = false,
)

@Serializable
data class CoreState(
    val index: Int,
    val online: Boolean,
    val frequencyHz: Long?,
    val maxFrequencyHz: Long?,
    val cluster: Int,
)

@Serializable
data class ClusterState(
    val id: Int,
    /** "Prime", "Performance", "Efficiency", derived from max frequency rank. */
    val role: String,
    val cores: List<Int>,
    val minFrequencyHz: Long?,
    val maxFrequencyHz: Long?,
    val currentFrequencyHz: Long?,
    /**
        * Frequency-weighted residency over the last interval, 0..100: time at
        * each step times (step - min) / (max - min), divided by the interval.
        * The governor parks idle clusters at the lowest step, so an idle phone
        * reads near 0 and a saturated one near 100; null when the cluster
        * reported no time at all (offline for the whole interval).
     */
    val load: Float?,
)

@Serializable
data class GpuState(
    /** 0..100 from kgsl gpubusy or /sys/kernel/gpu/gpu_busy. */
    val load: Float?,
    val frequencyHz: Long?,
    val maxFrequencyHz: Long?,
    val model: String?,
)

@Serializable
data class MemoryState(
    val totalBytes: Long,
    val availableBytes: Long,
    /** MemTotal - MemAvailable: what apps actually hold. */
    val usedBytes: Long,
    val cachedBytes: Long?,
    val swapTotalBytes: Long?,
    val swapFreeBytes: Long?,
    /** ActivityManager's own verdict: the system is killing background apps. */
    val lowMemory: Boolean,
    val thresholdBytes: Long?,
)

@Serializable
data class BatteryState(
    val percent: Float,
    val status: String, // charging | discharging | full | notCharging | unknown
    val plugged: String?, // ac | usb | wireless | dock | null
    val health: String, // good | overheat | dead | overVoltage | cold | unknown
    val temperatureC: Float?,
    val voltageV: Float?,
    /** Positive while charging, negative while discharging; null when the gauge does not say. */
    val currentMa: Float?,
    val powerW: Float?,
    val cycleCount: Int?,
    val technology: String?,
    /** Estimated full charge now, from the charge counter and the percentage. */
    val capacityMah: Float?,
    val chargeTimeRemainingMs: Long?,
)

@Serializable
data class ThermalState(
    /** PowerManager thermal status: none | light | moderate | severe | critical | emergency | shutdown. */
    val status: String,
    /** PowerManager headroom for the next 10 s, 1.0 = throttling starts; null if unsupported. */
    val headroom: Float?,
    val zones: List<ThermalZone>,
)

@Serializable
data class ThermalZone(
    val name: String,
    /** A readable group: cpu, gpu, battery, skin, modem, camera, other. */
    val group: String,
    val celsius: Float,
)

@Serializable
data class NetworkState(
    val transport: String?, // wifi | cellular | ethernet | vpn | none
    val rxBytesPerSec: Long?,
    val txBytesPerSec: Long?,
    val rxTotalBytes: Long?,
    val txTotalBytes: Long?,
    val downstreamKbps: Int?,
    val upstreamKbps: Int?,
    val metered: Boolean?,
    val validated: Boolean?,
    val wifiRssiDbm: Int?,
    val wifiLinkMbps: Int?,
    val wifiFrequencyMhz: Int?,
    val wifiStandard: String?,
    val cellularGeneration: String?,
    val cellularSignalDbm: Int?,
    val operator: String?,
    val ipv4: String?,
    val ipv6: String?,
)

/** Facts that do not change while the app runs. */
@Serializable
data class DeviceInfo(
    val manufacturer: String,
    val model: String,
    val marketingName: String?,
    val device: String,
    val androidVersion: String,
    val sdkInt: Int,
    val securityPatch: String?,
    val kernel: String?,
    val soc: String?,
    val socManufacturer: String?,
    val abis: List<String>,
    val cpuCores: Int,
    val totalMemoryBytes: Long,
    val displayWidthPx: Int,
    val displayHeightPx: Int,
    val displayDensityDpi: Int,
    val refreshRatesHz: List<Float>,
    val hdr: List<String>,
    val bootTimeMs: Long,
    val mediaPerformanceClass: Int,
    val isLowRam: Boolean,
    val glEsVersion: String?,
    val vulkan: Boolean,
)

/** One volume: internal storage, an SD card, USB. */
@Serializable
data class StorageVolume(
    val id: String,
    val label: String,
    val removable: Boolean,
    val totalBytes: Long,
    val freeBytes: Long,
    /** Categories from StorageStatsManager, bytes; null when not available on this volume. */
    val apps: Long?,
    val images: Long?,
    val video: Long?,
    val audio: Long?,
    /** Shared storage not counted as images, video or audio: downloads, documents, app folders. */
    val otherFiles: Long?,
    /** What is left: the OS, and anything the user cannot see. Derived, so null when any input is. */
    val system: Long?,
)

/** A folder in the storage scan tree. */
@Serializable
data class FolderSize(
    val path: String,
    val name: String,
    val bytes: Long,
    val files: Int,
    val children: List<FolderSize>,
)

/** Something the user could delete, rated like the desktop's cleanup (safe | review | risky). */
@Serializable
data class CleanupItem(
    val kind: String, // appCache | largeFile | duplicateDownload | apk | emptyFolder | thumbnails
    val label: String,
    val path: String?,
    val packageName: String?,
    val bytes: Long,
    val rating: String,
)

/** An installed app with what it cost over the chosen window (UsageStats + NetworkStats + StorageStats). */
@Serializable
data class AppUsage(
    val packageName: String,
    val label: String,
    val system: Boolean,
    val foregroundMs: Long?,
    val lastUsedMs: Long?,
    val launches: Int?,
    val mobileRxBytes: Long?,
    val mobileTxBytes: Long?,
    val wifiRxBytes: Long?,
    val wifiTxBytes: Long?,
    val appBytes: Long?,
    val dataBytes: Long?,
    val cacheBytes: Long?,
    val versionName: String?,
    val targetSdk: Int?,
    val installedMs: Long?,
    val updatedMs: Long?,
)

/** A hardware sensor from SensorManager, with its latest value when sampled. */
@Serializable
data class HardwareSensor(
    val name: String,
    val vendor: String,
    val type: String,
    val unit: String?,
    val values: List<Float>?,
    val resolution: Float,
    val maxRange: Float,
    val powerMa: Float,
    val wakeUp: Boolean,
)

/** One point of on-device history, kept by the app in its own store. */
@Serializable
data class DeviceSample(
    val ts: Long,
    val cpuLoad: Float?,
    val cpuTempC: Float?,
    val gpuLoad: Float?,
    val memoryUsedPercent: Float,
    val batteryPercent: Float?,
    val batteryTempC: Float?,
    val batteryCurrentMa: Float?,
    val charging: Boolean?,
    val rxBytesPerSec: Long?,
    val txBytesPerSec: Long?,
)

/** An alert raised on this device, keyed like the desktop's: `title` and `cause` are i18n keys. */
@Serializable
data class DeviceAlert(
    val kind: String, // thermal | batteryHot | batteryLow | memoryPressure | storageLow | cpuSustained
    val severity: String, // info | warning | critical
    val sinceMs: Long,
    val value: Float?,
)
