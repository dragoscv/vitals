package app.vitals.wear.data

import android.Manifest
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import app.vitals.core.model.Alert
import app.vitals.core.model.Severity
import app.vitals.wear.MainActivity
import app.vitals.wear.R
import app.vitals.wear.ui.alertText

/**
 * A PC alert on the wrist: a notification whose vibration says how bad it is.
 *
 * The buzz belongs to the notification channel, not to a direct
 * `Vibrator.vibrate` call (AND-21): the old direct call ignored Do Not
 * Disturb, bedtime mode and the user turning the channel's vibration off,
 * so a "warning" woke people at night. One channel per severity, each with
 * its own pattern, keeps the two felt differently while leaving the wearer
 * in control of both from the system settings.
 */
object Alerter {
    /** The old single channel, silent and superseded; deleted so it does not linger in settings. */
    private const val LEGACY_CHANNEL = "alerts"
    private const val CHANNEL_CRITICAL = "alerts-critical"
    private const val CHANNEL_WARNING = "alerts-warning"

    private val CRITICAL_PATTERN = longArrayOf(0, 220, 120, 220, 120, 220)
    private val WARNING_PATTERN = longArrayOf(0, 380)

    fun ensureChannel(context: Context) {
        val manager = context.getSystemService(NotificationManager::class.java) ?: return
        manager.deleteNotificationChannel(LEGACY_CHANNEL)
        manager.createNotificationChannel(
            channel(context, CHANNEL_CRITICAL, R.string.channel_alerts_critical, CRITICAL_PATTERN),
        )
        manager.createNotificationChannel(
            channel(context, CHANNEL_WARNING, R.string.channel_alerts_warning, WARNING_PATTERN),
        )
    }

    private fun channel(context: Context, id: String, name: Int, pattern: LongArray) =
        NotificationChannel(id, context.getString(name), NotificationManager.IMPORTANCE_HIGH).apply {
            description = context.getString(R.string.channel_alerts_description)
            enableVibration(true)
            vibrationPattern = pattern
        }

    fun notify(context: Context, alert: Alert, pcLabel: String?) {
        if (alert.severity == Severity.Info) return
        post(context, alert, pcLabel)
    }

    private fun post(context: Context, alert: Alert, pcLabel: String?) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            context.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) {
            return
        }
        val manager = context.getSystemService(NotificationManager::class.java) ?: return
        val open = PendingIntent.getActivity(
            context,
            0,
            Intent(context, MainActivity::class.java),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        val text = alertText(alert, context::getString)
        val title = if (pcLabel != null) "$pcLabel · ${text.title}" else text.title
        val channel = if (alert.severity == Severity.Critical) CHANNEL_CRITICAL else CHANNEL_WARNING
        val notification = Notification.Builder(context, channel)
            .setSmallIcon(R.drawable.ic_pulse)
            .setContentTitle(title)
            .setContentText(text.body)
            .setStyle(Notification.BigTextStyle().bigText(text.body))
            .setCategory(Notification.CATEGORY_STATUS)
            .setContentIntent(open)
            .setAutoCancel(true)
            .build()
        // Keyed by what is wrong and where, so a repeat of the same alert
        // replaces the old card instead of stacking ten identical ones.
        manager.notify("${pcLabel.orEmpty()}|${alert.kind}|${alert.subject}".hashCode(), notification)
    }
}
