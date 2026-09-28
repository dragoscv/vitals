package app.vitals.phone.ui.theme

import android.app.ActivityManager
import android.content.Context
import android.os.Build

/**
 * Whether this phone can afford real blur behind the bars.
 *
 * Blur re-renders what is behind every glass surface on every frame. On a
 * Galaxy A51 (Android 13, 3.6 GB, media performance class 0) that costs the
 * frame budget the gauges need, so it gets tinted translucency instead; the
 * design reads the same either way (ADR-0033). Performance class is the
 * primary signal because manufacturers certify it; total memory catches
 * flagships that never declared a class.
 */
object DeviceTier {
    private const val EIGHT_GB = 8L * 1024 * 1024 * 1024

    fun supportsBlur(context: Context): Boolean {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S) return false
        val am = context.getSystemService(ActivityManager::class.java) ?: return false
        if (am.isLowRamDevice) return false
        if (Build.VERSION.MEDIA_PERFORMANCE_CLASS >= Build.VERSION_CODES.S) return true
        val info = ActivityManager.MemoryInfo().also(am::getMemoryInfo)
        // totalMem excludes memory the kernel reserves, so an 8 GB phone
        // reports a little under 8; allow for that.
        return info.totalMem >= EIGHT_GB * 9 / 10
    }
}
