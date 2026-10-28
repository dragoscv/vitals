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

struct RootView: View {
    @State private var tab: Tab = .pcs

    var body: some View {
        TabView(selection: $tab) {
            NavigationStack { MachinesView() }
                .tabItem { Label(L("nav_machines"), systemImage: "desktopcomputer") }
                .tag(Tab.pcs)
            NavigationStack { SettingsView() }
                .tabItem { Label(L("nav_settings"), systemImage: "gearshape") }
                .tag(Tab.settings)
        }
    }
}
