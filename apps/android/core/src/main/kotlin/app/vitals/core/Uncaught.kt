package app.vitals.core

import android.util.Log
import kotlinx.coroutines.CoroutineExceptionHandler

/**
 * The handler every long-lived scope carries (app graph, TV graph, watch
 * app, tiles). Without one, an exception escaping a `launch` in a
 * `SupervisorJob` scope goes to the thread's default handler and kills the
 * process — a monitoring app crashing because one refresh failed. Logged
 * instead, so the failure is still visible in logcat and crash reports.
 */
fun loggingHandler(tag: String): CoroutineExceptionHandler =
    CoroutineExceptionHandler { _, e -> Log.e(tag, "uncaught in coroutine", e) }
