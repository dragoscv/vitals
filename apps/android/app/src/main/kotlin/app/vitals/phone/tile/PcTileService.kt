package app.vitals.phone.tile

import android.app.PendingIntent
import android.content.Intent
import android.graphics.drawable.Icon
import android.os.Build
import android.service.quicksettings.Tile
import android.service.quicksettings.TileService
import app.vitals.core.net.getOrNull
import app.vitals.phone.MainActivity
import app.vitals.phone.R
import app.vitals.phone.graph
import app.vitals.ui.Format
import app.vitals.ui.Readings
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch

/**
 * Quick Settings tile: the selected PC's CPU and heat.
 *
 * Fetched once each time the shade opens, and cancelled when it closes;
 * a tile that polled in the background would cost battery for a number
 * nobody is looking at.
 */
class PcTileService : TileService() {
    private var scope: CoroutineScope? = null

    override fun onStartListening() {
        super.onStartListening()
        val tile = qsTile ?: return
        val job = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
        scope = job
        job.launch { refresh(tile) }
    }

    override fun onStopListening() {
        scope?.cancel()
        scope = null
        super.onStopListening()
    }

    private suspend fun refresh(tile: Tile) {
        val graph = applicationContext.graph
        val pairings = graph.pairings.load()
        val selected = graph.settings.selected.first()
        val target = pairings.firstOrNull { it.id == selected } ?: pairings.firstOrNull()
        tile.icon = Icon.createWithResource(this, R.drawable.ic_stat_vitals)
        if (target == null) {
            show(tile, getString(R.string.tile_label), getString(R.string.tile_no_pc), Tile.STATE_INACTIVE)
            return
        }
        // Show the last known values at once, then the fresh ones.
        graph.widgetCache.all.first()[target.id]?.summary?.let { cached ->
            show(tile, target.label, subtitle(cached.system), Tile.STATE_ACTIVE)
        }
        val summary = graph.client(target).summary(top = 3).getOrNull()
        if (summary == null) {
            show(tile, target.label, getString(R.string.tile_unreachable), Tile.STATE_INACTIVE)
        } else {
            show(tile, target.label, subtitle(summary.system), Tile.STATE_ACTIVE)
            graph.widgetCache.put(target.id, target.label, summary)
        }
    }

    private fun subtitle(system: app.vitals.core.model.SystemMetrics): String =
        "${Format.percentCompact(system.cpu.total)} · ${Format.celsiusCompact(Readings.cpuTemperature(system))}"

    private fun show(tile: Tile, label: String, subtitle: String, state: Int) {
        tile.label = label
        tile.subtitle = subtitle
        tile.state = state
        tile.updateTile()
    }

    // Below API 34 the PendingIntent overload does not exist, and the Intent
    // overload is the only launch a tile is allowed from the shade; it is
    // deprecated only from 34, where the branch above is taken instead.
    @android.annotation.SuppressLint("StartActivityAndCollapseDeprecated")
    override fun onClick() {
        super.onClick()
        val intent = Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            startActivityAndCollapse(
                PendingIntent.getActivity(this, 0, intent, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT),
            )
        } else {
            @Suppress("DEPRECATION")
            startActivityAndCollapse(intent)
        }
    }
}
