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
    @Environment(\.scenePhase) private var phase

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(store)
                .environment(hub)
                .environment(lan)
                .environment(sample)
                .tint(Palette.accent)
        }
    }
}

enum Tab: Hashable { case pcs, settings }

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
        }
    }
}
