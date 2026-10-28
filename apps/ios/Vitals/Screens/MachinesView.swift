import SwiftUI
import VitalsCore

struct MachinesView: View {
    @Environment(PairingStore.self) private var store
    @Environment(LiveHub.self) private var hub
    @Environment(SampleMode.self) private var sample
    @State private var adding = false

    private var showSample: Bool { sample.enabled || store.pairings.isEmpty }

    var body: some View {
        List {
            if store.loaded && store.pairings.isEmpty {
                Section {
                    VStack(alignment: .leading, spacing: 10) {
                        Text(L("machines_empty_title")).font(.headline)
                        Text(L("machines_empty_body")).foregroundStyle(.secondary)
                        Button(L("add_pc")) { adding = true }.buttonStyle(.borderedProminent)
                        Button(L("sample_try")) { sample.enabled = true }
                    }
                    .padding(.vertical, 6)
                }
            }
            ForEach(store.pairings) { p in
                NavigationLink(value: p.id) { MachineCard(pairing: p, stream: hub.stream(for: p.id)) }
            }
            if sample.enabled {
                NavigationLink(value: SampleMode.id) { SampleCard() }
            }
        }
        .navigationTitle(L("machines_title"))
        .navigationDestination(for: String.self) { id in
            DetailView(pairingId: id)
        }
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button { adding = true } label: { Label(L("add_pc"), systemImage: "plus") }
            }
        }
        .refreshable {
            for p in store.pairings { hub.stream(for: p.id).retry(p) }
        }
        .sheet(isPresented: $adding) { AddPcView() }
        .onOpenURL { _ in adding = true }
        .onReceive(NotificationCenter.default.publisher(for: .vitalsScan)) { _ in adding = true }
    }
}

extension Notification.Name {
    static let vitalsScan = Notification.Name("app.vitals.ios.scan")
}

struct MachineCard: View {
    let pairing: Pairing
    let stream: PcStream
    @Environment(PairingStore.self) private var store

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text(pairing.label).font(.headline)
                Spacer()
                AlertChip(alerts: stream.alerts)
            }
            StatusLine(state: stream.state, lastSeenMs: stream.lastSeenMs)
            if let s = stream.view?.system, stream.state == .live {
                LiveBody(system: s, cpu: stream.cpuHistory, mem: stream.memHistory)
            } else if stream.state == .unauthorised {
                Text(L("pair_again_hint")).font(.footnote).foregroundStyle(.secondary)
            }
        }
        .padding(.vertical, 4)
        .onAppear { stream.attach(pairing) }
        .onDisappear { stream.detach() }
    }
}

struct SampleCard: View {
    @Environment(SampleMode.self) private var sample

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text(L("sample_title")).font(.headline)
                Spacer()
                Text(L("sample_title")).font(.caption2.weight(.semibold))
                    .padding(.horizontal, 6).padding(.vertical, 2)
                    .background(.yellow.opacity(0.25), in: Capsule())
            }
            Text(L("sample_banner")).font(.footnote).foregroundStyle(.secondary)
            if let s = sample.view?.system {
                LiveBody(system: s, cpu: sample.cpuHistory, mem: sample.memHistory)
            }
        }
        .padding(.vertical, 4)
        .onAppear { sample.start() }
    }
}

struct LiveBody: View {
    let system: SystemMetrics
    let cpu: [Double]
    let mem: [Double]

    var body: some View {
        let gpu = Readings.primaryGpu(system)
        let temp = Readings.hottest(system)
        VStack(spacing: 8) {
            HStack {
                Ring(label: L("metric_cpu"), value: system.cpu.total, text: Format.percent(system.cpu.total), colour: Palette.cpu)
                Spacer()
                Ring(label: L("metric_memory_short"), value: system.memory.usedPercent,
                     text: Format.percent(system.memory.usedPercent), colour: Palette.memory)
                Spacer()
                Ring(label: L("metric_gpu"), value: gpu?.utilization, text: Format.percent(gpu?.utilization), colour: Palette.gpu)
                Spacer()
                Ring(label: L("metric_temp"), value: temp, text: Format.celsius(temp), colour: Palette.thermal, scale: 110)
            }
            HStack(spacing: 12) {
                Sparkline(values: cpu, colour: Palette.cpu).frame(height: 24)
                Sparkline(values: mem, colour: Palette.memory).frame(height: 24)
            }
        }
    }
}

struct StatusLine: View {
    let state: PcState
    let lastSeenMs: Int64?

    var body: some View {
        switch state {
        case .live: EmptyView()
        case .connecting: Text(L("state_connecting")).font(.footnote).foregroundStyle(.secondary)
        case .unauthorised: Text(L("state_unauthorised")).font(.footnote).foregroundStyle(Palette.danger)
        case .incompatible: Text(L("state_incompatible")).font(.footnote).foregroundStyle(Palette.warn)
        case .offLan:
            VStack(alignment: .leading) {
                Text(L("state_off_lan")).font(.footnote.weight(.semibold))
                Text(L("state_off_lan_hint")).font(.caption).foregroundStyle(.secondary)
            }
        case let .unreachable(seen):
            if let seen {
                let date = Date(timeIntervalSince1970: Double(seen) / 1000)
                Text(L("state_last_seen", date.formatted(.relative(presentation: .named))))
                    .font(.footnote).foregroundStyle(.secondary)
            } else {
                VStack(alignment: .leading) {
                    Text(L("state_unreachable")).font(.footnote.weight(.semibold))
                    Text(L("state_unreachable_hint")).font(.caption).foregroundStyle(.secondary)
                }
            }
        }
    }
}

struct AlertChip: View {
    let alerts: [VitalsCore.Alert]

    var body: some View {
        let serious = alerts.filter { $0.severity != .info }
        if !serious.isEmpty {
            let critical = serious.contains { $0.severity == .critical }
            Text(String.localizedStringWithFormat(L("alert_count"), serious.count))
                .font(.caption.weight(.semibold))
                .padding(.horizontal, 8).padding(.vertical, 3)
                .background((critical ? Palette.danger : Palette.warn).opacity(0.18), in: Capsule())
                .foregroundStyle(critical ? Palette.danger : Palette.warn)
        }
    }
}
