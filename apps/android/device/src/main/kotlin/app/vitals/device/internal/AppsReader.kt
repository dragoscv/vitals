package app.vitals.device.internal

import android.app.usage.NetworkStats
import android.app.usage.NetworkStatsManager
import android.app.usage.StorageStatsManager
import android.app.usage.UsageEvents
import android.app.usage.UsageStatsManager
import android.content.Context
import android.content.pm.ApplicationInfo
import android.content.pm.PackageManager
import android.net.NetworkCapabilities
import android.os.Build
import android.os.Process
import android.os.storage.StorageManager
import app.vitals.device.model.AppUsage

/**
 * Per-app cost: screen time and launches from UsageStats, traffic from
 * NetworkStats, footprint from StorageStats. All three need usage access; the
 * list itself does not, so without it the rows are there and the numbers null.
 */
internal class AppsReader(private val context: Context) {
    private val pm = context.packageManager
    private val usage = context.getSystemService(UsageStatsManager::class.java)
    private val network = context.getSystemService(NetworkStatsManager::class.java)
    private val storage = context.getSystemService(StorageStatsManager::class.java)

    fun read(windowMs: Long, usageAccess: Boolean): List<AppUsage> {
        val end = System.currentTimeMillis()
        val start = end - windowMs
        val apps = launchable()
        val stats = if (usageAccess) usage.queryAndAggregateUsageStats(start, end) else emptyMap()
        val launches = if (usageAccess) launches(start, end) else emptyMap()
        val uids = apps.associate { it.uid to it.packageName }
        val wifi = if (usageAccess) traffic(NetworkCapabilities.TRANSPORT_WIFI, start, end, uids) else emptyMap()
        val mobile = if (usageAccess) traffic(NetworkCapabilities.TRANSPORT_CELLULAR, start, end, uids) else emptyMap()
        return apps.map { app ->
            val s = stats[app.packageName]
            val footprint = if (usageAccess) {
                runCatching { storage.queryStatsForPackage(StorageManager.UUID_DEFAULT, app.packageName, Process.myUserHandle()) }.getOrNull()
            } else {
                null
            }
            val pkg = runCatching { pm.getPackageInfo(app.packageName, 0) }.getOrNull()
            AppUsage(
                packageName = app.packageName,
                label = pm.getApplicationLabel(app).toString(),
                system = app.flags and ApplicationInfo.FLAG_SYSTEM != 0 && app.flags and ApplicationInfo.FLAG_UPDATED_SYSTEM_APP == 0,
                foregroundMs = if (usageAccess) s?.totalTimeInForeground ?: 0L else null,
                lastUsedMs = s?.lastTimeUsed?.takeIf { it > 0 },
                launches = if (usageAccess) launches[app.packageName] ?: 0 else null,
                mobileRxBytes = mobile[app.packageName]?.first ?: if (usageAccess) 0L else null,
                mobileTxBytes = mobile[app.packageName]?.second ?: if (usageAccess) 0L else null,
                wifiRxBytes = wifi[app.packageName]?.first ?: if (usageAccess) 0L else null,
                wifiTxBytes = wifi[app.packageName]?.second ?: if (usageAccess) 0L else null,
                appBytes = footprint?.appBytes,
                dataBytes = footprint?.dataBytes,
                cacheBytes = footprint?.cacheBytes,
                versionName = pkg?.versionName,
                targetSdk = app.targetSdkVersion,
                installedMs = pkg?.firstInstallTime,
                updatedMs = pkg?.lastUpdateTime,
            )
        }
    }

    /**
     * Apps with a launcher entry, plus anything that used the screen. Asking
     * for every installed package would list 400 system services the user
     * has never heard of.
     */
    private fun launchable(): List<ApplicationInfo> {
        val intent = android.content.Intent(android.content.Intent.ACTION_MAIN).addCategory(android.content.Intent.CATEGORY_LAUNCHER)
        val packages = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            pm.queryIntentActivities(intent, PackageManager.ResolveInfoFlags.of(0))
        } else {
            @Suppress("DEPRECATION") // The flags overload is API 33.
            pm.queryIntentActivities(intent, 0)
        }.map { it.activityInfo.packageName }.toHashSet()
        return packages.mapNotNull { runCatching { pm.getApplicationInfo(it, 0) }.getOrNull() }
    }

    private fun launches(start: Long, end: Long): Map<String, Int> {
        val out = HashMap<String, Int>()
        val events = usage.queryEvents(start, end) ?: return out
        val e = UsageEvents.Event()
        while (events.hasNextEvent()) {
            events.getNextEvent(e)
            if (e.eventType == UsageEvents.Event.ACTIVITY_RESUMED) out.merge(e.packageName, 1, Int::plus)
        }
        return out
    }

    /** rx, tx per package. Shared uids (system) are summed into the first package that holds them. */
    private fun traffic(transport: Int, start: Long, end: Long, uids: Map<Int, String>): Map<String, Pair<Long, Long>> {
        val out = HashMap<String, Pair<Long, Long>>()
        val summary = runCatching { network.querySummary(transport, null, start, end) }.getOrNull() ?: return out
        summary.use { s ->
            val bucket = NetworkStats.Bucket()
            while (s.hasNextBucket()) {
                s.getNextBucket(bucket)
                val pkg = uids[bucket.uid] ?: continue
                val prev = out[pkg] ?: (0L to 0L)
                out[pkg] = (prev.first + bucket.rxBytes) to (prev.second + bucket.txBytes)
            }
        }
        return out
    }
}
