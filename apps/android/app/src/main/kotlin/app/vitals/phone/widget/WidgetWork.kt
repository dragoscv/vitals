package app.vitals.phone.widget

import android.content.Context
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.ExistingWorkPolicy
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import app.vitals.core.net.getOrNull
import app.vitals.phone.graph
import kotlinx.coroutines.flow.first
import java.util.concurrent.TimeUnit

/** Refreshes the widget and tile cache. Fifteen minutes is WorkManager's floor, and plenty for a glance. */
class WidgetWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
    override suspend fun doWork(): Result {
        val graph = applicationContext.graph
        val pairings = graph.pairings.load()
        graph.widgetCache.retain(pairings.map { it.id }.toSet())
        val selected = graph.settings.selected.first()
        val target = pairings.firstOrNull { it.id == selected } ?: pairings.firstOrNull() ?: return Result.success()
        val summary = graph.client(target).summary(top = 3).getOrNull()
        graph.widgetCache.put(target.id, target.label, summary)
        return Result.success()
    }
}

object WidgetWork {
    private const val PERIODIC = "widgets-periodic"
    private const val NOW = "widgets-now"

    private val network = Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build()

    fun schedule(context: Context) {
        val request = PeriodicWorkRequestBuilder<WidgetWorker>(15, TimeUnit.MINUTES)
            .setConstraints(network)
            .build()
        WorkManager.getInstance(context)
            .enqueueUniquePeriodicWork(PERIODIC, ExistingPeriodicWorkPolicy.KEEP, request)
    }

    fun refreshNow(context: Context) {
        val request = OneTimeWorkRequestBuilder<WidgetWorker>().setConstraints(network).build()
        WorkManager.getInstance(context).enqueueUniqueWork(NOW, ExistingWorkPolicy.REPLACE, request)
    }
}
