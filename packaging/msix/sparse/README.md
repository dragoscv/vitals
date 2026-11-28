# Sparse identity package — prototype sketch (NOT ready to install)

Part of the spike in
[`docs/research/windows-widgets-sparse-package-2026-11-28.md`](../../../docs/research/windows-widgets-sparse-package-2026-11-28.md),
which asks whether an **unpackaged** Vitals install can put a real widget on the
Windows 11 Widgets Board via a **sparse package** ("packaging with external
location").

**This folder is a shape, not a tested artefact. Nothing here was installed or
run.** It exists so the spike has something concrete to edit. It **cannot be
installed as it stands**, for two reasons:

1. It is **unsigned**. A sparse package must be signed by a certificate _trusted
   on the target machine_ or `Add-AppxPackage` fails with
   `CERT_E_UNTRUSTEDROOT` (0x800B0109). Vitals holds no code-signing certificate
   today (`docs/distribution.md`).
2. Its `Identity` / `Publisher` / CLSID are **placeholders** and
   `vitals-widget-provider.exe` does not exist yet — no COM widget provider has
   been written.

## Files

| File                          | What it is                                                                                                                                                                                                                        |
| ----------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `AppxManifest.xml`            | Sparse-package manifest skeleton: `AllowExternalContent`, `runFullTrust` + `unvirtualizedResources`, a `windows.comServer` `ExeServer`/`Class`, and the `com.microsoft.windows.widgets` app extension. Placeholders in `{{...}}`. |
| `build-sparse-package.ps1`    | Fills the placeholders, packs with `makeappx /nv`, signs with `signtool`. **Fails closed without a certificate.**                                                                                                                 |
| `register-sparse-package.ps1` | `Add-AppxPackage -ExternalLocation <install dir>` (per-user) and the matching unregister.                                                                                                                                         |

## The placeholders that must be filled

| Placeholder                                             | Meaning                                                                                                                                                 |
| ------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `{{IDENTITY_NAME}}`                                     | A package family name you choose, e.g. `Vitals.Sparse`. Distinct from the Store identity (`DragosCatalin.VitalsSystemMonitor`).                         |
| `{{PUBLISHER}}`                                         | **Must equal the Subject of the signing certificate.** Not `Dragos Catalin Vladulescu`; a real `CN=...` from the certificate.                           |
| `{{PUBLISHER_DISPLAY_NAME}}`                            | Human-readable publisher name.                                                                                                                          |
| `{{VERSION}}`                                           | Four numeric parts, `1.2.3.4`. Must be bumped on every re-registration.                                                                                 |
| `{{WIDGET_CLSID}}`                                      | A real CLSID that `vitals-widget-provider.exe` registers.                                                                                               |
| `{{WASDK_FRAMEWORK_PACKAGE}}` / `{{WASDK_MIN_VERSION}}` | The Windows App SDK framework package the provider links, if it is framework-dependent. Remove the `<PackageDependency>` for a self-contained provider. |

## How the spike uses it

1. Build a **full-MSIX** copy of the Microsoft
   [Widgets sample](https://github.com/microsoft/WindowsAppSDK-Samples/tree/main/Samples/Widgets)
   and confirm its widget appears in Win+W. (Proves the machine/board, not sparse.)
2. Take the sample's provider executable, put it in an install directory beside
   the app's other files, add its CLSID to `AppxManifest.xml`, build and sign with
   a self-signed certificate (its `.cer` in `Cert:\CurrentUser\TrustedPeople`),
   and register with `-ExternalLocation <that directory>`.
3. **Pass/fail:** does Vitals appear under **Win+W → Add widgets**, and does
   pinning it render a card? Yes → the NSIS route is open. No → the board is
   MSIX-only and the widget is Store-build-only.

The external location is **not** part of this package and is not built here. The
executables live in the Vitals install directory; the sparse package only grants
identity and points at them.

## Why the executables are not packed

`com:ExeServer`'s `Executable` "must reference a file in the package", and a
sparse package's whole point is that files in the external location are addressed
as if inside it. So the layout here holds only `AppxManifest.xml` and the assets
it names; `makeappx /nv` skips validation of the paths that resolve externally.
Whether COM activation actually honours that is the second thing the spike must
prove (see §10 of the research note).
