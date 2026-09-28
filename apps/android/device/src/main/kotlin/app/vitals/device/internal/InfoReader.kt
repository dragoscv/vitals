package app.vitals.device.internal

import android.app.ActivityManager
import android.content.Context
import android.content.pm.PackageManager
import android.hardware.display.DisplayManager
import android.os.Build
import android.os.SystemClock
import android.view.Display
import app.vitals.device.model.DeviceInfo
import app.vitals.device.model.MemoryState

/** Static device facts and memory: ActivityManager is the authority, meminfo fills in what it omits. */
internal class InfoReader(private val context: Context, private val cpu: CpuSampler) {
    private val activity = context.getSystemService(ActivityManager::class.java)

    fun memory(): MemoryState {
        val info = ActivityManager.MemoryInfo()
        activity.getMemoryInfo(info)
        val proc = SysFs.read("/proc/meminfo")?.let(::parseMeminfo).orEmpty()
        val cached = listOfNotNull(proc["Cached"], proc["Buffers"]).takeIf { it.isNotEmpty() }?.sum()
        return MemoryState(
            totalBytes = info.totalMem,
            availableBytes = info.availMem,
            usedBytes = (info.totalMem - info.availMem).coerceAtLeast(0),
            cachedBytes = cached,
            swapTotalBytes = proc["SwapTotal"]?.takeIf { it > 0 },
            swapFreeBytes = proc["SwapTotal"]?.takeIf { it > 0 }?.let { proc["SwapFree"] },
            lowMemory = info.lowMemory,
            thresholdBytes = info.threshold.takeIf { it > 0 },
        )
    }

    fun info(): DeviceInfo {
        val display = context.getSystemService(DisplayManager::class.java).getDisplay(Display.DEFAULT_DISPLAY)
        val mode = display?.mode
        val metrics = context.resources.displayMetrics
        val memory = ActivityManager.MemoryInfo().also { activity.getMemoryInfo(it) }
        val pm = context.packageManager
        return DeviceInfo(
            manufacturer = Build.MANUFACTURER.replaceFirstChar { it.uppercase() },
            model = Build.MODEL,
            marketingName = marketingName(),
            device = Build.DEVICE,
            androidVersion = Build.VERSION.RELEASE,
            sdkInt = Build.VERSION.SDK_INT,
            securityPatch = Build.VERSION.SECURITY_PATCH.takeIf { it.isNotBlank() },
            kernel = System.getProperty("os.version"),
            soc = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) Build.SOC_MODEL.takeIf { it != Build.UNKNOWN } else null,
            socManufacturer = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                Build.SOC_MANUFACTURER.takeIf { it != Build.UNKNOWN }
            } else {
                null
            },
            abis = Build.SUPPORTED_ABIS.toList(),
            cpuCores = cpu.coreCount,
            totalMemoryBytes = memory.totalMem,
            displayWidthPx = mode?.physicalWidth ?: metrics.widthPixels,
            displayHeightPx = mode?.physicalHeight ?: metrics.heightPixels,
            displayDensityDpi = metrics.densityDpi,
            refreshRatesHz = display?.supportedModes?.map { it.refreshRate }?.distinct()?.sorted().orEmpty(),
            hdr = hdr(display),
            bootTimeMs = System.currentTimeMillis() - SystemClock.elapsedRealtime(),
            mediaPerformanceClass = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) Build.VERSION.MEDIA_PERFORMANCE_CLASS else 0,
            isLowRam = activity.isLowRamDevice,
            glEsVersion = activity.deviceConfigurationInfo?.glEsVersion,
            vulkan = pm.hasSystemFeature(PackageManager.FEATURE_VULKAN_HARDWARE_VERSION),
        )
    }

    @Suppress("DEPRECATION") // hdrCapabilities: the replacement needs API 34 and reports the same types.
    private fun hdr(display: Display?): List<String> = display?.hdrCapabilities?.supportedHdrTypes?.toList().orEmpty().mapNotNull {
        when (it) {
            Display.HdrCapabilities.HDR_TYPE_DOLBY_VISION -> "Dolby Vision"
            Display.HdrCapabilities.HDR_TYPE_HDR10 -> "HDR10"
            Display.HdrCapabilities.HDR_TYPE_HLG -> "HLG"
            Display.HdrCapabilities.HDR_TYPE_HDR10_PLUS -> "HDR10+"
            else -> null
        }
    }

    /** The name on the box, where the OEM publishes it as a global setting (Samsung, Google). */
    private fun marketingName(): String? = try {
        android.provider.Settings.Global.getString(context.contentResolver, "device_name")
            ?.takeIf { it.isNotBlank() && it != Build.MODEL }
    } catch (_: SecurityException) {
        null
    }
}
