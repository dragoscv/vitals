// The owner's health from vmui, in cards. Every missing reading is an em
// dash, never 0; body values are estimates; not medical advice.

import Charts
import SwiftUI
import VitalsCore

struct HealthView: View {
    @Environment(HealthStore.self) private var store
    let onPairAgain: () -> Void

    var body: some View {
        List {
            Section {
                Text(L("health_not_medical")).font(.footnote).foregroundStyle(.secondary)
                if let at = store.latest?.at, at.isFinite {
                    Text(L("health_updated", Self.time(at))).font(.footnote).foregroundStyle(.secondary)
                }
            }
            if let issue = store.issue { IssueSection(issue: issue, stale: store.latest != nil, onPairAgain: onPairAgain) }
            if let me = store.latest {
                if let h = me.health {
                    ActivityCard(h: h)
                    HeartCard(h: h)
                    SleepCard(h: h)
                    BodyCard(h: h)
                } else {
                    Section { Text(L("health_no_readings")).foregroundStyle(.secondary) }
                }
                if let n = me.nutrition {
                    NutritionCard(n: n)
                } else {
                    Section(L("health_nutrition")) { Text(L("health_no_nutrition")).foregroundStyle(.secondary) }
                }
            } else if store.issue == nil {
                Section { ProgressView(L("health_loading")) }
            }
        }
        .navigationTitle(L("health_title"))
        .refreshable { await store.refresh() }
    }

    static func time(_ ms: Double) -> String {
        Date(timeIntervalSince1970: ms / 1000).formatted(date: .omitted, time: .shortened)
    }
}

private struct IssueSection: View {
    let issue: HealthIssue
    let stale: Bool
    let onPairAgain: () -> Void

    var body: some View {
        Section {
            switch issue {
            case .signedOut:
                Label(L("health_signed_out"), systemImage: "key.slash").foregroundStyle(Palette.warn)
                Button(L("health_pair_again"), action: onPairAgain)
            case .notOwner:
                Label(L("health_not_owner"), systemImage: "person.crop.circle.badge.xmark")
                    .foregroundStyle(Palette.danger)
            case .unreachable:
                Label(L("health_unreachable"), systemImage: "wifi.exclamationmark").foregroundStyle(Palette.warn)
                Text(stale ? L("health_stale") : L("health_unreachable_body"))
                    .font(.footnote).foregroundStyle(.secondary)
            case .invalid:
                Label(L("health_invalid"), systemImage: "exclamationmark.triangle").foregroundStyle(Palette.warn)
            }
        }
    }
}

private struct ActivityCard: View {
    let h: PhoneHealth
    var body: some View {
        Section(L("health_activity")) {
            InfoRow(L("health_steps"), HealthFormat.value(h.steps, ""))
            InfoRow(L("health_distance"), HealthFormat.value(h.distanceKm, "km", digits: 1))
            InfoRow(L("health_floors"), HealthFormat.value(h.floors, ""))
            InfoRow(L("health_active_kcal"), HealthFormat.value(h.activeKcal, "kcal"))
            InfoRow(L("health_total_kcal"), HealthFormat.value(h.totalKcal, "kcal"))
        }
    }
}

private struct HeartCard: View {
    let h: PhoneHealth
    var body: some View {
        Section(L("health_heart")) {
            InfoRow(L("health_heart_rate"), HealthFormat.value(h.heartRate, "bpm"))
            if h.hrHistory.count > 1 {
                VStack(alignment: .leading, spacing: 6) {
                    Text(L("health_hr_24h")).font(.caption).foregroundStyle(.secondary)
                    Chart(h.hrHistory, id: \.t) { p in
                        LineMark(
                            x: .value(L("health_time"), Date(timeIntervalSince1970: p.t / 1000)),
                            y: .value(L("health_heart_rate"), p.v))
                            .interpolationMethod(.monotone)
                            .foregroundStyle(Palette.gpu)
                    }
                    .chartYScale(domain: .automatic(includesZero: false))
                    .frame(height: 120)
                }
                .accessibilityElement(children: .ignore)
                .accessibilityLabel(L("health_hr_24h"))
                .accessibilityValue(
                    L("health_hr_chart", HealthFormat.value(h.heartRate, "bpm")) + ", " + Self.summary(h.hrHistory))
            }
            InfoRow(L("health_resting_hr"), HealthFormat.value(h.restingHr, "bpm"))
            InfoRow(L("health_hrv"), HealthFormat.value(h.hrv, "ms"))
            InfoRow(L("health_spo2"), HealthFormat.value(h.spo2, "%"))
            InfoRow(L("health_respiratory"), HealthFormat.value(h.respiratoryRate, L("health_unit_breaths")))
            InfoRow(L("health_blood_pressure"), HealthFormat.pressure(h.bloodPressure))
        }
    }

    /// "58–96 bpm" for VoiceOver, which cannot read a chart line.
    static func summary(_ points: [HrPoint]) -> String {
        let values = points.map(\.v)
        guard let lo = values.min(), let hi = values.max() else { return Format.dash }
        return "\(HealthFormat.value(lo, "")) \u{2013} \(HealthFormat.value(hi, "bpm"))"
    }
}

private struct SleepCard: View {
    let h: PhoneHealth
    var body: some View {
        Section(L("health_sleep")) {
            InfoRow(L("health_sleep_last"), HealthFormat.sleep(h.sleepMin))
        }
    }
}

private struct BodyCard: View {
    let h: PhoneHealth
    var body: some View {
        Section(L("health_body")) {
            InfoRow(L("health_weight"), HealthFormat.value(h.weightKg, "kg", digits: 1))
            InfoRow(L("health_body_fat"), HealthFormat.value(h.bodyFat, "%", digits: 1))
            // `body` is vmui's estimate from height and weight, labelled as one.
            InfoRow(L("health_bmi"), Self.bmi(h.body))
            if h.bodyFat == nil, let fat = h.body?.fatPct {
                InfoRow(L("health_fat_estimate"), HealthFormat.value(fat, "%", digits: 1))
            }
            InfoRow(L("health_bmr"), HealthFormat.value(h.bmr, "kcal"))
            InfoRow(L("health_vo2max"), HealthFormat.value(h.vo2max, "mL/kg/min"))
        }
    }

    /// "27.2 · overweight"; an unknown band is left off rather than guessed.
    static func bmi(_ b: BodyEstimate?) -> String {
        let value = HealthFormat.value(b?.bmi, "", digits: 1)
          guard b?.bmi != nil else { return value }
        let band: String? = switch b?.bmiBand ?? "" {
          case "sub": L("health_band_sub")
          case "normal": L("health_band_normal")
          case "over": L("health_band_over")
          case "obese": L("health_band_obese")
          default: nil
          }
          guard let band else { return value }
          return "\(value) \u{00B7} \(band)"
    }
}

private struct NutritionCard: View {
    let n: Nutrition
    var body: some View {
        Section(L("health_nutrition")) {
            InfoRow(L("health_calories_eaten"), HealthFormat.value(n.today?.calories, "kcal"))
            InfoRow(L("health_calories_target"), HealthFormat.value(n.targets?.calories, "kcal"))
            InfoRow(L("health_calories_remaining"), HealthFormat.value(n.remaining?.calories, "kcal"))
            Macro(label: L("health_protein"), value: n.today?.protein, target: n.targets?.protein, unit: "g", colour: Palette.cpu)
            Macro(label: L("health_carbs"), value: n.today?.carbs, target: n.targets?.carbs, unit: "g", colour: Palette.disk)
            Macro(label: L("health_fats"), value: n.today?.fats, target: n.targets?.fats, unit: "g", colour: Palette.thermal)
            Macro(label: L("health_water"), value: n.water?.ml, target: n.water?.targetMl, unit: "ml", colour: Palette.network)
        }
    }
}

private struct Macro: View {
    let label: String
    let value: Double?
    let target: Double?
    let unit: String
    let colour: Color

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            InfoRow(label, HealthFormat.of(value, target, unit))
            Bar(fraction: HealthFormat.fraction(value, target), colour: colour)
        }
        .accessibilityElement(children: .combine)
    }
}
