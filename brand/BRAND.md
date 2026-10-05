# Vitals brand

Version 1.0.0 · owner Dragos Catalin Vladulescu · source of truth `brand/mark.mjs`

Every asset is generated: `node brand/scripts/build.mjs`, then
`node brand/scripts/demo.mjs` to render `brand/dist/demo.html` (sizes, contexts,
lockups, motion, storyboard, palette). Never edit a generated file by hand.

## Platform

**Purpose.** Answer "why is my PC slow?" in plain words, with honest numbers.

**For whom.** Everyday people whose computer feels slow — not system
administrators. Power users get the depth underneath.

**Against.** Task Manager's opacity, "PC cleaner" scareware, and monitors that
look like a cockpit.

**Personality — this, not that**

| This               | Not that              |
| ------------------ | --------------------- |
| calm               | alarmist              |
| precise            | approximate, inflated |
| helpful            | lecturing             |
| fast and light     | heavy, bloated        |
| playful in moments | jokey everywhere      |

**Voice.** Short sentences, everyday words ("memory in use", not "working
set"), British spelling in English, correct diacritics (ș ț, never ş ţ) in
Romanian. An unmeasured number is "—", never "0".

| Context   | EN                                            | RO                                                             |
| --------- | --------------------------------------------- | -------------------------------------------------------------- |
| Status    | All clear. Nothing is slowing your PC down.   | Totul e în regulă. Nimic nu îți încetinește PC-ul.             |
| Warning   | Memory is nearly full — Chrome is using most. | Memoria e aproape plină — Chrome folosește cea mai mare parte. |
| Error     | Could not read the disk. Try again as admin.  | Discul nu a putut fi citit. Încearcă din nou ca admin.         |
| Marketing | See what your computer is actually doing.     | Vezi ce face de fapt calculatorul tău.                         |

**Naming.** "Vitals", capital V, never "VITALS" or "vitals" in prose. Feature
names are lower-case common nouns ("the dashboard", "dev clean-up").

## The mark — "All clear"

A calm line that dips and then rises past where it started. It reads at once
as a V, a sparkline and a tick: your computer is being watched, and it is fine.

- 48-unit grid; tile 42 × 42 at (3, 3), radius 10; Fluent proportions.
- Optical sizes: **master** (≥ 64 px, stroke 4.5) · **small** (24–48 px,
  stroke 5) · **micro** (16–20 px, stroke 6, nodes on the 3-unit pixel grid).
- Clear space: 6 units on every side. Minimum size: 16 px (micro).
- Files: `logo/mark*.svg`, `logo/wordmark.svg` (Inter SemiBold, outlined,
  tracking −1 %), `logo/lockup-{light,dark}.svg`, `logo/construction.svg`.
- Don't: recolour the line, add a shadow inside the tile, rotate, outline the
  tile, put the tick on a circle, or pair it with a heartbeat zig-zag (the old
  mark).

## Colour

Authored in OKLCH, proven in `contrast-pairs.json` (WCAG 2.2 AA, 0 failures;
APCA advisories reviewed — dark-mode body pairs sit at Lc 57–69, all above
WCAG 7:1).

| Role             | OKLCH                | Hex       |
| ---------------- | -------------------- | --------- |
| Fern tile top    | oklch(0.52 0.12 152) | `#227c45` |
| Fern tile bottom | oklch(0.42 0.10 152) | `#135c30` |
| UI accent, light | oklch(0.55 0.13 152) | `#22864a` |
| UI accent, dark  | oklch(0.72 0.11 152) | `#6cb882` |
| Signal lime      | oklch(0.90 0.15 135) | `#b2f48c` |
| Night            | oklch(0.20 0.03 155) | `#0a1a10` |

The app accent defaults to `green` (`--accent-hue: 152; --accent-chroma: 0.13`).
Chroma 0.17 was rejected: it clips out of sRGB and drops white-on-accent below
4.5:1. Metric colours (CPU blue, memory green, …) are categorical and do not
follow the brand.

Signal lime is for the "all clear" moment (success, OG accents) on dark only —
never text on a light background.

## Typography

- UI: Segoe UI Variable on Windows (system), Inter Variable as fallback, JetBrains
  Mono for figures. Tabular numerals always.
- Wordmark: Inter SemiBold (OFL-1.1, `OFL-Inter.txt`), outlined to paths so no
  surface needs the font. Romanian coverage checked 2026-10-05: pass.

## Motion

One curve for the mark, `cubic-bezier(0.22, 1, 0.36, 1)` — a long, calm settle.
Only transform, opacity and stroke-dashoffset animate.

| State    | What it says                                   | Time         |
| -------- | ---------------------------------------------- | ------------ |
| intro    | tile settles (scale 0.9 → 1), line draws       | 900 ms, once |
| thinking | a lime highlight travels the line ("watching") | 1.6 s loop   |
| success  | line redraws, glyph lifts 1.5 px               | 360 ms       |
| error    | three horizontal nudges (shape, not colour)    | 180 ms       |
| hover    | glyph lifts 1 px; press scales to 0.96         | 140 ms       |
| morph    | splash mark → title-bar mark (View Transition) | 380 ms       |

Where: splash (intro + thinking, then morph), storage scan, dev clean-up scan
and benchmarks (thinking). The title bar mark is static.

Reduced motion: every state shows its final frame; thinking shows a static
trace over a dimmed line so "working" is still visible.

## Exports

| Platform        | Output                                                                                                                  |
| --------------- | ----------------------------------------------------------------------------------------------------------------------- |
| Windows (Tauri) | `apps/desktop/src-tauri/icons/` — ICO 32-first + 16–256, Square*Logo                                                    |
| MSIX / Store    | `icons/msix/` — AppList targetsize 16–256 × plated/unplated/lightunplated, tiles @100–400 %, Wide310x150, StoreLogo     |
| Web             | `apps/site/public/` — favicon.svg (dark-aware) + .ico, apple-touch 180, 192/512/512-maskable, manifest, og.png 1200×630 |
| Android         | adaptive fg/bg/monochrome (phone, TV, Wear), status icon, TV banner                                                     |
| Play            | `icons/play-512.png`                                                                                                    |
| Store listing   | `icons/store/` — 9:16 poster 720×1080 and 1440×2160, 1:1 box art 1080 and 2160 (night field, mark, wordmark)            |
| Tizen           | `apps/tizen/public/icon.png`                                                                                            |
| Tray            | live CPU tile drawn in Rust (`tray.rs`), fern fill                                                                      |

## Changelog

- **1.0.0 — 2026-10-05.** First brand system. "All clear" mark (chosen over
  ring, monogram and gauge), fern palette (over teal and graphite), optical
  sizes, outlined Inter wordmark, motion set, every platform regenerated. The
  heartbeat zig-zag mark and the blue default accent are retired.
