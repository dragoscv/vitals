import SwiftUI
import UIKit
import VitalsCore

/// A translated string by its catalog key (the Android resource name, so the
/// two phones share wording). Arguments fill %@ / %lld placeholders.
func L(_ key: String, _ args: CVarArg...) -> String {
    let format = Bundle.main.localizedString(forKey: key, value: nil, table: nil)
    return args.isEmpty ? format : String(format: format, locale: .current, arguments: args)
}

/// The Home Screen quick action declared in project.yml
/// (`UIApplicationShortcutItems`). Its type is the only part UIKit hands the
/// app, so the string lives here as well as in the plist.
enum QuickAction {
    static let scan = "app.vitals.ios.scan"
}

/// A quick action can arrive before the SwiftUI view tree exists (a cold
/// launch from the Home Screen), so it lands in this one-shot flag that
/// MachinesView reads when it appears. Reading and clearing it happen on the
/// main actor, in the same order.
@MainActor
@Observable
final class QuickActionBridge {
    static let shared = QuickActionBridge()
    var pendingScan = false
}

/// Wires the "Scan code" Home Screen quick action. With the SwiftUI app
/// lifecycle there is no scene delegate of ours, and UIKit delivers the
/// action to the app delegate when the scene delegate does not implement
/// `windowScene(_:performActionFor:)` — which is why the handler lives here.
final class AppDelegate: NSObject, UIApplicationDelegate {
    func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil
    ) -> Bool {
        if let item = launchOptions?[.shortcutItem] as? UIApplicationShortcutItem {
            handle(item.type)
        }
        return true
    }

    func application(
        _ application: UIApplication,
        performActionFor shortcutItem: UIApplicationShortcutItem,
        completionHandler: @escaping (Bool) -> Void
    ) {
        completionHandler(handle(shortcutItem.type))
    }

    private func handle(_ type: String) -> Bool {
        guard type == QuickAction.scan else { return false }
        Task { @MainActor in QuickActionBridge.shared.pendingScan = true }
        return true
    }
}

@main
struct VitalsApp: App {
    @UIApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate
    @State private var store = PairingStore()
    @State private var hub = LiveHub()
    @State private var lan = LanMonitor()
    @State private var sample = SampleMode()
    @State private var health = HealthStore()
    @State private var watch: WatchBridge?
    @State private var notifier: AlertNotifier?
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
                    // Local notifications for PC alerts: best effort on the LAN.
                    let notifier = AlertNotifier(store: store)
                    notifier.start()
                    self.notifier = notifier
                }
        }
    }
}

enum Tab: Hashable { case pcs, settings, health }

/// Screenshot routes, passed only as launch arguments (`-vitalsRoute sample
/// -vitalsTab 1`) by `simctl launch` in apps/ios/scripts/screenshots.sh, so
/// the App Store images are real captures of the Sample PC, reproducible on
/// every release. Nothing in the app writes these keys.
///
/// `-vitalsRootTab 0|1|2` additionally selects the root tab, which the UI
/// smoke test (apps/ios/VitalsUITests) needs: on a Mac without a GPU iOS 26
/// draws the glass tab bar with only its selected item, so tapping Settings
/// there is unreliable, while a launch argument is not.
enum ScreenshotRoute {
    static var opensSample: Bool { UserDefaults.standard.string(forKey: "vitalsRoute") != nil }
    static var tab: Int { UserDefaults.standard.integer(forKey: "vitalsTab") }
    static var processes: Bool { UserDefaults.standard.string(forKey: "vitalsRoute") == "processes" }
    static var rootTab: Tab {
        switch UserDefaults.standard.integer(forKey: "vitalsRootTab") {
        case 1: .settings
        case 2: .health
        default: .pcs
        }
    }
}

struct RootView: View {
    @Environment(HealthStore.self) private var health
    @Environment(\.scenePhase) private var phase
    @State private var tab: Tab = ScreenshotRoute.rootTab
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
