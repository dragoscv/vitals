import SwiftUI
import VitalsCore

struct SettingsView: View {
    @Environment(PairingStore.self) private var store
    @Environment(LiveHub.self) private var hub
    @Environment(SampleMode.self) private var sample
    @State private var renaming: Pairing?
    @State private var newName = ""
    @State private var removing: Pairing?

    private var version: String {
        Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? Format.dash
    }

    var body: some View {
        Form {
            Section(L("settings_pcs")) {
                if store.pairings.isEmpty { Text(L("machines_empty_title")).foregroundStyle(.secondary) }
                ForEach(store.pairings) { p in
                    VStack(alignment: .leading, spacing: 4) {
                        Text(p.label).font(.headline)
                        Text(p.baseUrl).font(.caption).foregroundStyle(.secondary)
                        Text(p.scope == .read ? L("settings_read_only") : L("settings_can_control"))
                            .font(.caption)
                    }
                    .swipeActions {
                        Button(L("settings_remove"), role: .destructive) { removing = p }
                        Button(L("settings_rename")) { newName = p.label; renaming = p }
                    }
                    .contextMenu {
                        Button(L("settings_rename")) { newName = p.label; renaming = p }
                        Button(L("settings_remove"), role: .destructive) { removing = p }
                    }
                }
                Toggle(L("sample_title"), isOn: Binding(get: { sample.enabled }, set: { sample.enabled = $0 }))
            }
            Section(L("settings_language")) {
                Button(L("settings_language")) {
                    // iOS keeps the per-app language in Settings > Vitals.
                    if let u = URL(string: UIApplication.openSettingsURLString) { UIApplication.shared.open(u) }
                }
            }
            Section(L("local_network_title")) {
                Text(L("local_network_body")).font(.footnote).foregroundStyle(.secondary)
            }
            Section(L("settings_about")) {
                Text(L("settings_version", version))
                Text(L("settings_about_body_ios")).font(.footnote).foregroundStyle(.secondary)
                Link(L("settings_privacy"), destination: URL(string: "https://vitals.dragoscatalin.ro/privacy/")!)
                Link(L("settings_source"), destination: URL(string: "https://github.com/dragoscv/vitals")!)
                VStack(alignment: .leading, spacing: 4) {
                    Text(L("settings_licences"))
                    Text(L("settings_licences_body_ios")).font(.caption).foregroundStyle(.secondary)
                }
            }
        }
        .navigationTitle(L("settings_title"))
        .alert(L("settings_rename"), isPresented: Binding(get: { renaming != nil }, set: { if !$0 { renaming = nil } })) {
            TextField(L("add_name"), text: $newName)
            Button(L("common_save")) { if let r = renaming { store.rename(r.id, to: newName) } }
            Button(L("common_cancel"), role: .cancel) {}
        }
        .confirmationDialog(
            L("settings_remove_confirm", removing?.label ?? ""),
            isPresented: Binding(get: { removing != nil }, set: { if !$0 { removing = nil } }),
            titleVisibility: .visible
        ) {
            Button(L("settings_remove"), role: .destructive) {
                if let r = removing { store.remove(r.id); hub.forget(r.id) }
            }
        }
    }
}
