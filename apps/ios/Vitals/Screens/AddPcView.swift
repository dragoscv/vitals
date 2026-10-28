@preconcurrency import AVFoundation
import SwiftUI
import UIKit
import VitalsCore

/// Add a PC: scan the QR code, pick one found nearby, type a six-digit code,
/// or type the address and token. Every path ends in PairCheck, so nothing is
/// saved until the PC has answered /health and accepted the token.
struct AddPcView: View {
    enum Mode: Hashable { case scan, nearby, code, manual }

    @Environment(PairingStore.self) private var store
    @Environment(LiveHub.self) private var hub
    @Environment(\.dismiss) private var dismiss
    @State private var mode: Mode = .scan
    @State private var busy = false
    @State private var message: String?
    @State private var scanned: PairingLink?

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                Picker("", selection: $mode) {
                    Text(L("add_scan")).tag(Mode.scan)
                    Text(L("add_nearby")).tag(Mode.nearby)
                    Text(L("add_code")).tag(Mode.code)
                    Text(L("add_manual")).tag(Mode.manual)
                }
                .pickerStyle(.segmented)
                .padding()
                Group {
                    switch mode {
                    case .scan: ScanPane { link in scanned = link }
                    case .nearby: NearbyPane(pair: pairToken)
                    case .code: CodePane(pair: pairCode)
                    case .manual: ManualPane(pair: pairToken)
                    }
                }
                .disabled(busy)
                if busy { ProgressView(L("pairing_checking")).padding() }
                if let message {
                    Text(message).font(.footnote).foregroundStyle(Palette.danger)
                        .padding().frame(maxWidth: .infinity, alignment: .leading)
                }
                Spacer(minLength: 0)
            }
            .navigationTitle(L("add_title"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button(L("common_cancel")) { dismiss() } }
            }
            .alert(L("add_confirm_title"), isPresented: Binding(get: { scanned != nil }, set: { if !$0 { scanned = nil } })) {
                Button(L("add_pair")) {
                    if let s = scanned { pairToken(s.baseUrl, s.token, nil) }
                    scanned = nil
                }
                Button(L("common_cancel"), role: .cancel) { scanned = nil }
            } message: {
                Text(URLComponents(string: scanned?.baseUrl ?? "")?.host ?? "")
            }
        }
    }

    private func pairToken(_ url: String, _ token: String, _ label: String?) {
        run { await PairCheck.withToken(rawUrl: url, rawToken: token, label: label, existing: store.pairings) }
    }

    private func pairCode(_ url: String, _ code: String, _ label: String?) {
        let device = UIDevice.current.name
        run {
            await PairCheck.withCode(
                rawUrl: url, rawCode: code, deviceLabel: device, pcLabel: label, existing: store.pairings)
        }
    }

    private func run(_ work: @escaping @MainActor () async -> PairCheck.Outcome) {
        busy = true
        message = nil
        Task { @MainActor in
            let outcome = await work()
            busy = false
            switch outcome {
            case let .ok(pairing, _):
                store.save(pairing)
                hub.forget(pairing.id)
                UINotificationFeedbackGenerator().notificationOccurred(.success)
                dismiss()
            case let .refused(reason, status):
                UINotificationFeedbackGenerator().notificationOccurred(.error)
                message = Self.words(reason, status)
            }
        }
    }

    static func words(_ r: PairRefusal, _ status: Int?) -> String {
        switch r {
        case .badAddress: L("pairing_bad_address")
        case .notPrivate: L("pairing_not_private")
        case .badToken: L("pairing_bad_token")
        case .badCode: L("pairing_bad_code")
        case .unreachable: L("pairing_unreachable")
        case .offLan: L("pairing_off_lan")
        case .unauthorised: L("pairing_unauthorised")
        case .incompatible: L("pairing_incompatible")
        case .notReady: L("pairing_not_ready")
        case .failed: status == 404 ? L("pairing_no_route") : L("pairing_failed", status ?? 0)
        }
    }
}

// MARK: Panes

struct ManualPane: View {
    let pair: (String, String, String?) -> Void
    @State private var address = ""
    @State private var token = ""
    @State private var name = ""

    var body: some View {
        Form {
            TextField(L("add_address"), text: $address, prompt: Text(L("add_address_hint")))
                .keyboardType(.URL).textInputAutocapitalization(.never).autocorrectionDisabled()
            SecureField(L("add_token"), text: $token)
                .textInputAutocapitalization(.never).autocorrectionDisabled()
            TextField(L("add_name"), text: $name)
            Button(L("add_pair")) { pair(address, token, name.isEmpty ? nil : name) }
                .disabled(address.isEmpty || token.isEmpty)
        }
    }
}

struct CodePane: View {
    let pair: (String, String, String?) -> Void
    @State private var address = ""
    @State private var code = ""
    @State private var name = ""

    var body: some View {
        Form {
            Section { Text(L("add_code_hint")).font(.footnote).foregroundStyle(.secondary) }
            TextField(L("add_address"), text: $address, prompt: Text(L("add_address_hint")))
                .keyboardType(.URL).textInputAutocapitalization(.never).autocorrectionDisabled()
            TextField(L("add_code_digits"), text: $code)
                .keyboardType(.numberPad).textContentType(.oneTimeCode)
                .font(.title2.monospacedDigit())
            TextField(L("add_name"), text: $name)
            Button(L("add_pair")) { pair(address, code, name.isEmpty ? nil : name) }
                .disabled(address.isEmpty || !PairingRules.isCode(code))
        }
    }
}

struct NearbyPane: View {
    let pair: (String, String, String?) -> Void
    @State private var discovery = Discovery()
    @State private var chosen: NearbyPC?
    @State private var token = ""

    var body: some View {
        List {
            if discovery.found.isEmpty {
                HStack { ProgressView(); Text(L("add_nearby_empty")).foregroundStyle(.secondary) }
            }
            ForEach(discovery.found) { pc in
                Button { chosen = pc } label: {
                    VStack(alignment: .leading) {
                        Text(pc.name)
                        Text(pc.baseUrl).font(.caption).foregroundStyle(.secondary)
                    }
                }
            }
        }
        .onAppear { discovery.start() }
        .onDisappear { discovery.stop() }
        .alert(chosen?.name ?? "", isPresented: Binding(get: { chosen != nil }, set: { if !$0 { chosen = nil } })) {
            SecureField(L("add_token"), text: $token)
            Button(L("add_pair")) {
                if let c = chosen { pair(c.baseUrl, token, c.name) }
                token = ""
            }
            Button(L("common_cancel"), role: .cancel) { token = "" }
        } message: {
            Text(L("add_nearby_token"))
        }
    }
}

struct ScanPane: View {
    let onLink: (PairingLink) -> Void
    @State private var status = AVCaptureDevice.authorizationStatus(for: .video)

    var body: some View {
        VStack(spacing: 12) {
            Text(L("add_scan_hint")).font(.footnote).foregroundStyle(.secondary).padding(.horizontal)
            switch status {
            case .authorized:
                QrScanner(onCode: { text in
                    if let link = PairingLink.parse(text) { onLink(link) }
                })
                .clipShape(RoundedRectangle(cornerRadius: 16))
                .padding(.horizontal)
                .frame(maxHeight: 360)
            case .notDetermined:
                ProgressView().task {
                    _ = await AVCaptureDevice.requestAccess(for: .video)
                    status = AVCaptureDevice.authorizationStatus(for: .video)
                }
            default:
                Text(L("add_camera_denied")).padding()
                Button(L("add_open_settings")) {
                    if let u = URL(string: UIApplication.openSettingsURLString) { UIApplication.shared.open(u) }
                }
            }
        }
    }
}

/// Live camera preview that reports each QR code once.
struct QrScanner: UIViewControllerRepresentable {
    let onCode: (String) -> Void

    func makeUIViewController(context: Context) -> ScannerController {
        let c = ScannerController()
        c.onCode = onCode
        return c
    }

    func updateUIViewController(_ controller: ScannerController, context: Context) {}
}

final class ScannerController: UIViewController, AVCaptureMetadataOutputObjectsDelegate {
    var onCode: ((String) -> Void)?
    private let session = AVCaptureSession()
    private var seen = Set<String>()
    private var preview: AVCaptureVideoPreviewLayer?

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .black
        guard let device = AVCaptureDevice.default(for: .video),
              let input = try? AVCaptureDeviceInput(device: device),
              session.canAddInput(input) else {
            let label = UILabel()
            label.text = L("add_camera_unavailable")
            label.textColor = .white
            label.numberOfLines = 0
            label.frame = view.bounds.insetBy(dx: 16, dy: 16)
            view.addSubview(label)
            return
        }
        session.addInput(input)
        let output = AVCaptureMetadataOutput()
        guard session.canAddOutput(output) else { return }
        session.addOutput(output)
        output.setMetadataObjectsDelegate(self, queue: .main)
        output.metadataObjectTypes = [.qr]
        let layer = AVCaptureVideoPreviewLayer(session: session)
        layer.videoGravity = .resizeAspectFill
        view.layer.addSublayer(layer)
        preview = layer
    }

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        preview?.frame = view.bounds
    }

    override func viewWillAppear(_ animated: Bool) {
        super.viewWillAppear(animated)
        // startRunning blocks; AVCaptureSession is documented thread-safe.
        nonisolated(unsafe) let s = session
        DispatchQueue.global(qos: .userInitiated).async { s.startRunning() }
    }

    override func viewWillDisappear(_ animated: Bool) {
        super.viewWillDisappear(animated)
        session.stopRunning()
    }

    nonisolated func metadataOutput(
        _ output: AVCaptureMetadataOutput, didOutput objects: [AVMetadataObject], from connection: AVCaptureConnection
    ) {
        let codes = objects.compactMap { ($0 as? AVMetadataMachineReadableCodeObject)?.stringValue }
        MainActor.assumeIsolated {
            for code in codes where !seen.contains(code) {
                seen.insert(code)
                onCode?(code)
            }
        }
    }
}
