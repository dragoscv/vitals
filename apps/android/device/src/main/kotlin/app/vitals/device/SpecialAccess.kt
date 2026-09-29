package app.vitals.device

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.provider.Settings

/**
 * Opens the system screen for one special access; each has its own intent
 * and none can be requested in-app. Lives beside the monitor that needs the
 * access so the phone and the TV open the same screens.
 */
object SpecialAccess {
    fun usage(context: Context) = open(
        context,
        Intent(Settings.ACTION_USAGE_ACCESS_SETTINGS, Uri.parse("package:${context.packageName}")),
        Intent(Settings.ACTION_USAGE_ACCESS_SETTINGS),
    )

    fun allFiles(context: Context) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) return appInfo(context, context.packageName)
        open(
            context,
            Intent(Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION, Uri.parse("package:${context.packageName}")),
            Intent(Settings.ACTION_MANAGE_ALL_FILES_ACCESS_PERMISSION),
        )
    }

    fun appInfo(context: Context, pkg: String) = open(
        context,
        Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:$pkg")),
        Intent(Settings.ACTION_MANAGE_APPLICATIONS_SETTINGS),
    )

    /**
     * Some builds lack the per-app form of an intent; the list form always
     * exists. Google TV is one: its Settings answers the usage-access list
     * but not `package:app.vitals` (Chromecast with Google TV, 2026-09-29).
     */
    private fun open(context: Context, first: Intent, fallback: Intent) {
        val flags = Intent.FLAG_ACTIVITY_NEW_TASK
        runCatching { context.startActivity(first.addFlags(flags)) }
            .onFailure { runCatching { context.startActivity(fallback.addFlags(flags)) } }
    }
}
