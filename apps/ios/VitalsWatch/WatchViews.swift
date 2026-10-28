import SwiftUI
import VitalsCore

/// The iPhone's metric colours (Design.swift), without its UIKit dynamic
/// colours, which watchOS does not have. The watch is always dark.
enum WatchPalette {
    static let cpu = Color(red: 0x00 / 255, green: 0x88 / 255, blue: 0xF2 / 255)
    static let memory = Color(red: 0x00 / 255, green: 0xB6 / 255, blue: 0x67 / 255)
    static let gpu = Color(red: 0xF0 / 255, green: 0x56 / 255, blue: 0x53 / 255)
    static let thermal = Color(red: 0xF6 / 255, green: 0x72 / 255, blue: 0x2B / 255)
    static let ok = Color(red: 0x1E / 255, green: 0xAB / 255, blue: 0x53 / 255)
    static let warn = Color(red: 0xE8 / 255, green: 0x9D / 255, blue: 0x00 / 255)
    static let danger = Color(red: 0xEE / 255, green: 0x34 / 255, blue: 0x3B / 255)
    static let accent = Color(red: 0x6C / 255, green: 0xB8 / 255, blue: 0x82 / 255)

    static func of(_ state: WatchPcState) -> Color {
        switch state {
        case .live: ok
        case .connecting, .unknown: .secondary
        case .offline, .offLan: warn
        case .unauthorised, .incompatible: danger
        }
    }
}

extension WatchPcState {
    var label: String {
        switch self {
        case .live: L("watch_state_live")
        case .connecting: L("watch_state_connecting")
        case .offline: L("watch_state_offline")
        case .offLan: L("watch_state_off_lan")
        case .unauthorised: L("watch_state_unauthorised")
        case .incompatible: L("watch_state_incompatible")
        case .unknown: L("watch_state_unknown")
        }
    }
}

struct PcListView: View {
    @Environment(WatchModel.self) private var model

    var body: some View {
        Group {
            if let snap = model.snapshot, !snap.pcs.isEmpty {
                List {
                    ForEach(snap.pcs) { pc in
                        NavigationLink(value: pc.id) { PcRow(pc: pc) }
                    }
                    Section { UpdatedLine(sentMs: snap.sentMs) }
                }
            } else {
                EmptyState(waiting: model.snapshot == nil)
            }
        }
        .navigationTitle(L("machines_title"))
        .navigationDestination(for: String.self) { id in PcDetailView(id: id) }
    }
}

struct EmptyState: View {
    /// Nothing heard from the phone yet, rather than "the phone has no PCs".
    let waiting: Bool

    var body: some View {
        ScrollView {
            VStack(spacing: 8) {
                Image(systemName: "desktopcomputer").font(.title2).foregroundStyle(.secondary)
                    .accessibilityHidden(true)
                Text(L(waiting ? "watch_waiting" : "watch_empty_title")).font(.headline)
                    .multilineTextAlignment(.center)
                Text(L("watch_empty_body")).font(.footnote).foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
            }
            .padding(.top, 8)
        }
    }
}

struct PcRow: View {
    let pc: WatchPc

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 6) {
                StateDot(state: pc.state)
                Text(pc.label).font(.headline).lineLimit(1)
            }
            if pc.sample { SampleTag() }
            if pc.state == .live {
                HStack(spacing: 6) {
                    MiniRing(value: pc.cpuPercent, text: Format.percentCompact(pc.cpuPercent), colour: WatchPalette.cpu)
                        .accessibilityLabel(Text(verbatim: "\(L("metric_cpu")) \(Format.percent(pc.cpuPercent))"))
                    MiniRing(value: pc.memoryPercent, text: Format.percentCompact(pc.memoryPercent), colour: WatchPalette.memory)
                        .accessibilityLabel(Text(verbatim: "\(L("metric_memory_short")) \(Format.percent(pc.memoryPercent))"))
                    MiniRing(value: pc.tempC, text: Self.temp(pc.tempC), colour: WatchPalette.thermal, scale: 110)
                        .accessibilityLabel(Text(verbatim: "\(L("metric_temp")) \(Format.celsius(pc.tempC))"))
                }
            } else {
                Text(pc.state.label).font(.caption2).foregroundStyle(.secondary)
            }
        }
        .padding(.vertical, 2)
    }

    /// "81°" fits a mini ring where "81 °C" does not.
    static func temp(_ c: Double?) -> String {
        guard let c, c.isFinite else { return Format.dash }
        return "\(Int(c.rounded()))°"
    }
}

struct PcDetailView: View {
    let id: String
    @Environment(WatchModel.self) private var model

    var body: some View {
        if let pc = model.pc(id) {
            ScrollView {
                VStack(alignment: .leading, spacing: 10) {
                    HStack(spacing: 6) {
                        StateDot(state: pc.state)
                        Text(pc.state.label).font(.footnote)
                    }
                    if pc.sample { SampleTag() }
                    Grid(horizontalSpacing: 8, verticalSpacing: 8) {
                        GridRow {
                            Reading(label: L("metric_cpu"), value: Format.percent(pc.cpuPercent), colour: WatchPalette.cpu)
                            Reading(label: L("metric_memory_short"), value: Format.percent(pc.memoryPercent), colour: WatchPalette.memory)
                        }
                        GridRow {
                            Reading(label: L("metric_gpu"), value: Format.percent(pc.gpuPercent), colour: WatchPalette.gpu)
                            Reading(label: L("metric_temp"), value: Format.celsius(pc.tempC), colour: WatchPalette.thermal)
                        }
                    }
                    Text(pc.alerts > 0 ? String.localizedStringWithFormat(L("alert_count"), pc.alerts) : L("watch_no_alerts"))
                        .font(.footnote.weight(.semibold))
                        .foregroundStyle(pc.alerts > 0 ? WatchPalette.warn : .secondary)
                    if pc.state != .live, let seen = pc.lastSeenMs {
                        let date = Date(timeIntervalSince1970: Double(seen) / 1000)
                        Text(L("state_last_seen", date.formatted(.relative(presentation: .named))))
                            .font(.caption2).foregroundStyle(.secondary)
                    }
                    if let sent = model.snapshot?.sentMs { UpdatedLine(sentMs: sent) }
                }
            }
            .navigationTitle(pc.label)
        } else {
            EmptyState(waiting: false)
        }
    }
}

struct Reading: View {
    let label: String
    let value: String
    let colour: Color

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(label).font(.caption2).foregroundStyle(.secondary).lineLimit(1).minimumScaleFactor(0.7)
            Text(value).font(.title3.weight(.semibold).monospacedDigit()).foregroundStyle(colour)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .combine)
    }
}

/// A ring with no fill and "—" when the reading is unmeasured, never an
/// empty-looking 0.
struct MiniRing: View {
    let value: Double?
    let text: String
    let colour: Color
    var scale: Double = 100

    var body: some View {
        ZStack {
            Circle().stroke(colour.opacity(0.25), lineWidth: 3)
            if let value, value.isFinite {
                Circle().trim(from: 0, to: min(1, max(0, value / scale)))
                    .stroke(colour, style: StrokeStyle(lineWidth: 3, lineCap: .round))
                    .rotationEffect(.degrees(-90))
            }
            Text(text).font(.system(size: 10, weight: .semibold).monospacedDigit())
                .minimumScaleFactor(0.6).lineLimit(1).padding(3)
        }
        .frame(width: 34, height: 34)
    }
}

struct StateDot: View {
    let state: WatchPcState

    var body: some View {
        Circle().fill(WatchPalette.of(state)).frame(width: 8, height: 8)
            .accessibilityLabel(state.label)
    }
}

struct SampleTag: View {
    var body: some View {
        Text(L("watch_sample")).font(.caption2.weight(.semibold))
            .padding(.horizontal, 6).padding(.vertical, 2)
            .background(.yellow.opacity(0.25), in: Capsule())
    }
}

/// "Updated 12 seconds ago", refreshed every 10 s, with an out-of-date mark
/// once the phone has been silent for more than two minutes.
struct UpdatedLine: View {
    let sentMs: Int64

    var body: some View {
        TimelineView(.periodic(from: .now, by: 10)) { ctx in
            let nowMs = Int64(ctx.date.timeIntervalSince1970 * 1000)
            let sent = Date(timeIntervalSince1970: Double(sentMs) / 1000)
            let stale = nowMs - sentMs > WatchSnapshot.staleAfterMs
            VStack(alignment: .leading, spacing: 2) {
                Text(L("watch_updated", sent.formatted(.relative(presentation: .named, unitsStyle: .abbreviated))))
                    .font(.caption2).foregroundStyle(.secondary)
                if stale {
                    Label(L("watch_stale"), systemImage: "exclamationmark.triangle")
                        .font(.caption2.weight(.semibold)).foregroundStyle(WatchPalette.warn)
                }
            }
        }
    }
}
