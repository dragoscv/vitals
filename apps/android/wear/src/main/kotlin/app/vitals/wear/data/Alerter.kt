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
import android.os.VibrationEffect
import android.os.Vibrator
import android.os.VibratorManager
import app.vitals.core.model.Alert
import app.vitals.core.model.Severity
import app.vitals.wear.MainActivity
import app.vitals.wear.R

/** A PC alert on the wrist: a notification and a vibration you can tell apart. */
object Alerter {
    private const val CHANNEL = "alerts"

    fun ensureChannel(context: Context) {
        val manager = context.getSystemService(NotificationManager::class.java) ?: return
        val channel = NotificationChannel(
            CHANNEL,
            context.getString(R.string.channel_alerts),
            NotificationManager.IMPORTANCE_HIGH,
        ).apply {
            description = context.getString(R.string.channel_alerts_description)
            // The channel's own buzz is the same for every app; ours encodes
            // the severity, so the channel stays silent and we vibrate.
            enableVibration(false)
        }
        manager.createNotificationChannel(channel)
    }

    fun notify(context: Context, alert: Alert, pcLabel: String?) {
        if (alert.severity == Severity.Info) return
        vibrate(context, alert.severity)
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
        val title = if (pcLabel != null) "$pcLabel · ${alert.title}" else alert.title
        val notification = Notification.Builder(context, CHANNEL)
            .setSmallIcon(R.drawable.ic_pulse)
            .setContentTitle(title)
            .setContentText(alert.cause)
            .setStyle(Notification.BigTextStyle().bigText(alert.cause))
            .setCategory(Notification.CATEGORY_STATUS)
            .setContentIntent(open)
            .setAutoCancel(true)
            .build()
        // Keyed by what is wrong and where, so a repeat of the same alert
        // replaces the old card instead of stacking ten identical ones.
        manager.notify("${pcLabel.orEmpty()}|${alert.kind}|${alert.subject}".hashCode(), notification)
    }

    private fun vibrate(context: Context, severity: Severity) {
        val vibrator = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            context.getSystemService(VibratorManager::class.java)?.defaultVibrator
        } else {
            context.getSystemService(Vibrator::class.java)
        } ?: return
        if (!vibrator.hasVibrator()) return
        val (timings, amplitudes) = when (severity) {
            Severity.Critical -> longArrayOf(0, 220, 120, 220, 120, 220) to intArrayOf(0, 255, 0, 255, 0, 255)
            else -> longArrayOf(0, 380) to intArrayOf(0, 170)
        }
        val effect = if (vibrator.hasAmplitudeControl()) {
            VibrationEffect.createWaveform(timings, amplitudes, -1)
        } else {
            VibrationEffect.createWaveform(timings, -1)
        }
        vibrator.vibrate(effect)
    }
}
