package app.vitals.phone.service

import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.PowerManager
import android.util.Log
import androidx.core.content.ContextCompat
import androidx.lifecycle.LifecycleService
import androidx.lifecycle.lifecycleScope
import app.vitals.core.model.Alert
import app.vitals.core.net.ApiFailure
import app.vitals.core.net.ApiResult
import app.vitals.core.net.LanNetwork
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
    private var wakeLock: PowerManager.WakeLock? = null

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
        if (!goForeground(emptyList())) {
            stopSelf()
            return START_NOT_STICKY
        }
        holdWakeLock()
        // A loop that ended (nothing watched, then a toggle turned back on)
        // is not null but is finished; restarting only on null left the
        // service foreground with nothing polling.
        if (loop?.isActive != true) loop = lifecycleScope.launch { run() }
        // Not sticky: a system restart arrives with the app in the background,
        // where Android 12+ refuses startForeground and the process dies with
        // ForegroundServiceStartNotAllowedException. The user's toggle and the
        // app's own start are the only legitimate ways in.
        return START_NOT_STICKY
    }

    /**
     * Held for exactly as long as the service runs. Without it the platform
     * is free to suspend the process between the loop's delays and a
     * five-second cadence becomes minutes or nothing while the screen is off
     * — the reading the user turned watching on for silently stops.
     *
     * Released in [onDestroy], which every way out of here passes through
     * (the loop's own `stopSelf`, the Stop action, the system stopping the
     * service), so the lock can never outlive the notification that tells the
     * user it is being held. Reference-counted is off because one lock is
     * taken once per service, not once per `onStartCommand`.
     */
    // Lint asks for a timeout so a lock cannot outlive its use; this one is
    // bounded by the foreground service instead, which the user can see and
    // stop, and which releases it in onDestroy.
    @Suppress("WakelockTimeout")
    private fun holdWakeLock() {
        if (wakeLock?.isHeld == true) return
        val manager = getSystemService(PowerManager::class.java) ?: return
        wakeLock = try {
            manager.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "vitals:watch").apply {
                setReferenceCounted(false)
                acquire()
            }
        } catch (e: SecurityException) {
            Log.w(TAG, "wake lock refused", e)
            null
        }
    }

    override fun onDestroy() {
        // Before super, so the lock is gone even if the superclass throws.
        wakeLock?.let { if (it.isHeld) it.release() }
        wakeLock = null
        super.onDestroy()
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
            if (!LanNetwork.onLan) {
                // Polling over mobile data would send the token to whatever a
                // private address routes to there (AND-6). Say so in the
                // ongoing notification and resume the moment Wi-Fi is back.
                if (!goForeground(watched.map { Notifications.Watched(it.label, null) }, offLan = true)) {
                    stopSelf()
                    return
                }
                LanNetwork.local.first { it }
                continue
            }
            val results = coroutineScope { watched.map { p -> async { poll(p) } }.awaitAll() }
            if (!goForeground(results)) {
                stopSelf()
                return
            }
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
            PcState(
                pairingId = p.id,
                label = p.label,
                fetchedMs = System.currentTimeMillis(),
                summary = value,
                // This poll asks for the summary and the alerts only, so it
                // has nothing to say about the sensors: `null` keeps the last
                // list the watch was sent instead of blanking it every five
                // seconds (S24-09).
                sensors = null,
                alerts = list,
                canControl = p.canControl(),
                unauthorised = refused(summary, alerts),
            ),
            urgent = changed,
        )
        return Notifications.Watched(p.label, value)
    }

    /** Posts warnings and critical alerts that were not there last round; returns whether the set changed. */
    private suspend fun notifyNew(p: Pairing, alerts: List<Alert>): Boolean {
        val before = notified[p.id].orEmpty()
        val (fresh, ids) = AlertDedupe.fresh(before, alerts) { Notifications.alertId(p.id, it) }
        notified[p.id] = ids
        // The watch is offered the alert first, so the phone's card can be
        // decided from whether anyone took it: given away, this card must not
        // also be bridged (the wrist would show the alert twice); with no
        // watch app installed nothing takes it, and bridging is then the only
        // way the wearer sees it at all.
        var onWatch = 0
        fresh.forEach { (_, alert) -> onWatch += graph.wear.sendAlert(alert) }
        val nm = getSystemService(NotificationManager::class.java)
        if (nm.areNotificationsEnabled()) {
            fresh.forEach { (id, alert) ->
                nm.notify(id, Notifications.alert(this, p.label, alert, localOnly = onWatch > 0))
            }
        }
        return ids != before
    }

    /** A round in which the PC refused the token (401/403): not "waiting", and never retried blindly. */
    private fun refused(vararg results: ApiResult<*>): Boolean =
        results.any { (it as? ApiResult.Err)?.failure == ApiFailure.Unauthorised }

    /** False when the system refused foreground (background start, missing permission); the caller stops. */
    private fun goForeground(watched: List<Notifications.Watched>, offLan: Boolean = false): Boolean {
        val stop = PendingIntent.getService(
            this,
            1,
            Intent(this, WatchService::class.java).setAction(ACTION_STOP),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        val notification = Notifications.ongoing(this, watched, stop, offLan)
        return try {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
                startForeground(Notifications.ONGOING_ID, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE)
            } else {
                startForeground(Notifications.ONGOING_ID, notification)
            }
            true
        } catch (e: IllegalStateException) {
            // ForegroundServiceStartNotAllowedException (API 31) is one of these.
            Log.w(TAG, "foreground refused", e)
            false
        } catch (e: SecurityException) {
            Log.w(TAG, "foreground refused", e)
            false
        }
    }

    companion object {
        private const val POLL_MS = 5_000L
        private const val ACTION_STOP = "app.vitals.action.STOP_WATCHING"
        private const val TAG = "WatchService"

        /**
         * Safe to call from a coroutine that may finish after the app left the
         * foreground: Android 12+ then refuses the start with an
         * IllegalStateException, and the right outcome is "not started", not a
         * crash. The next visit to the app starts it again.
         */
        fun start(context: Context) {
            try {
                ContextCompat.startForegroundService(context, Intent(context, WatchService::class.java))
            } catch (e: IllegalStateException) {
                Log.w(TAG, "start refused", e)
            } catch (e: SecurityException) {
                Log.w(TAG, "start refused", e)
            }
        }
    }
}
