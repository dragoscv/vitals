// The watch complication / Smart-Stack widget. It reads the same app-group
// snapshot the watch app writes the moment a new picture arrives from the
// phone, so a glance on the wrist shows what the phone last had — never a
// number it invented. An unmeasured reading is Format.dash.

import SwiftUI
import VitalsCore
import WidgetKit

/// Localised string from the catalog this target shares with the apps.
func L(_ key: String, _ args: CVarArg...) -> String {
    let format = Bundle.main.localizedString(forKey: key, value: nil, table: nil)
    return args.isEmpty ? format : String(format: format, locale: .current, arguments: args)
}

struct WatchEntry: TimelineEntry {
    let date: Date
    let snapshot: SharedSnapshot?
}

struct WatchProvider: TimelineProvider {
    func placeholder(in context: Context) -> WatchEntry { WatchEntry(date: .now, snapshot: nil) }

    func getSnapshot(in context: Context, completion: @escaping (WatchEntry) -> Void) {
        completion(WatchEntry(date: .now, snapshot: SharedSnapshotStore.load()))
    }

    func getTimeline(in context: Context, completion: @escaping (Timeline<WatchEntry>) -> Void) {
        let entry = WatchEntry(date: .now, snapshot: SharedSnapshotStore.load())
        completion(Timeline(entries: [entry], policy: .after(.now.addingTimeInterval(300))))
    }
}

struct VitalsWatchWidgetView: View {
    @Environment(\.widgetFamily) private var family
    let entry: WatchEntry

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
            case .accessoryCorner: corner(pc)
            default: rectangular(pc)
            }
        } else {
            Text(L("widget_empty")).font(.caption2).foregroundStyle(.secondary)
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
            .tint(WatchWidgetPalette.cpu)
        } else {
            Text(Format.dash).font(.caption2.weight(.semibold))
        }
    }

    private func corner(_ pc: SharedPc) -> some View {
        Text(Format.percentCompact(pc.cpuPercent))
            .font(.caption.monospacedDigit())
            .widgetLabel { Text(pc.label) }
    }

    private func inline(_ pc: SharedPc) -> String {
        "\(pc.label) · \(L("metric_cpu")) \(Format.percentCompact(pc.cpuPercent))"
    }
}

/// The watch metric colours of the watch app (WatchViews.swift), which this
/// target does not compile.
enum WatchWidgetPalette {
    static let cpu = Color(red: 0x00 / 255, green: 0x88 / 255, blue: 0xF2 / 255)
}

@main
struct VitalsWatchWidgetBundle: WidgetBundle {
    var body: some Widget { VitalsWatchWidget() }
}

struct VitalsWatchWidget: Widget {
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: "VitalsWatchWidget", provider: WatchProvider()) { entry in
            VitalsWatchWidgetView(entry: entry)
        }
        .configurationDisplayName(L("widget_title"))
        .description(L("widget_description"))
        .supportedFamilies([
            .accessoryCircular, .accessoryRectangular, .accessoryInline, .accessoryCorner,
        ])
    }
}
