package app.vitals.wear.data

import android.content.ComponentName
import android.content.Context
import android.os.SystemClock
import androidx.wear.tiles.TileService
import androidx.wear.watchface.complications.datasource.ComplicationDataSourceUpdateRequester
import app.vitals.wear.complication.CpuComplicationService
import app.vitals.wear.complication.TemperatureComplicationService
import app.vitals.wear.tile.VitalsTileService
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/**
 * Tells the tile and the complications that new data exists.
 *
 * Throttled with a trailing edge: while the app is open, state lands every
 * five seconds, and re-rendering a watch face that often costs battery for
 * nothing. The trailing update makes sure the last state of a burst is the
 * one that ends up on the face.
 */
class SurfaceUpdater(private val context: Context, private val scope: CoroutineScope) {
    private var lastMs = 0L
    private var pending: Job? = null

    @Synchronized
    fun request() {
        val now = SystemClock.elapsedRealtime()
        val wait = MIN_INTERVAL_MS - (now - lastMs)
        if (wait <= 0) {
            fire(now)
        } else if (pending?.isActive != true) {
            pending = scope.launch {
                delay(wait)
                fire(SystemClock.elapsedRealtime())
            }
        }
    }

    @Synchronized
    private fun fire(now: Long) {
        lastMs = now
        TileService.getUpdater(context).requestUpdate(VitalsTileService::class.java)
        for (cls in listOf(CpuComplicationService::class.java, TemperatureComplicationService::class.java)) {
            ComplicationDataSourceUpdateRequester.create(context, ComponentName(context, cls)).requestUpdateAll()
        }
    }

    private companion object {
        const val MIN_INTERVAL_MS = 30_000L
    }
}
