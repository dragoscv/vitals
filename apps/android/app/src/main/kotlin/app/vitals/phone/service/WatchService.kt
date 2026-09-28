package app.vitals.phone.service

import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import androidx.core.content.ContextCompat
import androidx.lifecycle.LifecycleService
import androidx.lifecycle.lifecycleScope
import app.vitals.core.model.Alert
import app.vitals.core.model.Severity
import app.vitals.core.net.ApiResult
import app.vitals.core.net.getOrNull
import app.vitals.core.pairing.Pairing
import app.vitals.core.wear.PcState
import app.vitals.phone.data.canControl
import app.vitals.phone.graph
import kotlinx.coroutines.Job
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch

/**
 * "Watch this PC": the only thing in the app that runs in the background.
 *
 * Started solely by the user's toggle, and stops itself the moment no
 * pairing is watched, so turning the last toggle off is all it takes to
 * leave the phone alone. It polls `/summary` (3 KB) rather than holding the
 * stream (250 KB keyframes), because five-second freshness is what an
 * alert needs.
 */
class WatchService : LifecycleService() {
    private var loop: Job? = null

    /** What was already notified, so the same alert does not buzz every five seconds. */
    private val notified = HashMap<String, Set<Int>>()

    override fun onCreate() {
        super.onCreate()
        Notifications.ensureChannels(this)
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        super.onStartCommand(intent, flags, startId)
        if (intent?.action == ACTION_STOP) {
            lifecycleScope.launch {
                graph.pairings.load().forEach { graph.settings.setWatched(it.id, false) }
                stopSelf()
            }
            return START_NOT_STICKY
        }
        goForeground(emptyList())
        if (loop == null) loop = lifecycleScope.launch { run() }
        return START_STICKY
    }

    private suspend fun run() {
        val graph = graph
        while (true) {
            val (pairings, watchedIds) = combine(graph.pairings.pairings, graph.settings.watched) { p, w -> p to w }.first()
            val watched = pairings.filter { it.id in watchedIds }
            if (watched.isEmpty()) {
                stopSelf()
                return
            }
            val results = coroutineScope { watched.map { p -> async { poll(p) } }.awaitAll() }
            goForeground(results)
            delay(POLL_MS)
        }
    }

    private suspend fun poll(p: Pairing): Notifications.Watched {
        val graph = graph
        val client = graph.client(p)
        val (summary, alerts) = coroutineScope {
            val s = async { client.summary(top = 5) }
            val a = async { client.alerts() }
            s.await() to a.await()
        }
        val list = (alerts as? ApiResult.Ok)?.value.orEmpty()
        val changed = notifyNew(p, list)
        val value = summary.getOrNull()
        graph.wear.putState(
            PcState(p.id, p.label, System.currentTimeMillis(), value, emptyList(), list, p.canControl()),
            urgent = changed,
        )
        return Notifications.Watched(p.label, value)
    }

    /** Posts warnings and critical alerts that were not there last round; returns whether the set changed. */
    private suspend fun notifyNew(p: Pairing, alerts: List<Alert>): Boolean {
        val serious = alerts.filter { it.severity != Severity.Info }
        val ids = serious.associateBy { Notifications.alertId(p.id, it) }
        val before = notified[p.id].orEmpty()
        notified[p.id] = ids.keys
        val nm = getSystemService(NotificationManager::class.java)
        if (nm.areNotificationsEnabled()) {
            ids.filterKeys { it !in before }.forEach { (id, alert) ->
                nm.notify(id, Notifications.alert(this, p.label, alert))
            }
        }
        (ids.keys - before).forEach { id -> ids[id]?.let { graph.wear.sendAlert(it) } }
        return ids.keys != before
    }

    private fun goForeground(watched: List<Notifications.Watched>) {
        val stop = PendingIntent.getService(
            this,
            1,
            Intent(this, WatchService::class.java).setAction(ACTION_STOP),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        val notification = Notifications.ongoing(this, watched, stop)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            startForeground(Notifications.ONGOING_ID, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE)
        } else {
            startForeground(Notifications.ONGOING_ID, notification)
        }
    }

    companion object {
        private const val POLL_MS = 5_000L
        private const val ACTION_STOP = "app.vitals.action.STOP_WATCHING"

        fun start(context: Context) {
            ContextCompat.startForegroundService(context, Intent(context, WatchService::class.java))
        }
    }
}
