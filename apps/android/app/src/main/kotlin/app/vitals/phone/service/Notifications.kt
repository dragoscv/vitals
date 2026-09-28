package app.vitals.phone.service

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.os.Build
import androidx.compose.ui.graphics.toArgb
import app.vitals.core.model.Alert
import app.vitals.core.model.Severity
import app.vitals.core.model.Summary
import app.vitals.phone.MainActivity
import app.vitals.phone.R
import app.vitals.phone.ui.title
import app.vitals.ui.Format
import app.vitals.ui.Level
import app.vitals.ui.Palette
import app.vitals.ui.Readings
import app.vitals.ui.Thresholds
import kotlin.math.roundToInt

object Notifications {
    const val CHANNEL_ALERTS = "alerts"
    const val CHANNEL_WATCHING = "watching"
    const val ONGOING_ID = 1

    fun ensureChannels(context: Context) {
        val nm = context.getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(
            NotificationChannel(CHANNEL_ALERTS, context.getString(R.string.channel_alerts), NotificationManager.IMPORTANCE_HIGH)
                .apply {
                    description = context.getString(R.string.channel_alerts_body)
                    enableVibration(true)
                },
        )
        // Low importance: it is always there while watching and must never
        // make a sound; the alerts channel is what interrupts.
        nm.createNotificationChannel(
            NotificationChannel(CHANNEL_WATCHING, context.getString(R.string.channel_watching), NotificationManager.IMPORTANCE_LOW)
                .apply { description = context.getString(R.string.channel_watching_body) },
        )
    }

    private fun openApp(context: Context): PendingIntent = PendingIntent.getActivity(
        context,
        0,
        Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
    )

    /** The stable id of an alert, so the same condition updates one notification rather than stacking. */
    fun alertId(pairingId: String, alert: Alert): Int = "$pairingId|${alert.kind}|${alert.subject}".hashCode()

    fun alert(context: Context, pcLabel: String, alert: Alert): Notification {
        val colour = if (alert.severity == Severity.Critical) Palette.Danger else Palette.Warn
        return Notification.Builder(context, CHANNEL_ALERTS)
            .setSmallIcon(R.drawable.ic_stat_vitals)
            .setContentTitle(context.getString(alert.kind.title()))
            .setContentText(listOf(pcLabel, alert.subject).filter { it.isNotEmpty() }.joinToString(" · "))
            .setColor(colour.toArgb())
            .setCategory(Notification.CATEGORY_STATUS)
            .setContentIntent(openApp(context))
            .setAutoCancel(true)
            .build()
    }

    /** One watched PC's latest reading, or null when it could not be reached this round. */
    data class Watched(val label: String, val summary: Summary?)

    /** Negative so it can never collide with a PC alert's hash in practice-sized id spaces. */
    fun deviceAlertId(kind: String): Int = -("device|$kind".hashCode() and 0x3fffffff) - 1

    fun deviceAlert(context: Context, alert: app.vitals.device.model.DeviceAlert): Notification {
        val colour = if (alert.severity == "critical") Palette.Danger else Palette.Warn
        return Notification.Builder(context, CHANNEL_ALERTS)
            .setSmallIcon(R.drawable.ic_stat_vitals)
            .setContentTitle(context.getString(app.vitals.phone.ui.device.deviceAlertTitle(alert.kind)))
            .setContentText(context.getString(R.string.device_alert_on_this_phone))
            .setColor(colour.toArgb())
            .setCategory(Notification.CATEGORY_STATUS)
            .setContentIntent(openApp(context))
            .setAutoCancel(true)
            .build()
    }

    fun ongoing(context: Context, watched: List<Watched>, stop: PendingIntent): Notification {
        val lead = watched.firstOrNull { it.summary != null } ?: watched.firstOrNull()
        val system = lead?.summary?.system
        val cpu = system?.cpu?.total
        val temp = system?.let(Readings::cpuTemperature)
        val title = if (watched.size == 1) {
            context.getString(R.string.watching_title, watched[0].label)
        } else {
            context.resources.getQuantityString(R.plurals.watching_many, watched.size, watched.size)
        }
        val text = when {
            lead == null -> ""
            system == null -> context.getString(R.string.watching_unreachable, lead.label)
            else -> context.getString(
                R.string.watching_text,
                Format.percent(cpu),
                Format.percent(Readings.memoryPercent(system)),
                Format.celsius(temp),
            )
        }
        val builder = Notification.Builder(context, CHANNEL_WATCHING)
            .setSmallIcon(R.drawable.ic_stat_vitals)
            .setContentTitle(title)
            .setContentText(text)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setCategory(Notification.CATEGORY_PROGRESS)
            .setContentIntent(openApp(context))
            .addAction(Notification.Action.Builder(null, context.getString(R.string.stop_watching), stop).build())
        if (Build.VERSION.SDK_INT >= 36) liveUpdate(builder, cpu, temp)
        return builder.build()
    }

    /**
     * Android 16 Live Update (One UI's Now Bar): CPU load as the progress,
     * the track split into the same ok/warn/danger bands the app uses, and
     * the chip text the hottest thing worth glancing at.
     */
    @androidx.annotation.RequiresApi(36)
    private fun liveUpdate(builder: Notification.Builder, cpu: Float?, temp: Float?) {
        val style = Notification.ProgressStyle()
            .setStyledByProgress(true)
            .setProgressSegments(
                listOf(
                    Notification.ProgressStyle.Segment(85).setColor(Palette.of(Level.Ok).toArgb()),
                    Notification.ProgressStyle.Segment(10).setColor(Palette.of(Level.Warn).toArgb()),
                    Notification.ProgressStyle.Segment(5).setColor(Palette.of(Level.Danger).toArgb()),
                ),
            )
        if (cpu != null) style.setProgress(cpu.roundToInt().coerceIn(0, 100)) else style.setProgressIndeterminate(true)
        builder.setStyle(style)
        val chip = when {
            temp != null && Thresholds.cpuTemp(temp) != Level.Ok -> Format.celsiusCompact(temp)
            else -> Format.percentCompact(cpu)
        }
        builder.setShortCriticalText(chip)
        // The builder method arrived in 36.1; 36.0 reads the same request
        // from this extra, which is what the method writes.
        if (Build.VERSION.SDK_INT_FULL >= Build.VERSION_CODES_FULL.BAKLAVA_1) {
            builder.setRequestPromotedOngoing(true)
        } else {
            builder.addExtras(android.os.Bundle().apply { putBoolean("android.requestPromotedOngoing", true) })
        }
    }
}
