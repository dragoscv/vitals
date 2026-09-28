package app.vitals.wear.tile

import android.content.ComponentName
import android.content.Context
import androidx.wear.protolayout.ActionBuilders
import androidx.wear.protolayout.DimensionBuilders
import androidx.wear.protolayout.LayoutElementBuilders
import androidx.wear.protolayout.ModifiersBuilders
import androidx.wear.protolayout.ResourceBuilders
import androidx.wear.protolayout.TimelineBuilders
import androidx.wear.protolayout.material3.MaterialScope
import androidx.wear.protolayout.material3.ProgressIndicatorColors
import androidx.wear.protolayout.material3.Typography
import androidx.wear.protolayout.material3.buttonGroup
import androidx.wear.protolayout.material3.circularProgressIndicator
import androidx.wear.protolayout.material3.materialScope
import androidx.wear.protolayout.material3.primaryLayout
import androidx.wear.protolayout.material3.text
import androidx.wear.protolayout.material3.textEdgeButton
import androidx.compose.ui.graphics.toArgb
import androidx.wear.protolayout.TypeBuilders
import androidx.wear.protolayout.modifiers.clickable
import androidx.wear.protolayout.types.LayoutColor
import androidx.wear.protolayout.types.LayoutString
import androidx.wear.tiles.RequestBuilders
import androidx.wear.tiles.TileBuilders
import androidx.wear.tiles.TileService
import app.vitals.ui.Format
import app.vitals.ui.Level
import app.vitals.ui.Palette
import app.vitals.ui.Thresholds
import app.vitals.wear.MainActivity
import app.vitals.wear.R
import app.vitals.wear.VitalsWearApp
import app.vitals.wear.data.Headline
import com.google.common.util.concurrent.ListenableFuture
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.guava.future

/**
 * One PC at a glance: CPU, memory and CPU temperature as three rings.
 *
 * The tile draws what the watch already has, then asks for a refresh so the
 * next render is current. Waiting for the network inside the request would
 * hold the carousel on a blank tile for up to three seconds.
 */
class VitalsTileService : TileService() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)

    override fun onTileRequest(requestParams: RequestBuilders.TileRequest): ListenableFuture<TileBuilders.Tile> =
        scope.future {
            val repository = VitalsWearApp.from(this@VitalsTileService).repository
            repository.load()
            val state = Headline.primary(repository.states.value.values)
            repository.refreshInBackground()
            val layout = materialScope(this@VitalsTileService, requestParams.deviceConfiguration) {
                layout(state?.let(Headline::of))
            }
            TileBuilders.Tile.Builder()
                .setResourcesVersion(RESOURCES_VERSION)
                .setFreshnessIntervalMillis(FRESHNESS_MS)
                .setTileTimeline(TimelineBuilders.Timeline.fromLayoutElement(layout))
                .build()
        }

    override fun onTileResourcesRequest(
        requestParams: RequestBuilders.ResourcesRequest,
    ): ListenableFuture<ResourceBuilders.Resources> = scope.future {
        ResourceBuilders.Resources.Builder()
            .setVersion(RESOURCES_VERSION)
            .addIdToImageMapping(
                ID_PULSE,
                ResourceBuilders.ImageResource.Builder()
                    .setAndroidResourceByResId(
                        ResourceBuilders.AndroidImageResourceByResId.Builder().setResourceId(R.drawable.ic_pulse).build(),
                    )
                    .build(),
            )
            .build()
    }

    override fun onDestroy() {
        scope.cancel()
        super.onDestroy()
    }

    private fun MaterialScope.layout(h: Headline?): LayoutElementBuilders.LayoutElement {
        val open = openApp(this@VitalsTileService)
        return primaryLayout(
            titleSlot = { text((h?.label ?: getString(R.string.app_name)).let(::LayoutString), maxLines = 1) },
            mainSlot = {
                if (h == null) {
                    text(
                        getString(R.string.tile_empty).let(::LayoutString),
                        typography = Typography.BODY_MEDIUM,
                        maxLines = 3,
                    )
                } else {
                    buttonGroup {
                        buttonGroupItem { ring(getString(R.string.cpu_short), h.cpu, Format.percentCompact(h.cpu), Palette.Cpu.toArgb()) }
                        buttonGroupItem { ring(getString(R.string.ram_short), h.memory, Format.percentCompact(h.memory), Palette.Memory.toArgb()) }
                        buttonGroupItem {
                            val level = Thresholds.cpuTemp(h.cpuTemp)
                            val color = if (level == Level.Unknown) Palette.Thermal else Palette.of(level)
                            ring(
                                getString(R.string.temp_short),
                                h.cpuTemp?.div(TEMP_MAX)?.times(100f),
                                Format.celsiusCompact(h.cpuTemp),
                                color.toArgb(),
                            )
                        }
                    }
                }
            },
            bottomSlot = {
                textEdgeButton(onClick = open) { text(getString(R.string.open).let(::LayoutString)) }
            },
        )
    }

    /**
     * A ring with its value inside. An unmeasured value is an empty track and
     * an em dash — the tile must not draw a full ring's worth of nothing.
     */
    private fun MaterialScope.ring(name: String, percent: Float?, value: String, argb: Int): LayoutElementBuilders.LayoutElement {
        val color = LayoutColor(argb)
        val box = LayoutElementBuilders.Box.Builder()
            .setWidth(DimensionBuilders.expand())
            .setHeight(DimensionBuilders.expand())
            .setModifiers(
                ModifiersBuilders.Modifiers.Builder()
                    .setSemantics(
                        ModifiersBuilders.Semantics.Builder()
                            .setContentDescription(TypeBuilders.StringProp.Builder("$name $value").build())
                            .build(),
                    )
                    .build(),
            )
            .addContent(
                circularProgressIndicator(
                    staticProgress = ((percent ?: 0f) / 100f).coerceIn(0f, 1f),
                    colors = ProgressIndicatorColors(
                        indicatorColor = if (percent == null) LayoutColor(0x00000000) else color,
                        trackColor = LayoutColor((argb and 0x00FFFFFF) or 0x40000000),
                    ),
                ),
            )
            .addContent(
                LayoutElementBuilders.Column.Builder()
                    .setHorizontalAlignment(LayoutElementBuilders.HORIZONTAL_ALIGN_CENTER)
                    .addContent(text(value.let(::LayoutString), typography = Typography.LABEL_MEDIUM, maxLines = 1))
                    .addContent(
                        text(
                            name.let(::LayoutString),
                            typography = Typography.LABEL_SMALL,
                            color = colorScheme.onSurfaceVariant,
                            maxLines = 1,
                        ),
                    )
                    .build(),
            )
        return box.build()
    }

    private fun MaterialScope.getString(id: Int): String = context.getString(id)

    companion object {
        private const val RESOURCES_VERSION = "1"
        private const val ID_PULSE = "pulse"
        private const val FRESHNESS_MS = 60_000L

        /** The temperature ring's full scale, matching the complication's range. */
        private const val TEMP_MAX = 110f

        private fun openApp(context: Context): ModifiersBuilders.Clickable = clickable(
            ActionBuilders.launchAction(ComponentName(context, MainActivity::class.java)),
            "open",
        )
    }
}
