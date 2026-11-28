> Status: research snapshot, 2026-11-28. Not normative — decisions belong in
> `docs/TRACKER.md` and the ADRs. Claims are marked **VERIFIED** (read in a
> source or in this repository) or **EXPECTED** (reasoned, not yet run). Where
> no authoritative source was found, that is stated rather than guessed.

# A Vitals widget on the Windows 11 Widgets Board — what it takes, what it cannot do

**Question (paraphrased from the owner's request):** put an animated, morphing,
customisable set of Vitals widgets on the Windows 11 Widgets Board (the panel the
taskbar's weather widget opens), "complete", several widgets at once, custom
styles, designs and layouts. Is that possible, what would a Tauri 2 app have to
do, what is the honest cost, and what are the alternatives? Also: what has another
agent left unfinished in this direction?

**Method:** read-only. Microsoft Learn and the Windows App SDK documentation were
read directly (URLs in §7), plus this repository's own Store research, MSIX
manifest and distribution notes, and the sibling project `E:\gh\dashy`. No file
was edited except this note; nothing was staged, committed or deployed.

**Verdict in one line:** the platform supports a Vitals widget only as a **static
Adaptive Card** (or, in the EEA only, a sandboxed HTML web widget) drawn by a
**packaged (MSIX) Win32 COM provider** — so the owner's "animated, morphing,
custom styles and layouts" request is **not deliverable on the board at all**, and
the board route is a narrow, MSIX-only, Windows 11-only glanceable card, not a
canvas. The always-on desktop surface they are describing is a different feature,
and `dashy` is the precedent to build it in.

---

## 1. What a third-party widget on the board actually requires

### 1.1 The provider contract

A third-party widget is not a window and not a web page. It is content served by a
**widget provider**: a Windows app that implements the WinRT interface
`IWidgetProvider` and pushes updates through `WidgetManager`. The host (the
Widgets Board) activates the provider, asks it for an **Adaptive Card JSON**
template plus a data JSON, and re-renders when the provider sends new content.
VERIFIED — ["Widget providers"](https://learn.microsoft.com/en-us/windows/apps/develop/widgets/widget-providers).

The provider lifecycle is a fixed set of callbacks, all required:
`CreateWidget`, `DeleteWidget`, `OnActionInvoked`, `OnWidgetContextChanged`,
`Activate`, `Deactivate`. `Activate`/`Deactivate` bracket the window in which the
host wants fresh content; updates may also be pushed at any time, but the docs
warn to balance that against battery. VERIFIED —
[implement a widget provider (C++/WinRT)](https://learn.microsoft.com/en-us/windows/apps/develop/widgets/implement-widget-provider-win32).

### 1.2 Which SDK introduced what

| Capability                                       | API                                                 | Since                       | Source                   |
| ------------------------------------------------ | --------------------------------------------------- | --------------------------- | ------------------------ |
| Widget lifecycle + Adaptive Card updates         | `IWidgetProvider`, `WidgetManager`                  | Windows App SDK **1.2**     | widget-providers § table |
| User customisation                               | `IWidgetProvider2`, manifest `IsCustomizable`       | Windows App SDK **1.4**     | widget-providers § table |
| Web (HTML) content, resource requests, messaging | `IWidgetResourceProvider`, `IWidgetProviderMessage` | Windows App SDK **1.6.3**   | web-widget-providers     |
| "Placeholder"/non-personalised content flag      | `WidgetUpdateRequestOptions.IsPlaceholderContent`   | Windows App SDK **1.6.6**   | widget-providers § table |
| Widget analytics callbacks                       | `IWidgetProviderAnalytics`                          | Windows 11 build **26900+** | widget-providers § table |

VERIFIED: the whole feature arrived in **Windows App SDK 1.2**, whose release
notes state plainly that third-party widgets were added there and that "Widgets
can only be created for packaged, Win32 apps" — see
[Windows App SDK 1.2 release notes](https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/release-notes/windows-app-sdk-1-2).

### 1.3 It must be a packaged (MSIX) app — and Win32, not UWP

- The provider is declared in the **app package manifest** as a `uap3:AppExtension`
  named `com.microsoft.windows.widgets`, with a `WidgetProvider` → `Activation` →
  `CreateInstance ClassId="…"` pointing at a **COM class** the OS activates with
  `CoCreateInstance`. There is also a `com:Extension Category="windows.comServer"`
  exporting that CLSID. VERIFIED —
  [widget provider manifest format](https://learn.microsoft.com/en-us/windows/apps/develop/widgets/widget-provider-manifest)
  and the C++/WinRT walkthrough.
- It is a **packaged Win32 desktop app** (or a PWA). The manifest format page
  says verbatim: "For Win32 apps, only packaged apps are currently supported".
  UWP is **not** the supported path and was explicitly dropped from later SDK
  releases. VERIFIED.
- "Packaged" means **MSIX package identity**. A plain unpackaged `.exe` (the NSIS
  build) is not accepted by the board. VERIFIED.

### 1.4 What the board allows — and does not

| The widget may…                                                                                                     | VERIFIED source                               |
| ------------------------------------------------------------------------------------------------------------------- | --------------------------------------------- |
| Render only **Adaptive Card** JSON (static) or, EEA-only, an HTML **web widget**                                    | widget-providers; web-widget-providers        |
| Support three fixed sizes: **small, medium, large**                                                                 | widget-provider-manifest § Size               |
| Declare **multiple `Definition` entries** (several widgets per app)                                                 | widget-provider-manifest § Definitions / `Id` |
| Set `AllowMultiple` (repeat one definition), `IsCustomizable`, region gating (`ExclusiveRegions`/`ExcludedRegions`) | widget-provider-manifest § Definition         |
| Receive input through `Action.Execute` verbs (buttons, `OnActionInvoked`)                                           | C++/WinRT walkthrough                         |

| It may **not**                                        | Why                                                                                                    |
| ----------------------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| Animate or morph                                      | The body is a declarative Adaptive Card; there is no animation/transition model. No source offers one. |
| Use arbitrary CSS/React/Canvas                        | Only card elements, or HTML **but EEA-only** and sandboxed                                             |
| Define its own layout, size or placement on the board | The board owns the grid, the frame and the three sizes                                                 |
| Restyle the card chrome                               | `ThemeResources`/`Icon`/`Screenshot` and header options only; no arbitrary styling                     |
| Run on Windows 10                                     | The board "was first introduced in Windows 11" (1.2 notes; design page)                                |

**Web widgets (HTML, so _capable_ of animation) are restricted to the European
Economic Area.** Both the provider page and the design page state this as an
"Important" note: "This feature is available only to users in the European
Economic Area (EEA)." VERIFIED. So even the one route that could animate is
geographically unavailable to most of the audience.

### 1.5 Update cadence, disabled board, Windows 10

- **Update cadence:** the docs say a provider "can send updates to the widget at
  any time, such as in response to a push notification", but to "balance providing
  up-to-date content with resource concerns like battery life". **No numeric
  rate limit was found** in the widget provider, manifest, or web-widget pages.
  I am stating that plainly rather than inventing a figure.
- **Board disabled / not present (Windows 10):** if there is no board, the host
  never activates the provider, so **nothing runs and nothing is shown**; the
  provider is not started to tell it so. **EXPECTED** — this follows from the
  activation model (the host calls `CreateWidget`/`Activate`), but Microsoft does
  not document the disabled case explicitly.
- **Third-party note (not Microsoft):** the board was made "quiet" by default in
  2026 (no longer opens on hover). This came from a third-party explainer in web
  search, not Microsoft Learn — treat as unverified colour, not a contract.

---

## 2. Could a Tauri 2 app ship one?

Short answer: **not as the Tauri app itself.** The board activates a **COM server**
(the CLSID in the MSIX manifest) and hands it Adaptive Card JSON. A Tauri webview
cannot expose a COM class factory and cannot render a card. So a Vitals widget
means **a separate native provider process** shipped alongside (or inside) the
package, plus an MSIX to register it.

### 2.1 A separate provider component that talks to the Tauri backend — YES

- The provider can be a **small standalone executable** (Rust via `windows-rs`
  `#[implement]`, or C++/WinRT) that registers the `IWidgetProvider` class factory
  and, when asked for content, **reads live numbers from Vitals' existing local
  surface**. Vitals already exposes exactly this: the LAN server's `/summary`
  route is described in-repo as "what a watch face or a widget shows"
  (`crates/vitals-server/src/router.rs:38`; `docs/api/README.md:162`). So the
  provider would call `http://127.0.0.1:7331/summary` (token or loopback scope) and
  format an Adaptive Card. This is the **same pattern the iOS/Android widgets
  already use** (a small snapshot handed to the platform). VERIFIED (in-repo).
- **What cannot work:** pointing the board at the Tauri webview, embedding the
  React dashboard, or animating with Motion/CSS. The board does not host a
  webview except in the EEA web-widget case, and never for arbitrary HTML.

### 2.2 MSIX and the existing NSIS installer

- **Both can coexist.** Vitals already ships an NSIS installer for direct download
  and an MSIX for the Store; `docs/distribution.md` §3 records that "Tauri has no
  MSIX bundler, so `packaging/msix/build-msix.ps1` lays the package out from the
  release executables". VERIFIED (in-repo, read).
- The MSIX manifest (`packaging/msix/AppxManifest.template.xml`) already declares
  the `uap3` namespace, a `runFullTrust` Win32 application, and a `Windows.FullTrustApplication`
  entry point. Adding a widget provider is an **additive manifest edit** (a
  `com:Extension windows.comServer` + a `uap3:Extension com.microsoft.windows.widgets`)
  plus the provider binary in the package. VERIFIED (in-repo, read).
- **The NSIS build cannot have the widget on its own** unless package identity is
  granted another way. Microsoft documents that route: a **sparse package**
  ("grant package identity by packaging with external location") gives an
  unpackaged Win32 app package identity — and its stated purpose includes
  "app extensions" and "without … changing your existing installer". VERIFIED —
  [grant package identity](https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/grant-identity-to-nonpackaged-apps).
- **The gap that is not documented:** whether the **Widgets host honours a widget
  provider registered from a _sparse_ (external-location) package** the same way
  as a full MSIX. Every Microsoft widget sample uses a full MSIX packaging
  project, and no page states sparse-package support for widgets.
  **EXPECTED-uncertain / must be spiked.** Treat "widget on the NSIS build via a
  sparse package" as unproven until a real provider registers and appears in the
  picker.

### 2.3 Release pipeline changes (MSIX-only route)

1. New binary target: a `vitals-widget-provider` (Rust `cdylib`/exe implementing
   the COM server) added to the MSIX package contents. EXPECTED ~ this is the
   bulk of the work.
2. `AppxManifest.template.xml`: add the two extensions and a `Definition` per
   widget (Id, DisplayName, Description, sizes, icons, screenshots). VERIFIED by
   reading the manifest format page and the existing template.
3. Ship the **Windows App SDK runtime** (framework-dependent) or go self-contained,
   because the provider links the Widgets APIs. VERIFIED — the samples use
   framework-dependent deployment; self-contained is offered as an alternative.
   This is a new dependency the current NSIS/MSIX builds do not carry.
4. Packaging (`build-msix.ps1`) and `scripts/check-drift.ps1`/Store listing work:
   plus widget icons and picker **screenshots** per theme (light/dark), which the
   manifest requires. VERIFIED.

---

## 3. Honest cost and risk

Effort figures below are **EXPECTED** (engineering judgement), not measured.

| Slice                                                  | Effort                                                   | Notes                                                                                                                              |
| ------------------------------------------------------ | -------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| **Minimal**: one static card (CPU/mem/disk), MSIX-only | **L–XL**                                                 | New native COM provider (Rust/C++), WASDK dependency, manifest, packaging, picker art, a spike to confirm the sparse/MSIX reality. |
| **Multiple widgets** (a definition each)               | +M                                                       | Manifest + templates; the host already supports several.                                                                           |
| **Custom styles/designs/layouts as asked**             | **N/A**                                                  | Not expressible. The card schema and the board's frame own the look.                                                               |
| **Morphing / transitions / animation**                 | **N/A** on the board; possible only in an EEA web widget | The one route that animates is region-locked.                                                                                      |

**Risks / failure modes**

- **Region:** the animated (web-widget) path is EEA-only; the static path works
  everywhere the board exists. VERIFIED.
- **Platform:** Windows 11 only; Windows 10 machines show nothing. VERIFIED.
- **Runtime:** a framework-dependent provider needs the Windows App SDK runtime;
  if it is missing the provider will not start. Self-contained avoids this but
  grows the package. VERIFIED (mechanism), EXPECTED (behaviour on a machine
  without the runtime).
- **Customisation bug:** the docs record a **known board bug** where, for
  WASDK-built widgets, "the ellipsis menu [becomes] unresponsive after the
  customization card is shown". VERIFIED — C++/WinRT walkthrough, Verbatim note.
- **Certification lag:** the Store path already lags direct download by days
  (`docs/distribution.md`). VERIFIED (in-repo).
- **Sparse-package uncertainty** (§2.2): a plausible dead end. EXPECTED.

---

## 4. Alternatives that deliver more of the request

The owner is describing an **always-on, animated, customisable ambient surface**.
That is not the board. Three alternatives, compared:

| Alternative                                          | What it is                                                                                                           | Effort  | Reach                                                      | What the owner loses                                            |
| ---------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------- | ------- | ---------------------------------------------------------- | --------------------------------------------------------------- |
| **A. Always-on-top desktop overlay (or corner pin)** | A second Tauri window: borderless, `alwaysOnTop`, translucent, click-through where wanted, snapping to a corner/side | **M**   | Every Windows 10/11 Vitals user, NSIS **and** MSIX         | Not "on the board"; it floats over work, not behind the taskbar |
| **B. Tray / notification-area presence**             | The existing live tray number, extended with hover detail / flyout                                                   | **S**   | Universal                                                  | Tiniest footprint possible; not an ambient display              |
| **C. Dashy-style "StandBy" ambient screen**          | Reuse `E:\gh\dashy`: a configurable grid of widgets, clocks, slideshows, skins and layouts                           | **M–L** | Any spare screen/tablet; the whole dashy feature catalogue | Needs a screen or window kept on; not inside the OS board       |

- **`dashy` is the precedent.** It is a Tauri 2 + React 19 "StandBy-class ambient
  display **and** dashboard … on Android (primary), Windows (full app **and** data
  agent)", and it **depends on `vitals-core` + `vitals-win`** for PC telemetry.
  Its research catalogue already covers exactly what the owner asked for —
  stacked widgets, layouts, skins, hundreds of prefabricated views. VERIFIED —
  `E:\gh\dashy\docs\TRACKER.md` and `docs/research/feature-catalogue.md`.
- **But note what dashy's Windows build is today:** a normal, decorated,
  resizable 1280×800 window (`apps/app/src-tauri/tauri.conf.json`, VERIFIED). Its
  always-on/docked behaviour is realised on **Android** (a `DreamService`
  screensaver, per `docs/android.md`). So "Windows always-on overlay" is **new
  work in either repo**; dashy gives you the _content and customisation_ layer,
  not a ready Windows overlay.
- **What Windows does not offer:** there is no third-party lock-screen widget API
  and no wallpaper API a normal app may draw into. The classic "behind the desktop
  icons" (WorkerW) and lock-screen tricks are unsupported/non-store surfaces.
  EXPECTED — treat as out of scope, not as a milestone.

**Recommendation:** build **A** (or fold it into **C** via dashy) as the real
feature; keep the **board** as an optional, MSIX-only, _static_ glanceable card
built later and only after a one-day spike proves the provider registers and
appears in the picker. Do **not** promise animation, custom styles or layout
"completeness" on the board — the platform forbids them.

---

## 5. Recommended plan

**Milestone 0 — one-day feasibility spike (do this before any estimate).**
Build the Microsoft `WindowsAppSDK-Samples/Samples/Widgets` provider, register it,
confirm it appears in the picker on a real Windows 11 machine, and then attempt
the **same provider from a sparse package** to learn whether the board honours
non-MSIX identity. _Proof:_ the sample widget visible and updating in the board;
then the sparse-package provider either appears (opens the NSIS route) or does not
(closes it).

**Milestone 1 — overlay feature (the real value).** A borderless, always-on-top,
optionally click-through, corner-snapping second window in the Vitals desktop app,
rendering a small set of live readings from the existing frame. _Proof on a real
machine:_ the window stays on top across apps, survives sleep/wake, respects
per-monitor DPI, does not steal focus, and can be dismissed.

**Milestone 2 — customisation.** Widget selection, order, size and a light/dark
theme, persisted with the app's existing settings. Borrow dashy's catalogue rather
than inventing one.

**Milestone 3 (optional, MSIX-only) — a static board card.** One Adaptive Card
reading `/summary`; multiple definitions if wanted. _Proof:_ the card appears in
the picker, updates, and opens the app on click.

**Dead ends to avoid**

- Expecting animation, morphing, custom styles or free layout **on the board** —
  impossible (Adaptive Cards; web widgets EEA-only).
- Shipping a widget on the NSIS build **without** first proving the sparse-package
  route.
- Driving the card from the React layer — the board talks to a COM provider, not
  a webview.
- Portrait/photo/clock "modes" à la StandBy on the board — the board owns the
  card.

---

## 6. Work another agent left unfinished (this direction)

- **The only trace is an idea, not a task.** `docs/store-research-2026-10.md`
  (idea #9, "Windows 11 Widgets board provider", effort "L") records the
  conclusion and links the same Microsoft page used here, with the note "works for
  the MSIX build only unless a sparse package is added to NSIS". VERIFIED — read
  in the file. It is **not** in `docs/tracker.csv` and **not** in `docs/TRACKER.md`
  (grep for "widgets board"/"widget provider" over both returns nothing), so it was
  never turned into scoped work.
- **No spike, no branch artefact, no manifest change** exists for it in the repo:
  `packaging/msix/AppxManifest.template.xml` contains no `com.microsoft.windows.widgets`
  extension, and there is no widget-provider crate or component. VERIFIED (read).
- So the gap is a **documented, unfollowed-through idea** — its one open question
  (does a **sparse package** work for a widget provider?) is exactly Milestone 0
  above, and remains unanswered here because Microsoft does not document it.

---

## 7. Sources (read this session)

- Widget providers — <https://learn.microsoft.com/en-us/windows/apps/develop/widgets/widget-providers>
- Implement a widget provider in a win32 app (C++/WinRT) — <https://learn.microsoft.com/en-us/windows/apps/develop/widgets/implement-widget-provider-win32>
- Web widget providers (EEA-only) — <https://learn.microsoft.com/en-us/windows/apps/develop/widgets/web-widget-providers>
- Windows widgets (design overview, terminology, EEA note) — <https://learn.microsoft.com/en-us/windows/apps/design/widgets/>
- Widget provider package manifest XML format — <https://learn.microsoft.com/en-us/windows/apps/develop/widgets/widget-provider-manifest>
- Windows App SDK 1.2 release notes (third-party widgets introduced; packaged Win32 only) — <https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/release-notes/windows-app-sdk-1-2>
- Grant package identity by packaging with external location (sparse package) — <https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/grant-identity-to-nonpackaged-apps>
- In-repo: `docs/store-research-2026-10.md`, `docs/distribution.md`,
  `packaging/msix/AppxManifest.template.xml`, `crates/vitals-server/src/router.rs`,
  `docs/api/README.md`, `docs/research/index.md`; and in `E:\gh\dashy`:
  `docs/TRACKER.md`, `docs/research/feature-catalogue.md`, `docs/android.md`,
  `apps/app/src-tauri/tauri.conf.json`.

## 8. Not verified / open questions

- **Whether a sparse (external-location) package can register a widget provider.**
  Not documented by Microsoft; a spike is required before the NSIS build is
  promised the feature.
- **Whether the board throttles updates.** No numeric cap found in the pages
  read; only generic battery guidance.
- **Behaviour when the board is disabled or absent.** Expected to be "provider
  never activated", but not stated in the docs.
- **Windows App SDK 1.2+ minimum _runtime_ on a retail machine** and exactly which
  Windows 11 builds ship a board new enough for `IWidgetProviderAnalytics`
  (build 26900+) — not resolved here.
