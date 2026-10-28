import SwiftUI
import UIKit
import VitalsCore

/// The PC's programs: search, sort, tap for details and controls. Re-sorted
/// at most every 2.5 s so rows do not jump under a finger (desktop S22-10).
struct ProcessesView: View {
    let pairingId: String
    enum Sort: Hashable { case cpu, memory, name }

    @Environment(PairingStore.self) private var store
    @Environment(LiveHub.self) private var hub
    @Environment(SampleMode.self) private var sample
    @State private var query = ""
    @State private var sort: Sort = .cpu
    @State private var selected: ProcessKey?
    @State private var order: [ProcessKey] = []
    @State private var lastSort = Date.distantPast

    private var isSample: Bool { pairingId == SampleMode.id }
    private var pairing: Pairing? { isSample ? sample.pairing : store.pairing(pairingId) }
    private var view: MachineView? { isSample ? sample.view : hub.stream(for: pairingId).view }

    var body: some View {
        let procs = Array(view?.processes.values ?? [:].values)
        let byKey = Dictionary(uniqueKeysWithValues: procs.map { ($0.key, $0) })
        let filtered = ordered(procs, byKey).filter {
            query.isEmpty || $0.displayName.localizedCaseInsensitiveContains(query) || $0.name.localizedCaseInsensitiveContains(query)
        }
        List {
            Picker("", selection: $sort) {
                Text(L("process_sort_cpu")).tag(Sort.cpu)
                Text(L("process_sort_memory")).tag(Sort.memory)
                Text(L("process_sort_name")).tag(Sort.name)
            }
            .pickerStyle(.segmented)
            if view == nil {
                Text(L("processes_waiting")).foregroundStyle(.secondary)
            } else if filtered.isEmpty {
                Text(L("processes_none")).foregroundStyle(.secondary)
            }
            ForEach(filtered) { p in
                Button { selected = p.key } label: { ProcessRow(process: p) }
                    .foregroundStyle(.primary)
            }
        }
        .searchable(text: $query, prompt: L("processes_search"))
        .navigationTitle(L("processes_title", pairing?.label ?? ""))
        .navigationBarTitleDisplayMode(.inline)
        .onChange(of: sort) { lastSort = .distantPast }
        .sheet(item: Binding(get: { selected.map(KeyBox.init) }, set: { selected = $0?.key })) { box in
            if let p = byKey[box.key], let pairing {
                ProcessSheet(process: p, pairing: pairing, isSample: isSample)
                    .presentationDetents([.medium, .large])
            }
        }
        .onAppear { if let pairing, !isSample { hub.stream(for: pairing.id).attach(pairing) } }
        .onDisappear { if !isSample { hub.stream(for: pairingId).detach() } }
    }

    private func ordered(_ procs: [Process], _ byKey: [ProcessKey: Process]) -> [Process] {
        let now = Date()
        if now.timeIntervalSince(lastSort) >= 2.5 || order.count != procs.count {
            let sorted: [Process] = switch sort {
            case .cpu: procs.sorted { $0.cpu > $1.cpu }
            case .memory: procs.sorted { $0.memoryPrivate > $1.memoryPrivate }
            case .name: procs.sorted { $0.displayName.localizedCaseInsensitiveCompare($1.displayName) == .orderedAscending }
            }
            DispatchQueue.main.async {
                order = sorted.map(\.key)
                lastSort = now
            }
            return sorted
        }
        return order.compactMap { byKey[$0] }
    }
}

struct KeyBox: Identifiable {
    let key: ProcessKey
    var id: ProcessKey { key }
}

struct ProcessRow: View {
    let process: Process
    var body: some View {
        HStack {
            VStack(alignment: .leading, spacing: 2) {
                Text(process.displayName).lineLimit(1)
                if process.state == .suspended {
                    Text(L("process_state_suspended")).font(.caption2).foregroundStyle(Palette.warn)
                } else if process.state == .notResponding {
                    Text(L("process_state_not_responding")).font(.caption2).foregroundStyle(Palette.danger)
                }
            }
            Spacer()
            Text(Format.percent(process.cpu)).monospacedDigit()
                .foregroundStyle(process.cpu >= 85 ? Palette.danger : .primary)
                .frame(width: 56, alignment: .trailing)
            Text(Format.bytes(process.memoryPrivate)).monospacedDigit().foregroundStyle(.secondary)
                .frame(width: 76, alignment: .trailing)
        }
        .accessibilityElement(children: .combine)
    }
}

struct ProcessSheet: View {
    let process: Process
    let pairing: Pairing
    let isSample: Bool
    @Environment(PairingStore.self) private var store
    @Environment(\.dismiss) private var dismiss
    @State private var armedEnd = false
    @State private var result: String?
    @State private var busy = false

    var body: some View {
        NavigationStack {
            List {
                Section {
                    InfoRow(L("metric_cpu"), Format.percent(process.cpu))
                    InfoRow(L("process_memory"), Format.bytes(process.memoryPrivate))
                    InfoRow(L("process_disk"), "\(Format.rate(process.diskRead)) · \(Format.rate(process.diskWrite))")
                    InfoRow(L("process_gpu"), Format.percent(process.gpu))
                    InfoRow(L("process_threads"), "\(process.threadCount)")
                    InfoRow(L("process_uptime"), Format.duration(process.uptimeSecs))
                    InfoRow(L("process_user"), process.user ?? Format.dash)
                    InfoRow(L("process_pid"), "\(process.key.pid)")
                }
                controls
                if let result { Section { Text(result).font(.footnote) } }
            }
            .navigationTitle(process.displayName)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button(L("common_done")) { dismiss() } } }
        }
    }

    @ViewBuilder
    private var controls: some View {
        let current = store.pairing(pairing.id) ?? pairing
        if isSample || current.scope == .read {
            Section { Text(L("process_read_only")).foregroundStyle(.secondary) }
        } else if !process.isSafelyTerminable {
            Section { Text(L("process_protected")).foregroundStyle(.secondary) }
        } else {
            Section {
                Button(role: .destructive) {
                    if armedEnd {
                        send(.terminate(process.key), closeOnSuccess: true)
                    } else {
                        armedEnd = true
                        Task { try? await Task.sleep(for: .seconds(4)); armedEnd = false }
                    }
                } label: { Text(armedEnd ? L("action_end_confirm") : L("action_end")) }
                if process.state == .suspended {
                    Button(L("action_resume")) { send(.resume(process.key)) }
                } else {
                    Button(L("action_suspend")) { send(.suspend(process.key)) }
                }
                Menu(L("action_priority")) {
                    ForEach(ControlRequest.Priority.allCases, id: \.self) { p in
                        Button(priorityName(p)) { send(.setPriority(process.key, p)) }
                    }
                }
                Toggle(isOn: Binding(get: { process.inEfficiencyMode }, set: { send(.setEfficiencyMode(process.key, $0)) })) {
                    VStack(alignment: .leading) {
                        Text(L("action_efficiency"))
                        Text(L("action_efficiency_body")).font(.caption).foregroundStyle(.secondary)
                    }
                }
            }
            .disabled(busy)
        }
    }

    private func priorityName(_ p: ControlRequest.Priority) -> String {
        switch p {
        case .idle: L("priority_idle")
        case .belowNormal: L("priority_below_normal")
        case .normal: L("priority_normal")
        case .aboveNormal: L("priority_above_normal")
        case .high: L("priority_high")
        case .realtime: L("priority_realtime")
        }
    }

    private func send(_ request: ControlRequest, closeOnSuccess: Bool = false) {
        busy = true
        let client = VitalsClient(baseUrl: pairing.baseUrl, token: pairing.token)
        Task { @MainActor in
            let r = await client.control(request)
            busy = false
            switch r {
            case .success:
                result = L("control_done")
                UINotificationFeedbackGenerator().notificationOccurred(.success)
                if closeOnSuccess { dismiss() }
            case let .failure(f):
                UINotificationFeedbackGenerator().notificationOccurred(.error)
                if case .refused(.forbidden) = f { store.downgradeToRead(pairing.id) }
                result = words(f)
            }
        }
    }

    private func words(_ f: ApiFailure) -> String {
        switch f {
        case .refused(.forbidden): L("control_forbidden")
        case .refused(.accessDenied): L("control_access_denied")
        case .refused(.notFound): L("control_not_found")
        case let .refused(.unsupported(m)): L("control_unsupported", m)
        case let .refused(.internalError(m)): L("control_internal", m)
        case .unreachable, .offLan, .refused(.unreachable): L("control_unreachable")
        case let .http(s): L("control_failed", s)
        default: L("control_failed", 0)
        }
    }
}
