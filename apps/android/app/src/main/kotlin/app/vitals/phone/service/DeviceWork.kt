package app.vitals.phone.service

import android.Manifest
import android.app.NotificationManager
import android.content.Context
import android.content.pm.PackageManager
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import app.vitals.phone.graph
import kotlinx.coroutines.flow.first
import java.util.concurrent.TimeUnit

/**
 * Records one history point for this phone and turns its alerts into
 * notifications. No network, no wake lock of its own: WorkManager batches it
 * with the system's other maintenance, which is the cheapest a periodic
 * sample can be on Android.
 */
class DeviceWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
    override suspend fun doWork(): Result {
        val graph = applicationContext.graph
        if (!graph.settings.deviceHistory.first()) return Result.success()
        graph.device.recordSample()
        graph.device.pruneHistory()
        notify(applicationContext)
        return Result.success()
    }

    private fun notify(context: Context) {
        if (context.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) return
        val nm = context.getSystemService(NotificationManager::class.java)
        Notifications.ensureChannels(context)
        val alerts = context.graph.device.alerts.value
        val previous = DeviceAlertMemory.load(context)
        for (a in alerts) {
            // Only a new or worsened alert interrupts; one that persists across
            // runs stays as the notification already shown.
            if (previous[a.kind] == a.severity) continue
            nm.notify(Notifications.deviceAlertId(a.kind), Notifications.deviceAlert(context, a))
        }
        for (kind in previous.keys - alerts.map { it.kind }.toSet()) nm.cancel(Notifications.deviceAlertId(kind))
        DeviceAlertMemory.save(context, alerts.associate { it.kind to it.severity })
    }
}

/** What was last notified, so the next run can tell a new alert from one still standing. */
private object DeviceAlertMemory {
    private const val FILE = "device-alerts"

    fun load(context: Context): Map<String, String> =
        context.getSharedPreferences(FILE, Context.MODE_PRIVATE).all.mapNotNull { (k, v) -> (v as? String)?.let { k to it } }.toMap()

    fun save(context: Context, alerts: Map<String, String>) {
        context.getSharedPreferences(FILE, Context.MODE_PRIVATE).edit().apply {
            clear()
            alerts.forEach { (k, v) -> putString(k, v) }
            apply()
        }
    }
}

object DeviceWork {
    private const val PERIODIC = "device-history"

    fun schedule(context: Context) {
        val request = PeriodicWorkRequestBuilder<DeviceWorker>(15, TimeUnit.MINUTES).build()
        WorkManager.getInstance(context).enqueueUniquePeriodicWork(PERIODIC, ExistingPeriodicWorkPolicy.KEEP, request)
    }

    fun cancel(context: Context) {
        WorkManager.getInstance(context).cancelUniqueWork(PERIODIC)
    }
}
