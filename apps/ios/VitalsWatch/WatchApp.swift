// Vitals on the Apple Watch: the PCs the iPhone already watches, sent over
// WatchConnectivity by the phone's WatchBridge. No pairing, no network of
// its own — the watch never holds a token or a PC's address.

import SwiftUI
import VitalsCore

/// A translated string by its catalog key: the same Localizable.xcstrings as
/// the iPhone app (built into this target by project.yml).
func L(_ key: String, _ args: CVarArg...) -> String {
    let format = Bundle.main.localizedString(forKey: key, value: nil, table: nil)
    return args.isEmpty ? format : String(format: format, locale: .current, arguments: args)
}

@main
struct VitalsWatchApp: App {
    @State private var model = WatchModel()
    @Environment(\.scenePhase) private var phase

    var body: some Scene {
        WindowGroup {
            NavigationStack { PcListView() }
                .environment(model)
                .tint(WatchPalette.accent)
                .task { model.start() }
                .onChange(of: phase) { _, p in
                    if p == .active { model.requestLatest() }
                }
        }
    }
}
