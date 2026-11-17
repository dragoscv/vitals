// The Home/Lock Screen widget: the headline PC from the snapshot the app
// writes into the shared app group. It reads nothing else and holds no token —
// the same picture the watch gets, in a smaller frame.
//
// An unmeasured reading is the em dash Format.dash, never a zero.

import SwiftUI
import VitalsCore
import WidgetKit

/// Localised string from the catalog this target shares with the app
/// (`Vitals/Localizable.xcstrings`, listed in project.yml).
func L(_ key: String, _ args: CVarArg...) -> String {
    let format = Bundle.main.localizedString(forKey: key, value: nil, table: nil)
    return args.isEmpty ? format : String(format: format, locale: .current, arguments: args)
}

struct VitalsEntry: TimelineEntry {
    let date: Date
    let snapshot: SharedSnapshot?
}

struct VitalsProvider: TimelineProvider {
    func placeholder(in context: Context) -> VitalsEntry {
        VitalsEntry(date: .now, snapshot: nil)
    }

    func getSnapshot(in context: Context, completion: @escaping (VitalsEntry) -> Void) {
        completion(VitalsEntry(date: .now, snapshot: SharedSnapshotStore.load()))
    }

    func getTimeline(in context: Context, completion: @escaping (Timeline<VitalsEntry>) -> Void) {
        // The app reloads the timeline whenever it writes a new snapshot, so a
        // 5-minute fallback is only a safety net.
        let entry = VitalsEntry(date: .now, snapshot: SharedSnapshotStore.load())
        completion(Timeline(entries: [entry], policy: .after(.now.addingTimeInterval(300))))
    }
}

struct VitalsWidgetView: View {
    @Environment(\.widgetFamily) private var family
    let entry: VitalsEntry

    var body: some View {
        content
            .containerBackground(.fill.tertiary, for: .widget)
    }

    @ViewBuilder
    private var content: some View {
        if let pc = entry.snapshot?.headline {
            switch family {
            case .accessoryCircular: circular(pc)
            case .accessoryInline: Text(inline(pc))
            case .accessoryRectangular: rectangular(pc)
            default: surface(pc)
            }
        } else {
            empty
        }
    }

    private var empty: some View {
        VStack(spacing: 4) {
            Image(systemName: "desktopcomputer").foregroundStyle(.secondary)
            Text(L("widget_empty")).font(.caption).foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
        }
    }

    private func surface(_ pc: SharedPc) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 4) {
                Circle().fill(colour(pc)).frame(width: 8, height: 8)
                Text(pc.label).font(.caption.weight(.semibold)).lineLimit(1)
            }
            HStack(alignment: .firstTextBaseline, spacing: 10) {
                reading(L("metric_cpu"), Format.percentCompact(pc.cpuPercent), Palette.cpu)
                reading(L("metric_memory_short"), Format.percentCompact(pc.memoryPercent), Palette.memory)
                if family == .systemMedium {
                    reading(L("metric_temp"), Format.celsius(pc.tempC), Palette.thermal)
                }
            }
        }
    }

    private func reading(_ label: String, _ value: String, _ colour: Color) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(label).font(.caption2).foregroundStyle(.secondary)
            Text(value).font(.title3.weight(.semibold).monospacedDigit()).foregroundStyle(colour)
        }
    }

    private func rectangular(_ pc: SharedPc) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(pc.label).font(.caption.weight(.semibold)).lineLimit(1)
            Text(inline(pc)).font(.caption2).foregroundStyle(.secondary)
        }
    }

    @ViewBuilder
    private func circular(_ pc: SharedPc) -> some View {
        // An unmeasured reading is the dash, not a gauge drawn at zero.
        if let cpu = pc.cpuPercent {
            Gauge(value: min(1, max(0, cpu / 100))) {
                Text(L("metric_cpu"))
            } currentValueLabel: {
                Text(Format.percentCompact(cpu)).font(.caption2.monospacedDigit())
            }
            .gaugeStyle(.accessoryCircularCapacity)
            .tint(Palette.cpu)
        } else {
            Text(Format.dash).font(.caption2.weight(.semibold))
        }
    }

    private func inline(_ pc: SharedPc) -> String {
        "\(pc.label) · \(L("metric_cpu")) \(Format.percentCompact(pc.cpuPercent))"
    }

    /// The dot reflects the PC's state, not a colour per metric.
    private func colour(_ pc: SharedPc) -> Color {
        if pc.alerts > 0 { return Palette.warn }
        return pc.live ? Palette.ok : .secondary
    }
}

@main
struct VitalsWidgetBundle: WidgetBundle {
    var body: some Widget { VitalsWidget() }
}

struct VitalsWidget: Widget {
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: "VitalsWidget", provider: VitalsProvider()) { entry in
            VitalsWidgetView(entry: entry)
        }
        .configurationDisplayName(L("widget_title"))
        .description(L("widget_description"))
        .supportedFamilies([
            .systemSmall, .systemMedium,
            .accessoryCircular, .accessoryRectangular, .accessoryInline,
        ])
    }
}

/// The metric colours of Design.swift, which this target does not compile.
enum Palette {
    static let cpu = Color(red: 0x00 / 255, green: 0x88 / 255, blue: 0xF2 / 255)
    static let memory = Color(red: 0x00 / 255, green: 0xB6 / 255, blue: 0x67 / 255)
    static let thermal = Color(red: 0xF6 / 255, green: 0x72 / 255, blue: 0x2B / 255)
    static let ok = Color(red: 0x1E / 255, green: 0xAB / 255, blue: 0x53 / 255)
    static let warn = Color(red: 0xE8 / 255, green: 0x9D / 255, blue: 0x00 / 255)
}
