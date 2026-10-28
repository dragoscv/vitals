package app.vitals.core.health

import kotlinx.serialization.Serializable

// `GET {base}/api/v1/me/health` from vmui (docs/api/vmui-health.md; source of
// truth vmui `src/lib/health/me.ts`). vmui is a separate app on its own
// release cycle, so every reading is nullable with a default: a field vmui
// stops sending hides one line, never fails the answer. Decoded with
// `WireJson.Lenient`, which ignores fields this build does not know.
//
// A null is a reading nobody measured and renders as an em dash; it is never
// coerced to 0. The body is kept in memory only — never written to disk.

@Serializable
data class MeHealth(
    val at: Long? = null,
    val health: HealthReadings? = null,
    val nutrition: Nutrition? = null,
)

@Serializable
data class HealthReadings(
    val device: String? = null,
    val steps: Long? = null,
    val stepsAt: Long? = null,
    val distanceKm: Double? = null,
    val floors: Long? = null,
    val activeKcal: Double? = null,
    val totalKcal: Double? = null,
    val heartRate: Double? = null,
    val heartRateAt: Long? = null,
    val restingHr: Double? = null,
    val hrv: Double? = null,
    val spo2: Double? = null,
    val respiratoryRate: Double? = null,
    val sleepMin: Double? = null,
    val sleepAt: Long? = null,
    val weightKg: Double? = null,
    val weightAt: Long? = null,
    val bodyFat: Double? = null,
    // `impedance` is deliberately absent: often decorative, the contract says
    // not to show it, and an unread field is one less thing to drift.
    val body: BodyEstimate? = null,
    val bmr: Double? = null,
    val vo2max: Double? = null,
    val bloodPressure: BloodPressure? = null,
    val hrHistory: List<HrPoint> = emptyList(),
)

/** Estimates vmui derives from height and weight; every surface labels them "estimate". */
@Serializable
data class BodyEstimate(
    val bmi: Double? = null,
    val fatPct: Double? = null,
    val fatKg: Double? = null,
    val waterKg: Double? = null,
    val waterPct: Double? = null,
    val leanKg: Double? = null,
    val muscleKg: Double? = null,
    val bmrKcal: Double? = null,
    val boneKg: Double? = null,
    val ageYears: Double? = null,
    /** "sub" | "normal" | "over" | "obese"; kept as text so a new band cannot fail the answer. */
    val bmiBand: String? = null,
)

@Serializable
data class BloodPressure(val sys: Double? = null, val dia: Double? = null)

@Serializable
data class HrPoint(val t: Long? = null, val v: Double? = null)

@Serializable
data class Nutrition(
    val day: String? = null,
    val targets: NutritionTargets? = null,
    val today: NutritionDay? = null,
    val remaining: NutritionRemaining? = null,
    val balance: Double? = null,
    val activeKcal: Double? = null,
    val water: Water? = null,
    val week: List<NutritionDay> = emptyList(),
)

@Serializable
data class NutritionTargets(
    val weightKg: Double? = null,
    val weightSource: String? = null,
    val ageYears: Double? = null,
    val bmr: Double? = null,
    val tdee: Double? = null,
    val calories: Double? = null,
    val protein: Double? = null,
    val carbs: Double? = null,
    val fats: Double? = null,
    val fiber: Double? = null,
)

@Serializable
data class NutritionDay(
    val day: String? = null,
    val calories: Double? = null,
    val protein: Double? = null,
    val carbs: Double? = null,
    val fats: Double? = null,
    val fiber: Double? = null,
    val meals: Long? = null,
)

/** What is left for today; may be negative once a target is passed. */
@Serializable
data class NutritionRemaining(
    val calories: Double? = null,
    val protein: Double? = null,
    val carbs: Double? = null,
    val fats: Double? = null,
)

@Serializable
data class Water(
    val day: String? = null,
    val ml: Double? = null,
    val glasses: Long? = null,
    val targetMl: Double? = null,
    val remainingMl: Double? = null,
    val lastAt: Long? = null,
    val underPace: Boolean? = null,
)
