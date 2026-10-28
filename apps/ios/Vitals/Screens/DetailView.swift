import SwiftUI
import VitalsCore

/// One PC: Now / Sensors / History, plus the programs list.
struct DetailView: View {
    let pairingId: String
    @Environment(PairingStore.self) private var store
    @Environment(LiveHub.self) private var hub
    @Environment(SampleMode.self) private var sample
    @State private var tab = ScreenshotRoute.tab
    @State private var showProcesses = ScreenshotRoute.processes

    private var isSample: Bool { pairingId == SampleMode.id }
    private var pairing: Pairing? { isSample ? sample.pairing : store.pairing(pairingId) }

    var body: some View {
        if let pairing {
            content(pairing)
        } else {
            ContentUnavailableView(L("state_unreachable"), systemImage: "desktopcomputer.trianglebadge.exclamationmark")
        }
    }

    @ViewBuilder
    private func content(_ pairing: Pairing) -> some View {
        let stream = isSample ? nil : hub.stream(for: pairing.id)
        let view = isSample ? sample.view : stream?.view
        let alerts = stream?.alerts ?? []
        VStack(spacing: 0) {
            Picker("", selection: $tab) {
                Text(L("tab_now")).tag(0)
                Text(L("tab_sensors")).tag(1)
                Text(L("tab_history")).tag(2)
            }
            .pickerStyle(.segmented)
            .padding([.horizontal, .top])
            if isSample {
                Text(L("sample_banner")).font(.caption).padding(8)
                    .frame(maxWidth: .infinity).background(.yellow.opacity(0.2))
            }
            switch tab {
            case 0:
                NowTab(state: isSample ? .live : stream?.state ?? .connecting, view: view, alerts: alerts,
                       lastSeen: stream?.lastSeenMs,
                       cpu: isSample ? sample.cpuHistory : stream?.cpuHistory ?? [],
                       mem: isSample ? sample.memHistory : stream?.memHistory ?? [],
                       pairing: pairing, isSample: isSample)
            case 1: SensorsTab(pairing: pairing, isSample: isSample)
            default: HistoryTab(pairing: pairing, isSample: isSample)
            }
        }
        .navigationTitle(pairing.label)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                NavigationLink {
                    ProcessesView(pairingId: pairing.id)
                } label: {
                    Label(L("processes"), systemImage: "list.bullet.rectangle")
                }
            }
        }
        .navigationDestination(isPresented: $showProcesses) { ProcessesView(pairingId: pairing.id) }
        .onAppear { if let stream { stream.attach(pairing) } else { sample.start() } }
        .onDisappear { stream?.detach() }
    }
}

struct NowTab: View {
    let state: PcState
    let view: MachineView?
    let alerts: [VitalsCore.Alert]
    let lastSeen: Int64?
    let cpu: [Double]
    let mem: [Double]
    let pairing: Pairing
    let isSample: Bool
    @Environment(LiveHub.self) private var hub

    var body: some View {
        List {
            if state != .live || view == nil {
                Section { StatusLine(state: state, lastSeenMs: lastSeen) }
            }
            if !alerts.isEmpty {
                Section(L("alert_count_title")) {
                    ForEach(alerts) { a in
                        Label(alertTitle(a), systemImage: a.severity == .critical ? "exclamationmark.octagon.fill" : "exclamationmark.triangle.fill")
                            .foregroundStyle(a.severity == .critical ? Palette.danger : Palette.warn)
                    }
                }
            }
            if let s = view?.system {
                Section { LiveBody(system: s, cpu: cpu, mem: mem) }
                CpuSection(cpu: s.cpu)
                MemorySection(memory: s.memory)
                ForEach(s.gpus) { GpuSection(gpu: $0) }
                if !s.disks.isEmpty { DisksSection(disks: s.disks) }
                let nets = s.networks.filter(\.connected)
                if !nets.isEmpty { NetworkSection(networks: nets) }
                if !s.fans.isEmpty {
                    Section(L("metric_fans")) { ForEach(s.fans, id: \.name) { InfoRow($0.name, Format.rpm($0.rpm)) } }
                }
                if let b = s.battery { BatterySection(battery: b) }
                if !isSample { AgentsCard(pairing: pairing) }
            }
        }
        .refreshable { if !isSample { hub.stream(for: pairing.id).retry(pairing) } }
    }

    private func alertTitle(_ a: VitalsCore.Alert) -> String {
        let key: String = switch a.kind {
        case .cpuSustained: "alert_cpu_sustained"
        case .cpuThrottled: "alert_cpu_throttled"
        case .memoryPressure: "alert_memory_pressure"
        case .memoryCommit: "alert_memory_commit"
        case .diskSaturated: "alert_disk_saturated"
        case .diskLatency: "alert_disk_latency"
        case .diskSpace: "alert_disk_space"
        case .diskHealth: "alert_disk_health"
        case .gpuThrottled: "alert_gpu_throttled"
        case .thermalCpu: "alert_thermal_cpu"
        case .networkErrors: "alert_network_errors"
        case .batteryLow: "alert_battery_low"
        case .batteryHealth: "alert_battery_health"
        case .unknown: "alert_other"
        }
        let title = L(key)
        return a.subject.isEmpty ? title : "\(title) · \(a.subject)"
    }
}

struct CpuSection: View {
    let cpu: CpuMetrics
    var body: some View {
        Section(L("metric_cpu")) {
            InfoRow(L("cpu_clock"), Format.ghz(cpu.effectiveClock))
            InfoRow(L("cpu_temperature"), Format.celsius(cpu.temperature))
            InfoRow(L("cpu_power"), Format.watts(cpu.power))
            InfoRow(L("cpu_throttled"), cpu.throttled == nil ? L("cpu_not_throttled") : L("cpu_throttled"))
            InfoRow(L("cpu_processes"), "\(cpu.processCount)")
            InfoRow(L("cpu_uptime"), Format.duration(cpu.uptimeSecs))
            if !cpu.perCore.isEmpty {
                VStack(alignment: .leading, spacing: 6) {
                    Text(L("cpu_cores")).font(.caption).foregroundStyle(.secondary)
                    LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 6), count: 8), spacing: 6) {
                        ForEach(Array(cpu.perCore.enumerated()), id: \.offset) { _, v in
                            Bar(fraction: v / 100, colour: Palette.of(Level.cpu(v)) == .secondary ? Palette.cpu : Palette.cpu)
                        }
                    }
                }
                .accessibilityElement(children: .ignore)
                .accessibilityLabel(L("cpu_cores"))
                .accessibilityValue(cpu.perCore.map { Format.percent($0) }.joined(separator: ", "))
            }
        }
    }
}

struct MemorySection: View {
    let memory: MemoryMetrics
    var body: some View {
        Section(L("metric_memory")) {
            Text(L("memory_used_of", Format.bytes(memory.used), Format.bytes(memory.total)))
            Bar(fraction: memory.usedPercent.map { $0 / 100 }, colour: Palette.memory)
            InfoRow(L("memory_available"), Format.bytes(memory.available))
            InfoRow(L("memory_cached"), Format.bytes(memory.cached))
            // Windows only: a Mac sends no commit charge, and the row is left out.
            if let c = memory.committed, let l = memory.commitLimit {
                InfoRow(L("memory_committed"), L("memory_used_of", Format.bytes(c), Format.bytes(l)))
            }
        }
    }
}

struct GpuSection: View {
    let gpu: GpuMetrics
    var body: some View {
        Section(gpu.name) {
            InfoRow(L("metric_gpu"), Format.percent(gpu.utilization))
            if gpu.memoryUsed != nil || gpu.memoryTotal != nil {
                InfoRow(L("gpu_memory"), L("memory_used_of", Format.bytes(gpu.memoryUsed), Format.bytes(gpu.memoryTotal)))
            }
            InfoRow(L("gpu_temperature"), Format.celsius(gpu.temperature))
            InfoRow(L("gpu_power"), Format.watts(gpu.power))
            if gpu.fanRpm != nil { InfoRow(L("gpu_fan"), Format.rpm(gpu.fanRpm)) }
        }
    }
}

struct DisksSection: View {
    let disks: [DiskMetrics]
    var body: some View {
        Section(L("metric_disk")) {
            ForEach(disks) { d in
                VStack(alignment: .leading, spacing: 4) {
                    Text(d.mount ?? d.name).font(.subheadline.weight(.semibold))
                    Text(L("disk_free", Format.bytes(d.free), Format.bytes(d.total))).font(.caption)
                    Bar(fraction: d.total > 0 ? Double(d.total - d.free) / Double(d.total) : nil, colour: Palette.disk)
                    Text(L("disk_read_write", Format.rate(d.read), Format.rate(d.write)))
                        .font(.caption).foregroundStyle(.secondary)
                    if let t = d.temperature { InfoRow(L("cpu_temperature"), Format.celsius(t)) }
                    if let life = d.health?.lifeRemaining { InfoRow(L("disk_life"), Format.percent(life)) }
                    if d.health?.failing == true {
                        Text(L("disk_health_failing")).font(.caption.weight(.semibold)).foregroundStyle(Palette.danger)
                    }
                }
            }
        }
    }
}

struct NetworkSection: View {
    let networks: [NetworkMetrics]
    var body: some View {
        Section(L("metric_network")) {
            ForEach(networks) { n in
                InfoRow(n.adapter ?? n.name, "↓ \(Format.rate(n.rx))  ↑ \(Format.rate(n.tx))")
            }
        }
    }
}

struct BatterySection: View {
    let battery: BatteryMetrics
    var body: some View {
        Section(L("metric_battery")) {
            InfoRow(battery.charging ? L("battery_charging") : L("battery_discharging"), Format.percent(battery.charge))
            InfoRow(L("battery_time_left"), Format.duration(battery.timeRemainingSecs))
            InfoRow(L("battery_health"), Format.percent(battery.health))
        }
    }
}

struct AgentsCard: View {
    let pairing: Pairing
    @State private var snapshot: AgentsSnapshot?

    var body: some View {
        Group {
            if let s = snapshot {
                Section(L("agents_title")) {
                    if s.busySessions.isEmpty {
                        Text(L("agents_none_working")).foregroundStyle(.secondary)
                    }
                    ForEach(s.busySessions) { session in
                        InfoRow(session.repo ?? L("agents_client_other"), stateText(session.state))
                    }
                    if s.busyQueues.isEmpty {
                        Text(L("agents_queues_idle")).foregroundStyle(.secondary)
                    }
                    ForEach(s.busyQueues) { q in
                        InfoRow(q.name, L("agents_queue_value", q.holding, q.waiting))
                    }
                }
            }
        }
        .task(id: pairing.id) {
            let client = VitalsClient(baseUrl: pairing.baseUrl, token: pairing.token)
            while !Task.isCancelled {
                var wait = 60
                switch await client.agents() {
                case let .success(s):
                    snapshot = s
                    wait = s.busySessions.isEmpty ? 60 : 10
                case .failure(.http(404)), .failure(.unauthorised):
                    snapshot = nil
                    wait = 300
                default: break
                }
                try? await Task.sleep(for: .seconds(wait))
            }
        }
    }

    private func stateText(_ s: SessionState) -> String {
        switch s {
        case .working: L("agents_state_working")
        case .waiting: L("agents_state_waiting")
        case .idle: L("agents_state_idle")
        case .ended: L("agents_state_ended")
        case .unknown: L("agents_state_unknown")
        }
    }
}

struct SensorsTab: View {
    let pairing: Pairing
    let isSample: Bool
    @State private var lines: [SensorLine]?
    @State private var failed = false

    var body: some View {
        List {
            if let lines {
                if lines.isEmpty {
                    Text(L("sensors_empty")).foregroundStyle(.secondary)
                }
                ForEach(groups(lines), id: \.0) { title, rows in
                    Section(title) {
                        ForEach(rows) { InfoRow($0.label, Format.sensor(unit: $0.unit, value: $0.value)) }
                    }
                }
            } else if failed {
                Text(L("sensors_unavailable")).foregroundStyle(.secondary)
            } else {
                ProgressView()
            }
        }
        .task(id: pairing.id) {
            if isSample {
                lines = [
                    SensorLine.sample("cpu.package", "CPU package", 54, "temperature"),
                    SensorLine.sample("gpu.core", "GPU core", 46, "temperature"),
                    SensorLine.sample("fan.1", "Fan 1", 1240, "fanSpeed"),
                ]
                return
            }
            let client = VitalsClient(baseUrl: pairing.baseUrl, token: pairing.token)
            while !Task.isCancelled {
                switch await client.sensors() {
                case let .success(l): lines = l; failed = false
                // Keep the last readings on a blip; show "unavailable" only with none.
                case .failure: if lines == nil { failed = true }
                }
                try? await Task.sleep(for: .seconds(5))
            }
        }
    }

    private func groups(_ lines: [SensorLine]) -> [(String, [SensorLine])] {
        let order: [(String, String)] = [
            ("temperature", "sensors_group_temperature"), ("fanSpeed", "sensors_group_fan"),
            ("power", "sensors_group_power"), ("voltage", "sensors_group_voltage"), ("percent", "sensors_group_percent"),
        ]
        var out: [(String, [SensorLine])] = []
        for (unit, key) in order {
            let rows = lines.filter { $0.unit == unit || (unit == "percent" && $0.unit == "charge") }
            if !rows.isEmpty { out.append((L(key), rows)) }
        }
        let known = Set(order.map(\.0) + ["charge"])
        let other = lines.filter { !known.contains($0.unit) }
        if !other.isEmpty { out.append((L("sensors_group_other"), other)) }
        return out
    }
}

extension SensorLine {
    static func sample(_ key: String, _ label: String, _ value: Double, _ unit: String) -> SensorLine {
        let json = #"{"key":"\#(key)","label":"\#(label)","value":\#(value),"unit":"\#(unit)","source":"sample","quality":"measured"}"#
        // Built from JSON so it goes through the same decoder as real lines.
        return try! JSONDecoder().decode(SensorLine.self, from: Data(json.utf8))  // swiftlint:disable:this force_try
    }
}

struct HistoryTab: View {
    let pairing: Pairing
    let isSample: Bool
    @State private var range = 3600
    @State private var rows: [MachineSample]?
    @State private var failed = false

    var body: some View {
        List {
            Picker("", selection: $range) {
                Text(L("history_1h")).tag(3600)
                Text(L("history_24h")).tag(86_400)
                Text(L("history_7d")).tag(604_800)
            }
            .pickerStyle(.segmented)
            if let rows {
                if rows.isEmpty {
                    Text(L("history_empty")).foregroundStyle(.secondary)
                } else {
                    chart(L("metric_cpu"), rows.map(\.cpuPercent), Palette.cpu, Format.percent(rows.map(\.cpuPercent).max()))
                    let mem = rows.map { $0.memoryTotal > 0 ? Double($0.memoryUsed) / Double($0.memoryTotal) * 100 : 0 }
                    chart(L("metric_memory_short"), mem, Palette.memory, Format.percent(mem.max()))
                    let gpu = rows.compactMap(\.gpuPercent)
                    if !gpu.isEmpty { chart(L("metric_gpu"), gpu, Palette.gpu, Format.percent(gpu.max())) }
                    let temp = rows.compactMap(\.cpuTempC)
                    if !temp.isEmpty { chart(L("metric_temp"), temp, Palette.thermal, Format.celsius(temp.max()), max: 110) }
                }
            } else if failed {
                Text(L("sensors_unavailable")).foregroundStyle(.secondary)
            } else {
                HStack { ProgressView(); Text(L("history_loading")) }
            }
        }
        .task(id: range) {
            rows = nil
            failed = false
            if isSample { rows = SampleData.history(); return }
            let client = VitalsClient(baseUrl: pairing.baseUrl, token: pairing.token)
            switch await client.history(seconds: range) {
            case let .success(r): rows = r
            case .failure: failed = true
            }
        }
    }

    private func chart(_ title: String, _ values: [Double], _ colour: Color, _ peak: String, max: Double = 100) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text(title).font(.subheadline.weight(.semibold))
                Spacer()
                Text(L("history_peak", peak)).font(.caption).foregroundStyle(.secondary)
            }
            Sparkline(values: values, colour: colour, max: max).frame(height: 70)
        }
        .accessibilityElement(children: .combine)
    }
}
