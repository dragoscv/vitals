package app.vitals.phone.ui.health

import android.text.format.DateUtils
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.repeatOnLifecycle
import app.vitals.core.health.HealthClient
import app.vitals.core.health.HealthFailure
import app.vitals.core.health.HealthPoll
import app.vitals.core.health.HealthReadings
import app.vitals.core.health.HrPoint
import app.vitals.core.health.MeHealth
import app.vitals.core.health.Nutrition
import app.vitals.phone.R
import app.vitals.phone.graph
import app.vitals.phone.ui.LocalNavigator
import app.vitals.phone.ui.components.GlassScaffold
import app.vitals.phone.ui.components.InfoRow
import app.vitals.phone.ui.components.SectionCard
import app.vitals.phone.ui.components.Tab
import app.vitals.phone.ui.nav.Machines
import app.vitals.phone.ui.nav.Settings
import app.vitals.ui.Format
import app.vitals.ui.Palette
import kotlinx.coroutines.delay
import java.text.NumberFormat

/**
 * The owner's health, read from vmui (docs/api/vmui-health.md).
 *
 * The answer lives in this composition only — `remember`, not
 * `rememberSaveable`, and nothing written to disk, a cache or a log — so
 * leaving the page or a process death drops it. Polling runs once a minute
 * while the page is on screen ([HealthPoll]) and not at all otherwise.
 */
@Composable
fun HealthScreen() {
    val graph = LocalContext.current.graph
    val navigator = LocalNavigator.current
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    val credentials by graph.health.credentials.collectAsStateWithLifecycle()
    var data by remember { mutableStateOf<MeHealth?>(null) }
    var failure by remember { mutableStateOf<HealthFailure?>(null) }
    var updatedAt by remember { mutableStateOf<Long?>(null) }

    // Disconnected in Settings (or never connected, after a restore of the
    // back stack): the tab is gone, so the page goes too.
    LaunchedEffect(credentials) {
        if (credentials == null && graph.health.load() == null) navigator.root(Machines)
    }

    LaunchedEffect(credentials, lifecycle) {
        val creds = credentials ?: return@LaunchedEffect
        lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) {
            val client = HealthClient.forBase(creds.base)
            if (client == null) {
                failure = HealthFailure.Invalid
                return@repeatOnLifecycle
            }
            var misses = 0
            while (true) {
                val next = HealthPoll.after(client.fetch(creds.token), misses)
                when (next) {
                    is HealthPoll.Show -> {
                        data = next.health
                        failure = null
                        misses = 0
                        updatedAt = System.currentTimeMillis()
                    }
                    is HealthPoll.Stale -> {
                        failure = next.failure
                        if (next.failure is HealthFailure.Unreachable) misses++
                    }
                    is HealthPoll.Stop -> {
                        failure = next.failure
                        // A revoked token or a foreign account shows nothing
                        // of the previous answer.
                        data = null
                        return@repeatOnLifecycle
                    }
                }
                delay(next.delayMs)
            }
        }
    }

    GlassScaffold(title = stringResource(R.string.health_title), tab = Tab.Health) { padding ->
        LazyColumn(
            contentPadding = PaddingValues(
                start = 16.dp,
                end = 16.dp,
                top = padding.calculateTopPadding() + 8.dp,
                bottom = padding.calculateBottomPadding() + 24.dp,
            ),
            verticalArrangement = Arrangement.spacedBy(12.dp),
            modifier = Modifier.fillMaxSize(),
        ) {
            item(key = "disclaimer") {
                Text(
                    stringResource(R.string.health_not_medical),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            item(key = "status") { StatusCard(failure, hasData = data != null, updatedAt = updatedAt) { navigator.root(Settings) } }
            val current = data
            if (current != null) {
                val h = current.health
                if (h == null) {
                    item(key = "none") {
                        SectionCard(stringResource(R.string.health_activity), accent = Palette.Ok) {
                            Text(stringResource(R.string.health_no_readings))
                        }
                    }
                } else {
                    item(key = "activity") { ActivityCard(h) }
                    item(key = "heart") { HeartCard(h) }
                    item(key = "sleep") { SleepCard(h) }
                    item(key = "body") { BodyCard(h) }
                }
                item(key = "nutrition") { NutritionCard(current.nutrition) }
            }
        }
    }
}

@Composable
private fun StatusCard(failure: HealthFailure?, hasData: Boolean, updatedAt: Long?, openSettings: () -> Unit) {
    val context = LocalContext.current
    when (failure) {
        null -> {
            val line = if (updatedAt == null) {
                stringResource(R.string.health_loading)
            } else {
                stringResource(
                    R.string.health_updated,
                    DateUtils.formatDateTime(context, updatedAt, DateUtils.FORMAT_SHOW_TIME),
                )
            }
            Text(line, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        HealthFailure.SignedOut -> SectionCard(stringResource(R.string.health_title), accent = Palette.Danger) {
            Text(stringResource(R.string.health_signed_out))
            TextButton(onClick = openSettings) { Text(stringResource(R.string.health_pair_again)) }
        }
        HealthFailure.NotOwner -> SectionCard(stringResource(R.string.health_title), accent = Palette.Danger) {
            Text(stringResource(R.string.health_not_owner))
            TextButton(onClick = openSettings) { Text(stringResource(R.string.health_pair_again)) }
        }
        is HealthFailure.Unreachable, HealthFailure.Invalid -> SectionCard(
            stringResource(R.string.health_unreachable),
            accent = Palette.Warn,
        ) {
            Text(
                stringResource(
                    when {
                        failure == HealthFailure.Invalid -> R.string.health_invalid
                        hasData -> R.string.health_stale
                        else -> R.string.health_unreachable_body
                    },
                ),
            )
            if (updatedAt != null) {
                Text(
                    stringResource(
                        R.string.health_updated,
                        DateUtils.formatDateTime(context, updatedAt, DateUtils.FORMAT_SHOW_TIME),
                    ),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
    }
}

@Composable
private fun ActivityCard(h: HealthReadings) {
    SectionCard(stringResource(R.string.health_activity), accent = Palette.Ok) {
        InfoRow(stringResource(R.string.health_steps), num(h.steps?.toDouble()))
        InfoRow(stringResource(R.string.health_distance), num(h.distanceKm, 1, "km"))
        InfoRow(stringResource(R.string.health_floors), num(h.floors?.toDouble()))
        InfoRow(stringResource(R.string.health_active_kcal), num(h.activeKcal, unit = "kcal"))
        InfoRow(stringResource(R.string.health_total_kcal), num(h.totalKcal, unit = "kcal"))
    }
}

@Composable
private fun HeartCard(h: HealthReadings) {
    SectionCard(stringResource(R.string.health_heart), accent = Palette.Danger) {
        InfoRow(stringResource(R.string.health_heart_rate), num(h.heartRate, unit = "bpm"))
        val points = h.hrHistory.mapNotNull { p -> p.v?.takeIf { it.isFinite() }?.let { p } }
        if (points.size >= 2) {
            HrSparkline(
                points,
                Palette.Danger,
                description = stringResource(R.string.health_hr_chart, num(h.heartRate, unit = "bpm")),
            )
        }
        InfoRow(stringResource(R.string.health_resting_hr), num(h.restingHr, unit = "bpm"))
        InfoRow(stringResource(R.string.health_hrv), num(h.hrv, unit = "ms"))
        InfoRow(stringResource(R.string.health_spo2), num(h.spo2, unit = "%"))
        InfoRow(stringResource(R.string.health_respiratory), num(h.respiratoryRate, unit = "/min"))
        val bp = h.bloodPressure
        InfoRow(
            stringResource(R.string.health_blood_pressure),
            if (bp?.sys != null && bp.dia != null) "${num(bp.sys)}/${num(bp.dia)} mmHg" else Format.DASH,
        )
        InfoRow(stringResource(R.string.health_vo2max), num(h.vo2max, unit = "mL/kg/min"))
    }
}

@Composable
private fun SleepCard(h: HealthReadings) {
    SectionCard(stringResource(R.string.health_sleep), accent = Palette.Network) {
        val minutes = h.sleepMin?.takeIf { it.isFinite() && it >= 0 }
        InfoRow(
            stringResource(R.string.health_sleep_last),
            minutes?.let { Format.duration((it * 60).toLong()) } ?: Format.DASH,
        )
    }
}

@Composable
private fun BodyCard(h: HealthReadings) {
    SectionCard(stringResource(R.string.health_body), accent = Palette.Power) {
        InfoRow(stringResource(R.string.health_weight), num(h.weightKg, 1, "kg"))
        // Measured readings and vmui's estimates stay on separate lines, so an
        // estimate is never shown under a measured label.
        InfoRow(stringResource(R.string.health_body_fat), num(h.bodyFat, 1, "%"))
        InfoRow(stringResource(R.string.health_bmr), num(h.bmr, unit = "kcal"))
        val band = when (h.body?.bmiBand) {
            "sub" -> stringResource(R.string.health_band_sub)
            "normal" -> stringResource(R.string.health_band_normal)
            "over" -> stringResource(R.string.health_band_over)
            "obese" -> stringResource(R.string.health_band_obese)
            else -> null
        }
        val bmi = num(h.body?.bmi, 1)
        InfoRow(stringResource(R.string.health_bmi), if (band != null && bmi != Format.DASH) "$bmi · $band" else bmi)
        InfoRow(stringResource(R.string.health_muscle), num(h.body?.muscleKg, 1, "kg"))
        InfoRow(stringResource(R.string.health_water_body), num(h.body?.waterPct, 1, "%"))
        InfoRow(stringResource(R.string.health_fat_estimate), num(h.body?.fatPct, 1, "%"))
    }
}

@Composable
private fun NutritionCard(n: Nutrition?) {
    SectionCard(stringResource(R.string.health_nutrition), accent = Palette.Disk) {
        if (n == null) {
            Text(stringResource(R.string.health_no_nutrition))
            return@SectionCard
        }
        val t = n.targets
        val d = n.today
        InfoRow(stringResource(R.string.health_calories), ofTarget(d?.calories, t?.calories, "kcal"))
        InfoRow(stringResource(R.string.health_protein), ofTarget(d?.protein, t?.protein, "g"))
        InfoRow(stringResource(R.string.health_carbs), ofTarget(d?.carbs, t?.carbs, "g"))
        InfoRow(stringResource(R.string.health_fats), ofTarget(d?.fats, t?.fats, "g"))
        InfoRow(stringResource(R.string.health_fibre), ofTarget(d?.fiber, t?.fiber, "g"))
        InfoRow(stringResource(R.string.health_meals), num(d?.meals?.toDouble()))
        InfoRow(stringResource(R.string.health_balance), signed(n.balance, "kcal"))
        InfoRow(stringResource(R.string.health_water), ofTarget(n.water?.ml, n.water?.targetMl, "mL"))
    }
}

@Composable
private fun ofTarget(value: Double?, target: Double?, unit: String): String {
    val v = num(value, unit = unit)
    if (v == Format.DASH || target == null || !target.isFinite()) return v
    return stringResource(R.string.health_of_target, v, num(target, unit = unit))
}

/** A locale-formatted number, or the em dash when vmui did not measure it. Never 0 for a missing reading. */
private fun num(v: Double?, digits: Int = 0, unit: String? = null): String {
    val x = v?.takeIf { it.isFinite() } ?: return Format.DASH
    val f = NumberFormat.getNumberInstance().apply {
        maximumFractionDigits = digits
        minimumFractionDigits = 0
    }
    return if (unit == null) f.format(x) else "${f.format(x)} $unit"
}

private fun signed(v: Double?, unit: String): String {
    val s = num(v, unit = unit)
    return if (v != null && v > 0 && s != Format.DASH) "+$s" else s
}

/**
 * The last 24 hours of heart rate. Scaled to its own range, not to zero:
 * a resting heart between 55 and 75 would be a flat line on a 0-based axis.
 */
@Composable
private fun HrSparkline(points: List<HrPoint>, colour: Color, description: String) {
    val path = remember { Path() }
    Canvas(
        Modifier
            .fillMaxWidth()
            .height(48.dp)
            .semantics { contentDescription = description },
    ) {
        val values = points.mapNotNull { it.v }
        val lo = values.min()
        val hi = values.max()
        val span = (hi - lo).takeIf { it > 0 } ?: 1.0
        val stroke = 2.dp.toPx()
        path.reset()
        values.forEachIndexed { i, v ->
            val x = size.width * i / (values.size - 1)
            val y = stroke + (size.height - 2 * stroke) * (1 - ((v - lo) / span)).toFloat()
            if (i == 0) path.moveTo(x, y) else path.lineTo(x, y)
        }
        drawPath(path, colour, style = Stroke(width = stroke, cap = StrokeCap.Round))
        val last = values.last()
        drawCircle(
            colour,
            radius = stroke * 1.5f,
            center = Offset(size.width, stroke + (size.height - 2 * stroke) * (1 - ((last - lo) / span)).toFloat()),
        )
    }
}
