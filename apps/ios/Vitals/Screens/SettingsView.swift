import SwiftUI
import VitalsCore

struct SettingsView: View {
    @Environment(PairingStore.self) private var store
    @Environment(LiveHub.self) private var hub
    @Environment(SampleMode.self) private var sample
    @Environment(HealthStore.self) private var health
    @State private var renaming: Pairing?
    @State private var newName = ""
    @State private var removing: Pairing?
    @State private var confirmDisconnect = false

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
            HealthSection(confirmDisconnect: $confirmDisconnect)
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
        .confirmationDialog(
            L("health_disconnect_confirm"), isPresented: $confirmDisconnect, titleVisibility: .visible
        ) {
            Button(L("health_disconnect"), role: .destructive) { health.disconnect() }
        }
    }
}

/// Pairing with vmui: a 4-digit code the owner approves there, or the vmui
/// account's email and password (sent once, never kept).
private struct HealthSection: View {
    @Environment(HealthStore.self) private var health
    @Binding var confirmDisconnect: Bool
    @State private var base = ""
    @State private var email = ""
    @State private var password = ""
    @State private var started = false

    private var deviceName: String { UIDevice.current.name }
    private var busy: Bool {
        switch health.pairing {
        case .starting, .signingIn, .waiting: true
        case .idle, .failed: false
        }
    }

    var body: some View {
        Section {
            if health.isConnected, health.issue != .signedOut {
                Text(L("health_connected", health.baseUrl, health.tokenPrefix ?? Format.dash))
                Button(L("health_disconnect"), role: .destructive) { confirmDisconnect = true }
            } else {
                // A revoked token: pairing again replaces it in place.
                if health.isConnected {
                    Label(L("health_signed_out"), systemImage: "key.slash").foregroundStyle(Palette.warn)
                }
                TextField(L("health_base_url"), text: $base, prompt: Text(L("health_base_url_hint")))
                    .textContentType(.URL)
                    .keyboardType(.URL)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .disabled(busy)
                pairingBody
                if health.isConnected {
                    Button(L("health_disconnect"), role: .destructive) { confirmDisconnect = true }
                }
            }
        } header: {
            Text(L("health_settings_title"))
        } footer: {
            Text(L("health_settings_body"))
        }
        .onAppear {
            if !started {
                base = health.baseUrl
                started = true
            }
        }
    }

    @ViewBuilder private var pairingBody: some View {
        switch health.pairing {
        case let .waiting(code):
            VStack(alignment: .leading, spacing: 6) {
                Text(L("health_code_title")).font(.caption).foregroundStyle(.secondary)
                Text(code).font(.largeTitle.monospacedDigit().weight(.semibold))
                    .accessibilityLabel(code.map { String($0) }.joined(separator: " "))
                Text(L("health_pair_waiting")).font(.footnote).foregroundStyle(.secondary)
            }
            HStack {
                ProgressView()
                Text(L("health_waiting")).foregroundStyle(.secondary)
            }
            Button(L("common_cancel"), role: .cancel) { health.cancelPairing() }
        case .starting, .signingIn:
            HStack {
                ProgressView()
                Text(L("health_connecting")).foregroundStyle(.secondary)
            }
            Button(L("common_cancel"), role: .cancel) { health.cancelPairing() }
        case .idle, .failed:
            if case let .failed(f) = health.pairing {
                Label(Self.message(f), systemImage: "exclamationmark.triangle").foregroundStyle(Palette.warn)
            }
            Button(L("health_pair_code")) { health.pairWithCode(rawBase: base, deviceName: deviceName) }
                .disabled(base.trimmingCharacters(in: .whitespaces).isEmpty)
            Text(L("health_or_sign_in")).font(.footnote).foregroundStyle(.secondary)
            TextField(L("health_email"), text: $email)
                .textContentType(.username)
                .keyboardType(.emailAddress)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
            SecureField(L("health_password"), text: $password)
                .textContentType(.password)
            Button(L("health_sign_in")) {
                let pw = password
                // The password leaves the field as soon as it is sent.
                password = ""
                health.signIn(rawBase: base, email: email, password: pw, deviceName: deviceName)
            }
            .disabled(base.trimmingCharacters(in: .whitespaces).isEmpty || email.isEmpty || password.isEmpty)
        }
    }

    static func message(_ f: HealthPairFailure) -> String {
        switch f {
        case .badAddress: L("health_base_url_https")
        case .unreachable: L("health_unreachable")
        case .rejected: L("health_pair_rejected")
        case .expired: L("health_pair_expired")
        case .timedOut: L("health_error_timed_out")
        case .wrongCredentials: L("health_wrong_credentials")
        case .slowDown: L("health_error_slow_down")
        case .invalid: L("health_invalid")
        }
    }
}
