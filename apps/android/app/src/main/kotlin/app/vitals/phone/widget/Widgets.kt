package app.vitals.phone.widget

import android.content.Context
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.glance.GlanceId
import androidx.glance.GlanceModifier
import androidx.glance.GlanceTheme
import androidx.glance.action.actionStartActivity
import androidx.glance.action.clickable
import androidx.glance.appwidget.GlanceAppWidget
import androidx.glance.appwidget.GlanceAppWidgetReceiver
import androidx.glance.appwidget.LinearProgressIndicator
import androidx.glance.appwidget.cornerRadius
import androidx.glance.appwidget.provideContent
import androidx.glance.background
import androidx.glance.layout.Alignment
import androidx.glance.layout.Column
import androidx.glance.layout.Row
import androidx.glance.layout.Spacer
import androidx.glance.layout.fillMaxSize
import androidx.glance.layout.fillMaxWidth
import androidx.glance.layout.height
import androidx.glance.layout.padding
import androidx.glance.layout.width
import androidx.glance.text.FontWeight
import androidx.glance.text.Text
import androidx.glance.text.TextStyle
import androidx.glance.unit.ColorProvider
import app.vitals.core.model.Summary
import app.vitals.phone.MainActivity
import app.vitals.phone.R
import app.vitals.phone.graph
import app.vitals.phone.ui.relativeTime
import app.vitals.ui.Format
import app.vitals.ui.Palette
import app.vitals.ui.Readings
import kotlinx.coroutines.flow.combine

/** What either widget renders: the chosen PC, or nothing paired yet. */
private data class WidgetModel(val label: String?, val cached: CachedPc?)

private suspend fun model(context: Context) = context.graph.let { graph ->
    graph.pairings.load()
    combine(graph.pairings.pairings, graph.settings.selected, graph.widgetCache.all) { pairings, selected, cache ->
        val target = pairings.firstOrNull { it.id == selected } ?: pairings.firstOrNull()
        WidgetModel(target?.label, target?.let { cache[it.id] })
    }
}

class SmallWidget : GlanceAppWidget() {
    override suspend fun provideGlance(context: Context, id: GlanceId) {
        val flow = model(context)
        provideContent {
            val m by flow.collectAsState(WidgetModel(null, null))
            GlanceTheme { Frame(m) { summary -> Small(summary) } }
        }
    }
}

class WideWidget : GlanceAppWidget() {
    override suspend fun provideGlance(context: Context, id: GlanceId) {
        val flow = model(context)
        provideContent {
            val m by flow.collectAsState(WidgetModel(null, null))
            GlanceTheme { Frame(m) { summary -> Wide(summary) } }
        }
    }
}

class SmallWidgetReceiver : GlanceAppWidgetReceiver() {
    override val glanceAppWidget: GlanceAppWidget = SmallWidget()
}

class WideWidgetReceiver : GlanceAppWidgetReceiver() {
    override val glanceAppWidget: GlanceAppWidget = WideWidget()
}

@Composable
private fun Frame(m: WidgetModel, body: @Composable (Summary?) -> Unit) {
    Column(
        GlanceModifier
            .fillMaxSize()
            .cornerRadius(24.dp)
            .background(GlanceTheme.colors.widgetBackground)
            .padding(12.dp)
            .clickable(actionStartActivity<MainActivity>()),
    ) {
        if (m.label == null) {
            Text(
                androidx.glance.LocalContext.current.getString(R.string.widget_no_pc),
                style = TextStyle(color = GlanceTheme.colors.onSurface),
            )
            return@Column
        }
        Text(
            m.label,
            style = TextStyle(color = GlanceTheme.colors.onSurface, fontWeight = FontWeight.Bold, fontSize = 14.sp),
            maxLines = 1,
        )
        m.cached?.let {
            Text(
                androidx.glance.LocalContext.current.getString(R.string.widget_stale, relativeTime(it.fetchedMs)),
                style = TextStyle(color = GlanceTheme.colors.onSurfaceVariant, fontSize = 11.sp),
                maxLines = 1,
            )
        }
        Spacer(GlanceModifier.height(6.dp))
        body(m.cached?.summary)
    }
}

@Composable
private fun Metric(label: String, value: String, fraction: Float?, colour: Color) {
    Column(GlanceModifier.fillMaxWidth().padding(vertical = 2.dp)) {
        Row(GlanceModifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            Text(label, style = TextStyle(color = GlanceTheme.colors.onSurfaceVariant, fontSize = 12.sp), modifier = GlanceModifier.defaultWeight())
            Text(value, style = TextStyle(color = GlanceTheme.colors.onSurface, fontSize = 13.sp, fontWeight = FontWeight.Medium))
        }
        // An unmeasured reading gets no bar at all rather than an empty one that reads as zero.
        if (fraction != null) {
            LinearProgressIndicator(
                progress = fraction.coerceIn(0f, 1f),
                modifier = GlanceModifier.fillMaxWidth().height(4.dp),
                color = ColorProvider(colour),
                backgroundColor = GlanceTheme.colors.surfaceVariant,
            )
        }
    }
}

@Composable
private fun Small(summary: Summary?) {
    val context = androidx.glance.LocalContext.current
    val system = summary?.system
    val memory = system?.let(Readings::memoryPercent)
    val temp = system?.let(Readings::cpuTemperature)
    Metric(context.getString(R.string.metric_cpu), Format.percent(system?.cpu?.total), system?.cpu?.total?.div(100f), Palette.Cpu)
    Metric(context.getString(R.string.metric_memory_short), Format.percent(memory), memory?.div(100f), Palette.Memory)
    Metric(context.getString(R.string.metric_temp), Format.celsius(temp), temp?.div(110f), Palette.Thermal)
}

@Composable
private fun Wide(summary: Summary?) {
    val context = androidx.glance.LocalContext.current
    val system = summary?.system
    val memory = system?.let(Readings::memoryPercent)
    val gpu = system?.let(Readings::primaryGpu)?.utilization
    val temp = system?.let(Readings::cpuTemperature)
    Row(GlanceModifier.fillMaxSize()) {
        Column(GlanceModifier.defaultWeight()) {
            Metric(context.getString(R.string.metric_cpu), Format.percent(system?.cpu?.total), system?.cpu?.total?.div(100f), Palette.Cpu)
            Metric(context.getString(R.string.metric_memory_short), Format.percent(memory), memory?.div(100f), Palette.Memory)
            Metric(context.getString(R.string.metric_gpu), Format.percent(gpu), gpu?.div(100f), Palette.Gpu)
            Metric(context.getString(R.string.metric_temp), Format.celsius(temp), temp?.div(110f), Palette.Thermal)
        }
        Spacer(GlanceModifier.width(12.dp))
        Column(GlanceModifier.defaultWeight()) {
            Text(
                context.getString(R.string.processes),
                style = TextStyle(color = GlanceTheme.colors.onSurfaceVariant, fontSize = 12.sp),
            )
            summary?.top?.take(3)?.forEach { p ->
                Row(GlanceModifier.fillMaxWidth().padding(vertical = 3.dp)) {
                    Text(
                        p.name,
                        style = TextStyle(color = GlanceTheme.colors.onSurface, fontSize = 12.sp),
                        maxLines = 1,
                        modifier = GlanceModifier.defaultWeight(),
                    )
                    Text(Format.percentCompact(p.cpu), style = TextStyle(color = GlanceTheme.colors.onSurface, fontSize = 12.sp))
                }
            } ?: Text(Format.DASH, style = TextStyle(color = GlanceTheme.colors.onSurface))
        }
    }
}
