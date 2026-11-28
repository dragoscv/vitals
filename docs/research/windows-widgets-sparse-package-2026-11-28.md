> Status: research snapshot, 2026-11-28. Not normative — decisions belong in
> `docs/TRACKER.md` and the ADRs. Every load-bearing claim carries a URL and is
> marked **VERIFIED** (read in the source named) or **EXPECTED** (reasoned, not
> yet run). Where the documentation is silent or ambiguous that is said plainly
> and the experiment that would settle it is named. Nothing here was installed or
> run on a machine.

# Can an unpackaged Vitals ship a Windows 11 widget — the sparse-package route

**Question.** Vitals installs unpackaged (NSIS installers from Tauri). A widget
provider on the Windows 11 Widgets Board must be a _packaged_ app with package
identity. Can an unpackaged install still get a real widget on the board, and if
so what is the minimum cost? Specifically: does the **sparse package**
("packaging with external location") route work, what does it require, and does
Vitals have those things today?

This note follows the prior art in this repository,
[`windows-widgets-2026-11-28.md`](windows-widgets-2026-11-28.md) (S26-47), which
established _what the board permits_ and left exactly one open question: whether
the Widgets host honours a provider registered from a **sparse** package. That is
the question this note answers.

---

## Verdict

**Conditional yes — the mechanism is real and Microsoft-documented, but it is not
proven for the Widgets host specifically, and it needs one thing Vitals does not
have today: a code-signing certificate.**

- **The building blocks are all documented and current.** A sparse package grants
  package identity to an unpackaged app and explicitly supports _app extensions_
  and a _Windows App SDK framework dependency_; a widget provider is nothing but an
  app extension plus a COM server. Each of those is a **VERIFIED** Microsoft
  statement (URLs in §2 and §3).
- **The single unverified link is the Widgets host itself.** No Microsoft page
  states that the board enumerates a provider declared by a _sparse_ package, and
  every Microsoft widget sample is a **full** MSIX. So the join is **EXPECTED**, not
  verified — and the honest answer is "reachable, contingent on a one-day spike".
- **What Vitals does not have today** (so the answer is not a plain "yes"):
  a code-signing certificate (§5), a native widget-provider binary (§4), and the
  Windows App SDK runtime on the machine or carried app-local (§5).
- **What it does _not_ need:** a Partner Center identity for this route, and a
  separate package per architecture (§5).

A reader who only wants the decision: **do the spike in §6 before scoping the
feature; if it passes, the minimum is a sparse identity package (one neutral
architecture, ~a few hundred KB) plus a small provider executable, and no change
to the NSIS install except registering and unregistering the identity package.**

---

## 1. The requirement, restated with its source

- **A Win32 widget provider must be a packaged app.** The manifest-format page
  says verbatim: _"For Win32 apps, only packaged apps are currently supported and
  widget providers specify their registration information in the app package
  manifest file."_ VERIFIED —
  [widget provider manifest format](https://learn.microsoft.com/en-us/windows/apps/develop/widgets/widget-provider-manifest).
  The providers page repeats it: _"Currently you can implement a widget provider
  using a packaged Win32 desktop app or a Progressive Web App (PWA)."_ VERIFIED —
  [Widget providers](https://learn.microsoft.com/en-us/windows/apps/develop/widgets/widget-providers).
- **Registration is two manifest extensions**, both inside the
  `<Application>` element: a `com:Extension Category="windows.comServer"` with a
  `com:ExeServer`/`com:Class` (the CLSID the host activates with
  `CoCreateInstance`), and a `uap3:Extension Category="windows.appExtension"`
  whose `uap3:AppExtension` has `Name="com.microsoft.windows.widgets"`. VERIFIED —
  [Implement a widget provider in a win32 app (C++/WinRT)](https://learn.microsoft.com/en-us/windows/apps/develop/widgets/implement-widget-provider-win32).
- **Third-party widgets arrived in Windows App SDK 1.2**, which states _"Widgets
  can only be created for packaged, Win32 apps."_ VERIFIED —
  [Windows App SDK 1.2 release notes](https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/release-notes/windows-app-sdk-1-2).
- **`com:ExeServer`'s `Executable` must reference a file in the package.** VERIFIED
  — [com:ExeServer](https://learn.microsoft.com/en-us/uwp/schemas/appxpackage/uapmanifestschema/element-com-exeserver):
  _"A path relative to the package root and must reference a file in the package."_
  This is the sentence that makes the sparse route non-trivial, and §3 explains how
  external location satisfies it.

This note assumes the _content_ limit the prior art already found — a static
Adaptive Card in one of three sizes, with the animated (HTML) route restricted to
the EEA. VERIFIED —
[Windows widgets design](https://learn.microsoft.com/en-us/windows/apps/design/widgets/).

---

## 2. What a sparse package actually is, and what it grants

"Packaging with external location" (a **sparse package**) is an MSIX that contains
only a manifest and grants an already-installed unpackaged app **package identity
at runtime**, without moving its binaries into the package and without changing
its installer. VERIFIED —
[Grant package identity by packaging with external location](https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/grant-identity-to-nonpackaged-apps-overview).
It was introduced in Windows 10, version 2004 (build 19041). VERIFIED — same page
and the [2019 Windows blog that introduced it](https://blogs.windows.com/windowsdeveloper/2019/10/29/identity-registration-and-activation-of-non-packaged-win32-apps/).

The load-bearing facts for a widget:

1. **Identity is exactly what a widget provider is refused without.** The feature
   list that identity unlocks explicitly includes **app extensions**. VERIFIED —
   [Grant package identity … manually](https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/grant-identity-to-nonpackaged-apps)
   ("…unlocks Windows platform features that are otherwise unavailable to
   unpackaged apps: toast and push notifications, background tasks, **app
   extensions**, share targets, file associations, startup tasks…").
2. **App extensions are supported for external-location packages.** The extensions
   page states, in a note, that its features work for _"packaged apps **and
   packaged apps with external location**"_. VERIFIED —
   [Integrate your desktop app with Windows using packaging extensions](https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/desktop-to-uwp-extensions).
3. **A sparse package can carry a Windows App SDK framework dependency.**
   _"Packaged apps, including apps packaged with external location (sparse
   packages), can declare the Framework dependency statically with
   `<PackageDependency>` in AppxManifest.xml."_ VERIFIED —
   [WASDK deployment guide for framework-dependent packaged apps](https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/deploy-packaged-apps).
   Microsoft ships a working sample of exactly this —
   [`cs-winforms-sparse`](https://github.com/microsoft/WindowsAppSDK-Samples/tree/main/Samples/WindowsAIFoundry/cs-winforms-sparse)
   — whose
   [`AppxManifest.xml`](https://raw.githubusercontent.com/microsoft/WindowsAppSDK-Samples/main/Samples/WindowsAIFoundry/cs-winforms-sparse/WinForms/AppxManifest.xml)
   shows `uap10:AllowExternalContent`, `runFullTrust`,
   `unvirtualizedResources`, `uap10:TrustLevel="mediumIL"`,
   `uap10:RuntimeBehavior="win32App"` and a `<PackageDependency>` on
   `Microsoft.WindowsAppRuntime.*`. VERIFIED (read).

The sparse manifest's required shape, from the manual page (VERIFIED, same URL as
§2.1): `Identity` with `Name`/`Publisher`/`Version`/`ProcessorArchitecture="neutral"`;
`Properties` with `<uap10:AllowExternalContent>true</uap10:AllowExternalContent>`;
`Capabilities` with `runFullTrust` **and** `unvirtualizedResources`; and one
`Application` per executable with `uap10:TrustLevel="mediumIL"` and
`uap10:RuntimeBehavior="win32App"`.

**Why "must reference a file in the package" (§1) is satisfied by external
location.** The sparse package's whole purpose is that _"the manifest can
reference files outside the package in a predetermined 'external location'"_
(2019 blog, VERIFIED). The external location is declared at registration and is the
app's own install directory (the sample README requires the external location to be
_"the same as its file location"_). So a package-relative path like
`vitals-widget-provider.exe` resolves to
`<install-dir>\vitals-widget-provider.exe`. This is how the community plan in §3
reads the rule; it is **EXPECTED** that the COM server resolves the same way, and it
is the second thing the spike in §6 must prove.

---

## 3. The one unverified link: does the Widgets host honour a sparse provider?

This is the crux, and it must be stated honestly.

- **No Microsoft page says it.** Searches of the widget provider, manifest,
  ActivateApplication, deployment and identity pages find no statement about a
  _sparse_ widget provider. Every Microsoft widget sample — the
  [Widgets samples](https://github.com/microsoft/WindowsAppSDK-Samples/tree/main/Samples/Widgets)
  README included — is a **full** MSIX ("a packaged application"). VERIFIED (read).
- **The reasoning that it should work.** A widget is registered through the same
  app-extension mechanism that §2.2 says works for external-location packages, and
  the identity the host needs is granted. The board discovers providers by
  enumerating the app extension, not by requiring a full package. That chain is
  sound but **EXPECTED**, not documented.
- **A community data point exists, and it is a plan, not a proof.** The
  `NetfallNetworks/windows-image-viewer` repository carries a design document that
  describes precisely this: a _"sparse MSIX 'identity package' that sits alongside
  the existing MSI installer"_, built with `makeappx` and signed with a self-signed
  cert for dev, declaring `windows.comServer` + `windows.widgetProvider`, with the
  acceptance criterion _"widget appears in Win+W Add widgets panel (manual test)"_.
  VERIFIED (read the document) —
  [widget-phase-2-stories.md](https://github.com/NetfallNetworks/windows-image-viewer/blob/main/plan/widget-phase-2-stories.md).
  **But it is a planning document**; it is not evidence the widget host enabled it.
  Cite it as "someone is attempting exactly this", never as proof.

**What would settle it** (the spike, §6): build the Microsoft
[Widgets sample](https://github.com/microsoft/WindowsAppSDK-Samples/tree/main/Samples/Widgets)
provider as a full MSIX, confirm it appears in the picker, then rebuild the _same_
provider as a **sparse** package against an external install and see whether it
appears. Appears → the NSIS route is open; does not → the board is MSIX-only and
the widget exists only for the Store build.

---

## 4. What the widget provider itself costs to build

Independent of packaging, the provider is new native code:

- **It is a COM local server, not the Tauri webview.** The host activates the CLSID
  with `CoCreateInstance`; the provider registers a class factory with
  `CoRegisterClassObject(..., CLSCTX_LOCAL_SERVER, ...)` and blocks on a shutdown
  event. VERIFIED —
  [Implement a widget provider (C++/WinRT)](https://learn.microsoft.com/en-us/windows/apps/develop/widgets/implement-widget-provider-win32).
  A Tauri webview exposes no COM class factory and cannot render an Adaptive Card,
  so the provider cannot be the React layer. EXPECTED (reasoned from the
  documented activation model; consistent with the prior art note).
- **Can the provider process be the already-running Vitals app?** No — not as the
  COM server, and the answer splits in two:
  - _As the COM server process:_ the manifest names **one** executable for the
    `com:ExeServer`, and the host launches that fresh and calls
    `CoRegisterClassObject`. Pointing it at `vitals-desktop.exe` would mean the
    Tauri process registering a class object and blocking its message loop on
    `CoWaitForMultipleObjects` — at odds with Tauri's own single-instance/event-loop
    design. EXPECTED.
  - _As the source of the data:_ yes. A small dedicated provider executable can read
    the live numbers from the **already-running** app over its existing local
    surface (the LAN server's `/summary`, loopback scope — see the prior-art note
    and `crates/vitals-server/src/router.rs`). The provider process is separate; the
    _data_ is the running app's. VERIFIED (in-repo, from the prior-art note).
  - So the minimal design is a **separate tiny provider binary** (Rust + `windows-rs`
    `#[implement]` is plausible; C++/WinRT is the documented path), shipped next to
    the app, that implements `IWidgetProvider` and pushes an Adaptive Card built
    from `/summary`.
- **Runtime dependency.** The provider links `Microsoft.Windows.Widgets.Providers`,
  so it needs the Windows App SDK runtime — either framework-dependent (the
  machine must have the `Microsoft.WindowsAppRuntime.*` framework package, which a
  sparse package can declare via `<PackageDependency>`, §2.3) or self-contained
  (binaries carried app-local, larger). VERIFIED —
  [WASDK deployment for framework-dependent packaged apps](https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/deploy-packaged-apps)
  and
  [self-contained](https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/deploy-unpackaged-apps).
- **Picker assets.** Each widget needs a provider icon and a **screenshot**; the
  picker screenshot is 300×304 px with rounded transparent corners, and optional
  light/dark variants. VERIFIED —
  [Integrate with the widget picker](https://learn.microsoft.com/en-us/windows/apps/design/widgets/widgets-picker-integration).
- **Alternative activation (not recommended).** `ActivateApplication` launches the
  app with a `--widget-call=<base64url>` argument instead of COM. The docs recommend
  `CreateInstance`, and the argument is explicitly _"not recommended for most widget
  provider implementations"_. VERIFIED —
  [Widget provider ActivateApplication protocol](https://learn.microsoft.com/en-us/windows/apps/develop/widgets/widget-provider-activateapplication-protocol).
  It does **not** remove the Windows App SDK dependency (the provider still calls
  `WidgetManager`), so it is not a shortcut.

---

## 5. Prerequisites Vitals does NOT have today

This is the list a reader needs to decide "can we do it now?".

1. **A code-signing certificate — the hard prerequisite.**
   - A sparse package _"must be signed with a cert that is trusted on that
     machine"_, exactly like any MSIX. VERIFIED — manual page and 2019 blog (§2).
   - A self-signed certificate is enough **for local testing only**, and only after
     its public `.cer` is imported into
     `Cert:\CurrentUser\TrustedPeople` (else `CERT_E_UNTRUSTEDROOT`, 0x800B0109).
     VERIFIED — manual page, troubleshooting table.
   - For **distribution**, a certificate issued by a trusted CA (or a service such
     as Azure Trusted Signing) is required. VERIFIED — same page.
   - **Vitals is unsigned today.** No Authenticode certificate exists; the NSIS
     installers are built unsigned and the workflow prints a notice saying so.
     VERIFIED (in-repo) — `docs/distribution.md` (§"What actually removes the
     warning"), `docs/signing-and-store-identity.md` (row: _"currently unsigned"_,
     `release.yml:470`), `apps/desktop/src-tauri/tauri.conf.json` (no
     `certificateThumbprint`/`signCommand`). The free-for-OSS route is SignPath
     Foundation, and Azure Trusted Signing is _"not offered to individual
     developers outside the US and Canada"_ — both stated in-repo in
     `docs/distribution.md`. Whether SignPath will countersign an **MSIX identity
     package** (as opposed to an `.exe`) is **EXPECTED-uncertain** and worth asking
     them before relying on it.
2. **A non-Store identity (name + publisher).** For the sparse route you choose the
   `Identity` `Name` yourself and set `Publisher` to **the subject of your signing
   certificate**. VERIFIED — manual page (_"Publisher must match the Subject of the
   certificate used to sign"_) and 2019 blog. This is **independent of Partner
   Center** — a Partner Center identity is needed for a **Store** MSIX (which Vitals
   already has: reserved product `9P95RK5VMGPJ`, publisher
   `CN=FE8B5839-…`, `packaging/msix/build-msix.ps1` and
   `AppxManifest.template.xml`, VERIFIED in-repo), but not for a self-distributed
   sparse package. Keep the two identities separate; do not try to reuse the Store
   publisher (Vitals does not hold that certificate — the Store does).
3. **The Windows App SDK runtime.** Either the framework package must be present on
   the machine (declarable via `<PackageDependency>`, §2.3) or the provider is
   self-contained. Vitals ships neither today. VERIFIED (in-repo: no WASDK
   dependency anywhere in `apps/desktop/`; the app is Rust/Tauri, not a WASDK app).
4. **A new native provider binary** (§4). Vitals has no COM widget provider, no
   `IWidgetProvider` implementation, and no CLSID. VERIFIED (in-repo: the prior-art
   note and `rg` over the repo find no such component).
5. **A Win32 application manifest with the `<msix>` element**, if any Vitals
   executable must run _with identity_ through `CreateProcess` (rather than COM
   activation). The `publisher`/`packageName`/`applicationId` values must match the
   sparse manifest. VERIFIED — manual page §"Add identity metadata" and the 2019
   blog. The COM-activation path gives the provider its identity via the manifest,
   so this is required for the _main_ app if we want it identified, not strictly
   for the provider. Marked as a decision, not a blocker.

**Things that are NOT prerequisites (so they do not inflate the estimate):**

- **No Partner Center identity** for the sparse route (§5.2). VERIFIED-by-reasoning
  from the manual page (you choose the fields).
- **No per-architecture package.** The sparse identity package is
  `ProcessorArchitecture="neutral"` and works across x64/arm64/x86. VERIFIED —
  manual page (_"should be neutral as shown so the identity package works across all
  architectures"_). This is a real cost saving over the Store MSIX path, which
  builds one package per architecture (`packaging/msix/build-msix.ps1`, VERIFIED).
- **No Developer Mode on Windows 11** for a package signed by a trusted
  certificate — sideloading is on by default since Windows 10 2004. VERIFIED —
  [WASDK unpackaged deployment](https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/deploy-unpackaged-apps)
  (_"Sideloading is automatically enabled on Windows 10 version 2004 and later"_).
  (The Microsoft Widgets sample README says _"Sideloading … requires Developer Mode"_,
  which describes its unsigned dev workflow, not the signed distribution case —
  the two statements are reconciled by trust: a trusted-signed package needs no
  Developer Mode, an untrusted one does.)

---

## 6. Recommended next step — the cheapest yes/no

**One-day spike, on a real Windows 11 machine, before any feature is scoped.**

1. Build the Microsoft
   [Widgets sample](https://github.com/microsoft/WindowsAppSDK-Samples/tree/main/Samples/Widgets)
   and deploy it as a full MSIX; confirm a widget appears in **Win+W → Add
   widgets**. This proves the toolchain and the board on the machine.
2. Rebuild the _same_ provider as a **sparse** package: a manifest built from the
   [`cs-winforms-sparse`](https://github.com/microsoft/WindowsAppSDK-Samples/tree/main/Samples/WindowsAIFoundry/cs-winforms-sparse)
   shape (which carries `AllowExternalContent` and a `<PackageDependency>`),
   signed with a **self-signed** certificate whose subject matches `Publisher`, the
   `.cer` imported into `TrustedPeople`, registered with
   `Add-AppxPackage -ExternalLocation <install-dir>` pointing at the folder that
   already holds the provider `.exe`.
3. **The pass/fail test:** does the widget appear in **Win+W → Add widgets**, and
   does pinning it start the provider and render a card?
   - **Appears and runs → the NSIS route is open.** Then scope the feature:
     sparse identity package (neutral, one file) + a tiny provider binary + the
     certificate from §5.1 + registration wired into the NSIS installer/uninstaller
     (`apps/desktop/src-tauri/windows/hooks.nsh`).
   - **Does not appear → the board is MSIX-only.** The widget is then a
     Store-build-only feature, which is the prior-art note's conclusion, and the
     sparse work is dropped.

The prototype skeleton to run step 2 already sits under
`packaging/msix/sparse/` — see its README. **It cannot be installed as it stands**:
it has no certificate and its `Identity`/`Publisher`/CLSID are placeholders. It is
a correct _shape_, not a tested artefact.

**A second, cheap experiment** should run in the same day: register the sparse
package with the provider **absent**, then `Get-AppxPackage` and open the picker.
That isolates "does the host see a sparse registration at all" from "does COM
activation of an external exe succeed" — the two failure modes are otherwise
indistinguishable in step 3.

---

## 7. Cost summary (EXPECTED engineering judgement — not measured)

| Slice                                                           | Effort   | Notes                                                                                               |
| --------------------------------------------------------------- | -------- | --------------------------------------------------------------------------------------------------- |
| **Spike** (§6) — full MSIX then sparse, on a real machine       | **S–M**  | No new product code; sample + cert + two registrations. The cheapest possible yes/no.               |
| **Certificate acquisition**                                     | **—**    | Not engineering: a funding/eligibility step (§5.1). Blocks distribution, not the spike.             |
| **Minimal provider** — one static Adaptive Card from `/summary` | **L–XL** | New native COM server (Rust `#[implement]` or C++/WinRT), WASDK dependency, card template.          |
| **Sparse identity package** — manifest + build/register scripts | **S**    | One neutral package; `makeappx /nv` + `signtool`; register/unregister in NSIS.                      |
| **Picker art** — icon + 300×304 screenshot, light/dark          | **S**    | Required by the manifest (§4).                                                                      |
| **Multiple widgets / customisation**                            | **+M**   | Manifest definitions + `IWidgetProvider2`.                                                          |
| **Animation / custom layouts**                                  | **N/A**  | Not expressible on the board; the animated route (web widgets) is EEA-only. See the prior-art note. |

**Risks**

- **The sparse link is unproven** (§3) — the whole route is contingent on the
  spike. This is the dominant risk.
- **COM activation of an external executable** must resolve through the external
  location (§2). If it does not, the provider must live _inside_ the package, which
  forces a full MSIX and closes the NSIS route.
- **The runtime dependency** (§5.3): if a machine lacks the WASDK framework package
  and framework-dependent deployment is chosen, the provider silently fails to
  start. Self-contained avoids this at a size cost.
- **Certification of the MSIX by SignPath** is unconfirmed (§5.1) — a distribution
  risk, not a spike risk.

---

## 8. In-repo facts this note relies on (VERIFIED, read)

- `docs/research/windows-widgets-2026-11-28.md` — the prior art; leaves the sparse
  question open.
- `packaging/msix/AppxManifest.template.xml` and `packaging/msix/build-msix.ps1` —
  the existing **full-MSIX** path: a `runFullTrust` Win32 application with
  `Windows.FullTrustApplication`, two `windows.startupTask` extensions, and
  `Identity`/`Publisher` placeholders filled from Partner Center. It contains **no**
  `com.microsoft.windows.widgets` extension and **no** `AllowExternalContent` — so
  the widget is not started there either.
- `packaging/msix/build-and-upload.ps1` — one package per architecture (x64 + arm64),
  signed by the Store, unsigned locally.
- `apps/desktop/src-tauri/tauri.conf.json` — `bundle.targets: ["nsis"]`, no code
  signing, `publisher: "Dragos Catalin Vladulescu"` (the _display_ name, not a cert
  subject).
- `docs/distribution.md`, `docs/signing-and-store-identity.md`,
  `.github/workflows/release.yml` — Vitals is unsigned; Store MSIX is the only
  warning-free install; Azure Trusted Signing unavailable to individual
  non-US/Canada; SignPath Foundation is the free-for-OSS route.

---

## 9. Sources (read this session)

Microsoft Learn:

- Widget providers — <https://learn.microsoft.com/en-us/windows/apps/develop/widgets/widget-providers>
- Widget provider package manifest XML format — <https://learn.microsoft.com/en-us/windows/apps/develop/widgets/widget-provider-manifest>
- Implement a widget provider in a win32 app (C++/WinRT) — <https://learn.microsoft.com/en-us/windows/apps/develop/widgets/implement-widget-provider-win32>
- Widget provider ActivateApplication protocol — <https://learn.microsoft.com/en-us/windows/apps/develop/widgets/widget-provider-activateapplication-protocol>
- Integrate with the widget picker — <https://learn.microsoft.com/en-us/windows/apps/design/widgets/widgets-picker-integration>
- Windows widgets (design, EEA note) — <https://learn.microsoft.com/en-us/windows/apps/design/widgets/>
- Windows App SDK 1.2 release notes — <https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/release-notes/windows-app-sdk-1-2>
- Grant package identity by packaging with external location (overview) — <https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/grant-identity-to-nonpackaged-apps-overview>
- Grant package identity by packaging with external location manually — <https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/grant-identity-to-nonpackaged-apps>
- Features that require package identity — <https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/modernize-packaged-apps>
- Integrate your desktop app with Windows using packaging extensions — <https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/desktop-to-uwp-extensions>
- com:ComServer — <https://learn.microsoft.com/en-us/uwp/schemas/appxpackage/uapmanifestschema/element-com-comserver>
- com:ExeServer — <https://learn.microsoft.com/en-us/uwp/schemas/appxpackage/uapmanifestschema/element-com-exeserver>
- WASDK deployment guide for framework-dependent packaged apps — <https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/deploy-packaged-apps>
- WASDK deployment guide for framework-dependent unpackaged apps — <https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/deploy-unpackaged-apps>

Microsoft blog and samples:

- Identity, Registration and Activation of Non-packaged Win32 Apps (2019) — <https://blogs.windows.com/windowsdeveloper/2019/10/29/identity-registration-and-activation-of-non-packaged-win32-apps/>
- Windows App SDK Widgets samples — <https://github.com/microsoft/WindowsAppSDK-Samples/tree/main/Samples/Widgets>
- Windows App SDK sparse-package sample (`cs-winforms-sparse`) — <https://github.com/microsoft/WindowsAppSDK-Samples/tree/main/Samples/WindowsAIFoundry/cs-winforms-sparse>

Community (indicative only, not authoritative):

- NetfallNetworks `windows-image-viewer` — sparse widget-provider plan — <https://github.com/NetfallNetworks/windows-image-viewer/blob/main/plan/widget-phase-2-stories.md>

In-repo: `docs/research/windows-widgets-2026-11-28.md`, `docs/distribution.md`,
`docs/signing-and-store-identity.md`, `packaging/msix/*`,
`apps/desktop/src-tauri/tauri.conf.json`, `.github/workflows/release.yml`.

---

## 10. Not verified / open questions

- **Whether the Widgets host enumerates a provider from a sparse package.** Not
  documented; the whole route hinges on it. The spike in §6 answers it.
- **Whether `com:ExeServer`'s `Executable` resolves through the external location.**
  The `com:ExeServer` page says _"must reference a file in the package"_; the sparse
  mechanism says external files are referenced as if in the package. The two
  statements are compatible but not explicitly reconciled for COM servers. Second
  thing the spike must prove.
- **Whether a `<PackageDependency>` on the WASDK framework package works _and_ the
  COM server starts framework-dependent.** Documented for the dependency; not
  documented jointly with a COM server.
- **Whether SignPath Foundation will countersign an MSIX identity package.** Ask
  them; affects distribution, not the spike.
- **Whether the board throttles updates.** No numeric cap found in the pages read;
  only generic battery guidance (already noted in the prior art).
