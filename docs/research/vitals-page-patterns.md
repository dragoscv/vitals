> Status: research snapshot, 2026-11-25. Not normative — see docs/TRACKER.md and the ADRs for decisions.

# Adding one page to the Vitals desktop app — the exact recipe

Read `E:\gh\vitals\AGENTS.md` first; it overrides anything below. This report
answers: what files declare a route and its nav entry, what the feature-folder
convention is, every gate a new page must pass, and how new wire types are
added. Read-only research; nothing was edited, staged, built or installed.

Worked example: a **"Projects health"** page — a list of remote things fetched
from an API, with loading / empty / error states. The two existing features
most like it are **`features/health`** (polls a remote, states union, error
kinds) and **`features/apps`** (one-shot list read, prefetch, sort/filter,
per-row action). Both are quoted below.

---

## 1. The route + navigation chain

A page exists when **four** things agree: a `RouteId`, a `navItems` entry, a
`RouteContent` case, and a `nav.*` translation. The sidebar, palette,
shortcuts, route restore and the hidden-mount preload all read the same
visibility truth, so a page cannot be reachable in one place and invisible in
another.

### 1a. The closed union and the nav array — `apps/desktop/src/shell/navigation.ts`

`routeIds` (VERIFIED, lines 41-57) is a closed union; `navItems` (VERIFIED,
lines 92-108) is the sidebar order, which is also the Ctrl+1…9 order:

```ts
export const routeIds = [
  'dashboard',
  'performance',
  'processes',
  'agents',
  'collab',
  'startup',
  'services',
  'appHistory',
  'users',
  'network',
  'storage',
  'installedApps',
  'devices',
  'benchmarks',
  'health',
] as const;
export type RouteId = (typeof routeIds)[number];
```

```ts
export interface NavItem {
  readonly id: RouteId;
  /** Key into the `nav.*` namespace. Resolved at render time, never here. */
  readonly labelKey: string;
  readonly icon: LucideIcon;
  readonly group: NavGroupId; // 'now' | 'machine' | 'more'
}
```

Current sidebar items, in order (VERIFIED, `navigation.ts:92-108`):

| #   | id              | labelKey            | group   | icon            |
| --- | --------------- | ------------------- | ------- | --------------- |
| 1   | `dashboard`     | `nav.dashboard`     | now     | LayoutDashboard |
| 2   | `performance`   | `nav.performance`   | now     | Activity        |
| 3   | `processes`     | `nav.processes`     | now     | ListTree        |
| 4   | `agents`        | `nav.agents`        | now     | Bot             |
| 5   | `collab`        | `nav.collab`        | now     | MessagesSquare  |
| 6   | `startup`       | `nav.startup`       | machine | ClockArrowUp    |
| 7   | `services`      | `nav.services`      | machine | Server          |
| 8   | `storage`       | `nav.storage`       | machine | HardDrive       |
| 9   | `installedApps` | `nav.installedApps` | machine | Boxes           |
| 10  | `devices`       | `nav.devices`       | machine | Usb             |
| 11  | `network`       | `nav.network`       | machine | Network         |
| 12  | `appHistory`    | `nav.appHistory`    | more    | Gauge           |
| 13  | `users`         | `nav.users`         | more    | Users           |
| 14  | `benchmarks`    | `nav.benchmarks`    | more    | Cpu             |
| 15  | `health`        | `nav.health`        | more    | HeartPulse      |

The comment at (VERIFIED, `navigation.ts:106-107`) says `health` is "last,
after the first nine, so Ctrl+1…9 never shifts when it appears" — the same
argument applies to a new item: **append a new item at the end of a group so
the Ctrl+digit order of what exists does not shift.**

### 1b. How a route is hidden or feature-gated

Two mechanisms, both in `navigation.ts`:

- **Platform gate** — `windowsOnlyRoutes` (VERIFIED, lines 168-175) lists
  routes whose backend commands are `#[cfg(windows)]`; `routeSupported()`
  (VERIFIED, 178-183) hides them on a non-Windows build, driven by
  `Capabilities.platform`. Add `'projects'` here only if its Rust commands are
  Windows-only.
- **Fact gate** — `useNavVisibility` is a zustand store (VERIFIED, 126-165)
  holding `health` (`null` = unknown = hidden), `agentq`, `platform`.
  `isRouteVisible()` (VERIFIED, 186-200) is the single source of truth:

```ts
export function isRouteVisible(id, state = useNavVisibility.getState()): boolean {
  if (id === 'health') return state.health === true;
  if (id === 'agents' || id === 'collab') return state.agentq !== false;
  return routeSupported(id, state.platform);
}
```

The `health` pattern is the model for a page that should only appear once a
connection is stored: a store fact, a small `features/health/visibility.ts`
(VERIFIED, whole file) that calls the backend and calls
`useNavVisibility.getState().setHealth(configured)`, wired once in
`AppShell.tsx` via `useHealthVisibility()` (VERIFIED, `AppShell.tsx:86`).

### 1c. The route → view resolution — `apps/desktop/src/routes.tsx`

Every feature is lazy-loaded, with its translations registered inside the
`lazy` factory, and memoised with `once()` so the chunk is fetched once
(VERIFIED, `routes.tsx:55-106`, e.g.):

```ts
const loadHealth = once(async () => {
  const m = await import('./features/health');
  m.registerHealthStrings();
  return m;
});
const HealthScreen = lazy(async () => ({ default: (await loadHealth()).HealthScreen }));
```

The view is resolved by an **exhaustive switch** so a missing case is a
compile error (VERIFIED, `routes.tsx:447-500`):

```ts
export function RouteContent({ route, onNavigate }) {
  switch (route) {
    case 'dashboard':  return <DashboardScreen onNavigate={onNavigate} />;
    // …
    case 'health':     return <HealthScreen />;
    default: {
      const exhaustive: never = route;
      return exhaustive;
    }
  }
}
```

Optional: add the loader to `preloaders` (VERIFIED, 133-148) so a first visit
is warm. Health's chunk is deliberately **not** preloaded (VERIFIED, 494-496)
because most installs never connect; a Projects page that polls may want the
same treatment, or a `prefetchProjects()` entry if it is a one-shot list.

### 1d. The screen wrapper

`Content` renders `<main id="main-content" tabIndex={-1}>` into a
`@container/main` flex column (VERIFIED, `Content.tsx:76-118`), and every
wrapper down to the screen root is `flex min-h-0 flex-1 flex-col` so a screen
can scroll an inner region (VERIFIED comment `Content.tsx:96-113`). A screen's
root therefore uses the shared `screen` class (below), never `h-full`.

---

## 2. The feature folder convention

Features live in `apps/desktop/src/features/<name>/`. There is **no** runtime
route registry: a feature is a folder plus the four edits in §1. The files a
feature carries today (VERIFIED, `features/health`, `features/apps`):

| File                     | Role                                                                 | health                                       | apps                                |
| ------------------------ | -------------------------------------------------------------------- | -------------------------------------------- | ----------------------------------- |
| `index.ts`               | public barrel; exports the screen + `register*Strings` + prefetchers | ✅                                           | ✅                                  |
| `strings.ts`             | its own i18n namespace, `en`+`ro` side by side, `register*Strings()` | ✅                                           | ✅                                  |
| `model.ts`               | pure data shape, sort/filter/derive, unit-tested                     | ✗ (shape in api)                             | ✅                                  |
| `api.ts` / `use*.ts`     | the backend seam + the hook                                          | api.ts + useHealth.ts                        | useApps.ts                          |
| `<Name>Screen.tsx`       | the component                                                        | ✅                                           | ✅                                  |
| `visibility.ts`          | feeds the nav-visibility store (optional)                            | ✅                                           | ✗                                   |
| `fixtures.test-utils.ts` | fake api for tests                                                   | ✅                                           | ✗                                   |
| `*.test.ts(x)`           | unit tests                                                           | `useHealth.test.ts`, `HealthScreen.test.tsx` | `model.test.ts`, `strings.test.ts`… |

### 2a. The backend seam — a Tauri command (the dominant variant)

`features/health/api.ts` (VERIFIED, 100-125) is the canonical shape: an
interface, a `tauri…Api`, a `noHost…Api` (the browser preview), and
`default…Api()` choosing by `hasTauriHost()`. The `invoke` call is **always a
dynamic import** so a browser never evaluates the IPC module:

```ts
async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const core = await import('@tauri-apps/api/core');
  return core.invoke<T>(command, args);
}
export const tauriHealthApi: HealthApi = {
  status: () => invoke<HealthStatus>('health_status'),
  fetch: () => invoke<MeHealth>('health_fetch'),
};
export const noHostHealthApi: HealthApi = {/* … documented absence … */};
export function defaultHealthApi(): HealthApi {
  return hasTauriHost() ? tauriHealthApi : noHostHealthApi;
}
```

A simpler feature guards the host at the call site instead
(VERIFIED, `features/apps/useApps.ts:34-52`):

```ts
export async function readApps(): Promise<AppsSnapshot> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<AppsSnapshot>('get_installed_apps');
}
export async function runUninstaller(app: { keyName: string; source: AppSourceKey }) {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<void>('uninstall_app', { keyName: app.keyName, source: app.source });
}
```

Args are passed as an object whose keys match the Rust parameter names
(`{ base, name }`, `{ keyName, source }`).

### 2b. The hook

Two shapes exist; pick by whether the data changes on its own.

- **Polled remote** — `features/health/useHealth.ts` (VERIFIED, 26-40): a
  discriminated union, polled every 60 s only while the document is visible,
  keeping the last good body across an `unreachable` error.

```ts
export type HealthState =
  | { readonly status: 'loading'; readonly data: null; readonly error: null }
  | { readonly status: 'ok'; readonly data: MeHealth; readonly error: null }
  | { readonly status: 'error'; readonly data: MeHealth | null; readonly error: HealthErrorKind };
```

- **One-shot list with prefetch** — `features/apps/useApps.ts`
  (VERIFIED, 63-160): `prefetch(key, reader, ttl)`, `peekPrefetched`,
  `takePrefetched` from `../../lib/prefetch`, a `pending`/`error`/`refresh`
  state, a `reader?` injection seam, and an explicit no-host early return
  ("a loading state that cannot end makes a broken app look busy").

A brand-new **"Projects health"** feature should copy `features/health` if it
polls a remote, or `features/apps` if it is a one-shot list.

### 2c. Where the Rust command lives and is registered

Commands live in a module under `apps/desktop/src-tauri/src/`, declared
`#[tauri::command]` (VERIFIED, `health.rs:320-321,643-644`):

```rust
#[tauri::command]
pub async fn health_status() -> Result<HealthStatus, String> { … }

#[tauri::command]
pub async fn health_fetch() -> Result<MeHealth, FetchError> { … }
```

They are reachable only if listed in the single `generate_handler!` block in
`apps/desktop/src-tauri/src/lib.rs` (VERIFIED, 249-484), qualified as
`module::name,`, with a per-entry `#[cfg(...)]` when the definition is
platform-gated:

```rust
tauri::generate_handler![
    commands::get_host_info,
    // …
    #[cfg(any(windows, target_os = "macos"))]
    commands::terminate_process,
    // …
    health::health_status,
    health::health_pair_start,
    health::health_fetch,
]
```

Use `async fn` for anything that touches the network or a driver: a plain
`fn` command runs on the main thread and must be on the allowlist in
`check-drift.ps1` check 7 or the gate fails (VERIFIED, `check-drift.ps1:352-390`).

### 2d. A direct HTTP call from a feature — there is none

VERIFIED: `rg 'fetch\(|new WebSocket' apps/desktop/src` finds no webview HTTP
against a remote; the only `@vitals/client` (the REST/SSE/WS SDK) use is in
`apps/desktop/src/mobile/` (the phone PWA talking to the LAN server). Principle
1 in `docs/architecture.md`: **the webview renders; Rust computes.** So the
"remote API" variant is: the webview calls a Tauri command (2a), and the Rust
command does the HTTP. `health.rs` is exactly that — `health_fetch` fetches the
vmui HTTP endpoint in Rust and returns typed data.

### 2e. Loading / empty / error rendering

`health/HealthScreen.tsx` (VERIFIED, render tail) and `apps/AppsScreen.tsx`
(VERIFIED, 126-197) share the pattern:

```tsx
{state.status === 'loading' && (
  <div aria-busy="true" aria-label={t('loading')} className="grid gap-3 lg:grid-cols-2">
    {[0,1,2,3].map((i) => <Skeleton key={i} className="h-36" />)}
  </div>
)}
{state.status === 'error' && state.data === null && (
  <EmptyState icon={ERROR_ICON[state.error]} title={…} description={…} action={…} />
)}
{state.error !== null && state.error !== NO_HOST && (
  <p role="alert" className="text-2xs text-[var(--color-danger-text)]">…</p>
)}
{state.data !== null && <HealthCards … />}
```

Empty (no rows) uses `<EmptyState title={t('empty.title')} description={t('empty.body')} />`
(VERIFIED, `AppsScreen.tsx:197`).

### 2f. Styles

Screens use the shared component classes in `apps/desktop/src/styles.css`,
never bespoke layout (VERIFIED, 131-179):

```css
.screen {
  display: flex;
  flex: 1 1 0%;
  flex-direction: column;
  gap: 1rem;
  min-height: 0;
}
.screen-header {
  display: flex;
  flex-wrap: wrap;
  align-items: flex-start;
  justify-content: space-between;
  gap: 0.5rem 1rem;
}
.screen-scroll,
.table-scroll {
  flex: 1 1 0%;
  min-height: 0;
  overflow: auto;
}
```

Colours come from tokens (`var(--color-fg-muted)`, `var(--color-danger-text)`,
`--color-accent-text`), never raw values.

---

## 3. Every gate, with the exact command

Run everything CI runs with **`pwsh -NoProfile -File scripts/verify.ps1`**
(VERIFIED: contracts drift → rust clippy/tests → bindings drift → ts
typecheck/lint/test → bundle + size budget). The individual gates:

### 3a. i18n locale parity

- **App strings** live in the feature's own `features/<name>/strings.ts` as
  `en`/`ro` objects side by side and are registered at chunk load through
  `i18n.addResourceBundle` (VERIFIED, `features/health/strings.ts` tail):

```ts
export const HEALTH_NS = 'health';
const en = { title: 'Health' /* … nested … */ };
const ro = { title: 'Sănătate' /* … */ };
export const bundles = { en, ro } as const;
export function registerHealthStrings(): void {
  for (const [locale, bundle] of Object.entries(bundles)) {
    i18n.addResourceBundle(locale, HEALTH_NS, bundle, true, false);
  }
}
```

- **Shell strings** live in `apps/desktop/src/shell/strings.ts`, namespace
  `shell` (`SHELL_NS`), same `en`/`ro` side-by-side shape (VERIFIED, head).
- **Nav labels** live in `packages/i18n/src/locales/en.json` + `ro.json` under
  `nav.*` (VERIFIED, `en.json:6-23`), e.g. add `"projects": "Projects health"`.
- **Locale file shape**: a plain nested JSON object; keys are resolved by
  path, plural keys use CLDR suffixes `_one`/`_other` (VERIFIED, `apps`
  strings `summary.count_one`/`_other`).
- **Exemption**: the drift check compares leaf keys after stripping
  `_(zero|one|two|few|many|other)$`, because Romanian has a `_few` form
  English does not (VERIFIED, `check-drift.ps1:227-232`).
- Each feature also carries a `strings.test.ts` asserting en/ro key parity by
  the same strip (VERIFIED, `features/apps/strings.test.ts:17-21`).

### 3b. The drift script — `scripts/check-drift.ps1`

Exit code is the number of hard failures; warnings do not fail. It checks
(VERIFIED, header 1-66): **1** every `ts_rs::TS` struct/enum has a serde
`rename_all = "camelCase"`; **2** every `invoke('name')`/`call<T>('name')` in
the webview has a matching `#[tauri::command]` registered in
`generate_handler!`; **3** a registered command nobody invokes is a _warning_;
**3b** an ungated handler entry must not be a `#[cfg(windows)]`-only
definition; **4** en/ro locale parity (packages/i18n **and** shell strings
**and** Tizen); **5** settings keys read/written symmetry; **6** Android
contract fixtures; **7** a **synchronous** `#[tauri::command]` must be on the
cheap allowlist; **8** `docs/tracker.csv` row shape/ids/status; **9** Android
required-features allowlist; **10** version literals in scripts.

### 3c. Size budget — `size-budget.json` + `scripts/check-size.ps1`

What counts (VERIFIED, `check-size.ps1:11-25`): **INITIAL** = entry chunk the
main window parses before the first screen (the startup-latency figure);
**SHIPPED** = every `.js`/`.css` under `apps/desktop/dist/assets`; **INSTALLER**
= the NSIS download; plus one budget per secondary entry (`hud.html`,
`mobile.html`). Current numbers (VERIFIED, `size-budget.json`):

| key                | bytes   | meaning           |
| ------------------ | ------- | ----------------- |
| `initialGzipBytes` | 221184  | entry chunk, gzip |
| `shippedGzipBytes` | 548864  | all JS+CSS, gzip  |
| `installerBytes`   | 6012928 | NSIS installer    |
| `hudGzipBytes`     | 125296  | HUD entry         |
| `mobileGzipBytes`  | 155648  | mobile entry      |

Measured after a production build:

```pwsh
pnpm --filter @vitals/desktop build:vite
pwsh -NoProfile -File scripts/check-size.ps1            # add -SkipInstaller to skip the NSIS
```

The budgets "sit just above the current measured sizes" and are raised only
deliberately, in a reviewable edit (ADR 0018). A new page must stay **lazy** —
`routes.tsx`'s comment (VERIFIED, 15-32) is the reason: eager-importing one
screen moves its whole subtree into INITIAL.

### 3d. The e2e suite

Specs live in `apps/desktop/e2e/`; run with **`pnpm --filter @vitals/desktop e2e`**
(Playwright, `playwright.config.ts`). It drives the **no-host** fallback served
by plain Vite on port **5373** (never 5273, the Tauri dev port — VERIFIED
comment 5-10). The config sets `contextOptions: { reducedMotion: 'reduce' }`
so a measurement never lands mid-fade (VERIFIED, 47-52).

**Two mandatory touch-points for a new page:**

1. **`e2e/support.ts`** — add a `routes` entry `{ id, label, landmark }`
   (VERIFIED, 22-60). `landmark` is the screen's own `<h2>`; the parity test
   in `navigation.spec.ts` reads `routeIds` from `navigation.ts` and asserts
   the e2e list matches it exactly (VERIFIED, `navigation.spec.ts:12-27`) —
   **a route added to `navigation.ts` with no `support.ts` row fails that
   test.**
2. What the per-route specs assert (VERIFIED, `navigation.spec.ts:29-41`):
   the landmark is visible, `document.title` is `` `${label} — Vitals` ``, and
   `expectNoProblems` (no console errors / uncaught exceptions).

`a11y.spec.ts` (VERIFIED, whole file) runs axe with
`['wcag2a','wcag2aa','wcag21a','wcag21aa','wcag22aa']` on **every** route and
fails on **serious or critical** violations only.

A spec that needs data installs the fake host
`installFakeHost(page, { locale: 'en', … })` from `e2e/fakeHost.ts`
(VERIFIED, `collab.spec.ts:1-14`) — the no-host fallback only shows an empty
state, so layout/type defects need real content.

**Known flaky / timing-sensitive spec:** `e2e/live-order.spec.ts` uses several
`page.waitForTimeout(400…1000)` calls (VERIFIED, lines 130-225) to watch live
lists keep their places between ticks; it needs the fake host and real frames,
so it is the most timing-dependent. `navigation.spec.ts`'s "visiting every
section in one session" spec is also heavier (it walks every route twice).

### 3e. Vitest rules for a feature

Config is the `test` block in `apps/desktop/vite.config.ts` (VERIFIED, 87-135):
`environment: 'happy-dom'`, `globals: true`, `setupFiles: ['./src/test/setup.ts']`,
`include: ['src/**/*.{test,spec}.{ts,tsx}']`, **`testTimeout`/`hookTimeout` 15 s**,
`maxWorkers: '25%'`, coverage `thresholds: { statements: 70, branches: 70,
functions: 70, lines: 70 }`. Run with `pnpm --filter @vitals/desktop test`
(or `pnpm test` at the root). A feature test `beforeAll` runs
`await initI18n('en'); registerXxxStrings();` (VERIFIED,
`HealthScreen.test.tsx:9-12`). `strings.ts` files are excluded from coverage
(VERIFIED, `vite.config.ts:126`).

### 3f. Lint / type expectations that bite

tsconfig (`packages/config/tsconfig/base.json`, extended by the desktop app;
VERIFIED): **`strict`**, **`exactOptionalPropertyTypes`**,
**`noUncheckedIndexedAccess`**, **`verbatimModuleSyntax`**,
`noUnusedLocals/Parameters`, `useUnknownInCatchVariables`. Concretely:

- `exactOptionalPropertyTypes`: you may not pass `undefined` to an optional
  prop — use `...(x !== undefined && { key: x })`, not `key: x ?? undefined`
  (VERIFIED, `HealthScreen.tsx:392-395` and AGENTS.md).
- `noUncheckedIndexedAccess`: `array[0]` is `T | undefined`.
- `verbatimModuleSyntax`: `import type { … }` for type-only imports.

ESLint is type-aware (`projectService`), and the rules that throw are
`@typescript-eslint/no-explicit-any: error`,
`@typescript-eslint/no-floating-promises: ['error', { ignoreVoid: true }]`,
`consistent-type-imports` (VERIFIED, `eslint.config.mjs:23-61`). Run
`pnpm --filter @vitals/desktop lint` and `typecheck`.

### 3g. Motion rules

- Import components as `import * as m from 'motion/react-m'` and render
  `m.div`; import `AnimatePresence`/`LazyMotion`/`MotionConfig` from
  `motion/react` (VERIFIED, `features/agents/SessionsPanel.tsx:10-11`).
- **`motion.*` is forbidden** — `LazyMotion … strict` **throws** the moment a
  `motion.*` component renders, because it would pull the full feature bundle
  into that chunk (VERIFIED, `theme/MotionRuntime.tsx:8-14,29`). Use `m.*`.
- The root providers (`LazyMotion(domAnimation, strict)` + `MotionConfig
reducedMotion`) live only in `theme/MotionRuntime.tsx`, imported lazily by
  `theme/MotionProvider.tsx` (VERIFIED). A new page must not add its own
  `LazyMotion`.
- Prefer CSS or the Web Animations API for one-property fades: `routes.tsx`
  documents the measured reason a Motion `AnimatePresence`/`m` fade was
  rejected for the route transition (VERIFIED, 210-245).
- Honour `useReducedMotion()` from `@vitals/ui`.

---

## 4. Accessibility the repo already enforces

Quoted as the pattern from `health/HealthScreen.tsx` and the shell:

- **Landmark + heading**: the screen root is `<div className="screen">` and
  its first child `<header className="screen-header">` carries the page
  **`<h2>`** (VERIFIED, `HealthScreen.tsx:311-327`). The e2e landmark is
  `getByRole('heading', { level: 2, name, exact: true })` (VERIFIED,
  `support.ts:63-66`), so **the title must be an `h2`**; card titles are `h3`
  (`CardTitle level={3}`, VERIFIED, `HealthScreen.tsx:264`).
- **`aria-current` on the active nav item**: the sidebar button sets
  `aria-current={isActive ? 'page' : undefined}` and `tabIndex` 0 only when
  active (VERIFIED, `Sidebar.tsx:259-268`); the e2e `openRoute` asserts
  `toHaveAttribute('aria-current', 'page')` (VERIFIED, `support.ts`).
- **Focus**: on navigation, focus moves to the new screen's heading
  (`Content.tsx:18-60`); the nav is a roving-tabindex list with arrow keys
  (`Sidebar.tsx:100-140`); the skip link targets `<main tabIndex={-1}>`.
- **Live regions for async state**: loading skeletons use
  `aria-busy="true"` + `aria-label` (VERIFIED, `HealthScreen.tsx:338-344`);
  errors use `role="alert"` (VERIFIED, `AppsScreen.tsx:162-164`); neutral
  updates use `role="status"` (VERIFIED, `HealthScreen.tsx:366-370`);
  `EmptyState` renders the empty/error body.
- **Contrast tokens**: use `var(--color-accent-text)` on an accent-tinted
  surface, never `var(--color-accent)` — the comment records it measured
  4.12:1, under AA (VERIFIED, `Sidebar.tsx:276-279`). The axe gate (§3d)
  enforces WCAG 2.2 AA, serious+critical.

---

## 5. Adding a new wire type

Two cases, and they are not interchangeable:

**(a) A `vitals-core` domain type** (a measurement / frame field) — derive
ts-rs and let it be generated. The attribute block is fixed (VERIFIED,
`crates/vitals-core/src/*.rs`, e.g. `sample.rs:25`):

```rust
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "core/", rename_all = "camelCase")]
pub struct ProjectsHealth { … }
```

Regenerate and check (VERIFIED, root `package.json` scripts and
`.cargo/config.toml`):

```pwsh
pnpm protocol:generate   # = cargo test -p vitals-core --features ts
pnpm protocol:check      # = regenerate + git diff --exit-code -- packages/protocol/src/generated
```

`TS_RS_EXPORT_DIR` is set in `.cargo/config.toml` to
`packages/protocol/src/generated`, so files land in
`packages/protocol/src/generated/core/*.ts` and are re-exported from
`@vitals/protocol` (`packages/protocol/src/index.ts`). The TS side imports
from `@vitals/protocol`; **never hand-write a type that lives there.**

**Serde casing rule (hard):** any `ts_rs::TS` struct/enum must carry
`#[serde(rename_all = "camelCase")]` — ts-rs exports camelCase; serde does not
by default (VERIFIED, `check-drift.ps1:82-107`). Missing it is a hard
failure. `ProcessKey` shipped `start-time` against a `startTime` binding this
way.

**(b) A desktop-shell-only shape** (no `vitals-core` type — registry hooks,
settings) — hand-written in `packages/protocol/src/commands.ts`, mirroring the
Rust struct in `apps/desktop/src-tauri/src/commands.rs`, both camelCase, and
the drift gate still checks the field names (VERIFIED, `commands.ts` header).

---

## 6. What a new page must not violate (AGENTS.md + ADRs)

- **A number that cannot be measured is `None`, never `0`** — omit absent
  fields, render an em dash, never `.unwrap_or(0)` (AGENTS.md; ADR-0007
  honest stubs; `docs/architecture.md`).
- **British spelling** in prose and comments (AGENTS.md).
- **Every user-facing string goes through i18n, in both `en` and `ro`** — no
  inline strings in components (AGENTS.md).
- **Comments explain why, never what** — record the reason a decision was
  made, what breaks if reverted (AGENTS.md § Comments).
- **Remote access off by default; read scope cannot control; the default
  pairing is read-only; a token is shown once** — if the page touches the LAN
  server (AGENTS.md § Security, ADR-0003).
- **Sync `#[tauri::command]` must be on the cheap allowlist; anything doing
  I/O is `async fn`** (check 7).
- **Windows-only commands need their own `#[cfg]` on the handler entry**
  (check 3b; ADR-0007).
- **The size budget is a ratchet, raised deliberately** (ADR-0018).
- **Keep `docs/tracker.csv` (state) and `docs/TRACKER.md` (reasoning + a
  verification-log entry with real command output) in step** (AGENTS.md §
  Before you finish; ADR-0009).
- **Commit only explicit paths, never push** (ADR-0010, ADR-0019).
- **`icon buttons` and nav entries need an accessible name**; axe is a gate.

---

## Gate checklist

| Gate                | Command                                                                                  | Must output for the page to be acceptable                                       |
| ------------------- | ---------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------- |
| Route list parity   | (inside `pnpm e2e`) `navigation.spec.ts`                                                 | e2e `routes` list equals `routeIds`; the new id present in both                 |
| Contract drift      | `pwsh -NoProfile -File scripts/check-drift.ps1`                                          | exit 0 — serde casing, `invoke`↔command, locale parity, tracker, sync allowlist |
| Bindings drift      | `pnpm protocol:check`                                                                    | `git diff` empty on `packages/protocol/src/generated`                           |
| Typecheck           | `pnpm --filter @vitals/desktop typecheck`                                                | exit 0 (strict, exactOptional, noUnchecked, verbatim)                           |
| Lint                | `pnpm --filter @vitals/desktop lint`                                                     | exit 0 (type-aware; no `any`, no floating promises)                             |
| Unit tests          | `pnpm --filter @vitals/desktop test`                                                     | all pass; coverage ≥ 70 %                                                       |
| e2e                 | `pnpm --filter @vitals/desktop e2e`                                                      | new route renders its `h2`, title `` `${label} — Vitals` ``, no console errors  |
| a11y                | (inside `pnpm e2e`) `a11y.spec.ts`                                                       | zero serious/critical WCAG 2.2 AA violations on the new route                   |
| Size budget         | `pnpm --filter @vitals/desktop build:vite; pwsh -NoProfile -File scripts/check-size.ps1` | all budgets within `size-budget.json`                                           |
| Rust (if a command) | `cargo clippy --workspace --all-targets -- -D warnings; cargo test --workspace`          | clean                                                                           |
| Everything          | `pwsh -NoProfile -File scripts/verify.ps1`                                               | exit 0                                                                          |

---

## Recipe — a competent agent could follow this blindly

1. **Decide the id and the group.** e.g. `projects`, group `more` (or `now`).
   Append at the end of its group so Ctrl+1…9 order does not shift.
2. **`apps/desktop/src/shell/navigation.ts`** (MODIFY): add `'projects'` to
   `routeIds`; add `{ id: 'projects', labelKey: 'nav.projects', icon: <Lucide>,
group: 'more' }` to `navItems`; if it is Windows-only add `'projects'` to
   `windowsOnlyRoutes`; if it is gated on a stored fact, add a `projects` field
   - setter to `useNavVisibility` and a branch in `isRouteVisible`.
3. **`packages/i18n/src/locales/en.json` and `ro.json`** (MODIFY): add
   `"nav.projects": "Projects health"` / `"Sănătatea proiectelor"` under
   `nav.*`. (Only the nav label goes here; the page's own strings go in step 5.)
4. **Create `apps/desktop/src/features/projects/`** copying `features/health`
   (polled) or `features/apps` (one-shot list). Files:
   `index.ts`, `strings.ts` (`PROJECTS_NS`, `en`/`ro`, `registerProjectsStrings`),
   `model.ts`, `api.ts` (interface + `tauri…Api` + `noHost…Api` +
   `default…Api`), `useProjects.ts`, `ProjectsScreen.tsx`. Tests:
   `useProjects.test.ts`, `ProjectsScreen.test.tsx`, `strings.test.ts`.
5. **`apps/desktop/src/routes.tsx`** (MODIFY): add `loadProjects = once(…)`
   registering its strings, `const ProjectsScreen = lazy(…)`, optionally add
   to `preloaders`, and add `case 'projects': return <ProjectsScreen />;` to
   `RouteContent` (the `never` default proves you did not forget).
6. **If it needs Rust data** — create
   `apps/desktop/src-tauri/src/projects.rs` with `#[tauri::command] pub async
fn get_projects_health() -> Result<ProjectsHealth, _>`, register it in
   `lib.rs`'s `generate_handler!` with the right `#[cfg]`, and add the type:
   ts-rs in `vitals-core` if it is a domain type (§5a), else hand-write it in
   `packages/protocol/src/commands.ts` (§5b). Run `pnpm protocol:generate`.
7. **`apps/desktop/e2e/support.ts`** (MODIFY): add a `routes` row
   `{ id: 'projects', label: 'Projects health', landmark: heading('Projects
health') }`. Add `apps/desktop/e2e/projects.spec.ts` if the page has content
   worth asserting (use `installFakeHost` from `fakeHost.ts`).
8. **`docs/tracker.csv` + `docs/TRACKER.md`** (MODIFY): add the row and the
   verification-log entry with real command output.
9. **Run** `pwsh -NoProfile -File scripts/verify.ps1`, then
   `pnpm --filter @vitals/desktop e2e`, then `pnpm protocol:check`, then
   `build:vite` + `check-size.ps1`. Fix until all pass. Then commit explicit
   paths only (no push).

---

## Traps — the things that fail the first time

1. **A route in `navigation.ts` with no `support.ts` row** fails
   `navigation.spec.ts`'s parity test. The two lists are asserted equal.
2. **`nav.projects` missing from `ro.json`** — check-drift check 4 fails
   (and the UI renders the raw key `nav.projects`).
3. **`invoke('get_projects_health')` with no matching registered command** —
   check 2 fails. Registered-but-uncalled is only a warning.
4. **A synchronous `fn` command** doing I/O — check 7 fails unless you add it
   to the allowlist _with a reason_; make it `async fn` instead.
5. **A `#[cfg(windows)]` command listed ungated** in `generate_handler!` —
   compiles on Windows, breaks the Linux CI leg (check 3b).
6. **`motion.div`** instead of `m.div` — `LazyMotion strict` throws at render.
7. **`exactOptionalPropertyTypes`**: passing `undefined` to an optional prop
   is a type error; spread `...(x !== undefined && { key: x })`.
8. **Using `aria-selected`** instead of `aria-current="page"` on the nav item
   — ignored by screen readers, and `openRoute` asserts `aria-current`.
9. **Title not an `<h2>`** — the e2e landmark (`getByRole('heading', { level:
2 })`) never matches, so the route spec fails.
10. **A skeleton that never clears** (`aria-busy="true"` stays) makes
    `settle()` time out; also never leave a no-host path in a permanent
    "loading" state.
11. **Eager-importing the screen** (not going through `routes.tsx`'s lazy
    `once()`) blows the INITIAL gzip budget.
12. **A missing `ts_rs::TS` `#[serde(rename_all = "camelCase")]`** — hard
    drift failure even though `cargo build` is green.
13. **Hand-writing a type that already exists** in
    `packages/protocol/src/generated` — forbidden (AGENTS.md § TypeScript).
14. **Editing the shared clone's tree for the page and forgetting the tracker
    rows** — `check-drift` check 8 and AGENTS.md both require them.
