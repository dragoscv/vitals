import SwiftUI
import VitalsCore

/// A translated string by its catalog key (the Android resource name, so the
/// two phones share wording). Arguments fill %@ / %lld placeholders.
func L(_ key: String, _ args: CVarArg...) -> String {
    let format = Bundle.main.localizedString(forKey: key, value: nil, table: nil)
    return args.isEmpty ? format : String(format: format, locale: .current, arguments: args)
}

@main
struct VitalsApp: App {
    @State private var store = PairingStore()
    @State private var hub = LiveHub()
    @State private var lan = LanMonitor()
    @State private var sample = SampleMode()
    @State private var health = HealthStore()
    @State private var watch: WatchBridge?
    @Environment(\.scenePhase) private var phase

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(store)
                .environment(hub)
                .environment(lan)
                .environment(sample)
                .environment(health)
                .tint(Palette.accent)
                .task {
                    guard watch == nil else { return }
                    let bridge = WatchBridge(store: store, hub: hub, sample: sample)
                    bridge.start()
                    watch = bridge
                }
        }
    }
}

enum Tab: Hashable { case pcs, settings, health }

/// Screenshot routes, passed only as launch arguments (`-vitalsRoute sample
/// -vitalsTab 1`) by `simctl launch` in apps/ios/scripts/screenshots.sh, so
/// the App Store images are real captures of the Sample PC, reproducible on
/// every release. Nothing in the app writes these keys.
enum ScreenshotRoute {
    static var opensSample: Bool { UserDefaults.standard.string(forKey: "vitalsRoute") != nil }
    static var tab: Int { UserDefaults.standard.integer(forKey: "vitalsTab") }
    static var processes: Bool { UserDefaults.standard.string(forKey: "vitalsRoute") == "processes" }
}

struct RootView: View {
    @Environment(HealthStore.self) private var health
    @Environment(\.scenePhase) private var phase
    @State private var tab: Tab = .pcs
    @State private var path: [String] = ScreenshotRoute.opensSample ? [SampleMode.id] : []

    var body: some View {
        TabView(selection: $tab) {
            NavigationStack(path: $path) { MachinesView() }
                .tabItem { Label(L("nav_machines"), systemImage: "desktopcomputer") }
                .tag(Tab.pcs)
            NavigationStack { SettingsView() }
                .tabItem { Label(L("nav_settings"), systemImage: "gearshape") }
                .tag(Tab.settings)
            // Last, so the screenshot script's tab indices stay valid.
            if health.isConnected {
                NavigationStack { HealthView(onPairAgain: { tab = .settings }) }
                    .tabItem { Label(L("health_title"), systemImage: "heart.text.square") }
                    .tag(Tab.health)
            }
        }
        .onChange(of: health.isConnected, initial: true) { _, connected in
            if !connected, tab == .health { tab = .pcs }
            health.setActive(connected && phase == .active && tab == .health)
        }
        .onChange(of: phase) { _, p in health.setActive(p == .active && tab == .health) }
        .onChange(of: tab) { _, t in health.setActive(phase == .active && t == .health) }
    }
}
