---
title: Desktop widgets
description: Place a small always-on-top panel that shows one reading, wherever you want it on screen.
---

A widget is a window of its own: a small overlay that stays above other windows and shows **one**
reading from this machine — processor load, memory in use, disk activity, network throughput,
temperatures, or a compact overview. You can place several at once, each with its own content, size,
look and position.

Widgets are placed from the app's **Widgets** screen, in the sidebar between **Tracking** and
**Health**. Each row there shows a live preview of exactly what the widget displays.

## Place a widget

1. Open **Widgets** in the sidebar.
2. Choose a reading from the buttons across the top: **CPU**, **Memory**, **Disk**, **Network**,
   **Thermals** or **Overview**.
3. The widget opens at once as a small panel that stays on top. Drag it wherever you want it.
4. Adjust it in its row on the Widgets screen; the preview updates as you change anything.

You can place up to **12** widgets at once. Each one is a separate window of its own, so a dozen is
already a dozen renderers sitting on top of whatever is running underneath — the limit is a real
budget, not a round number.

Each row also says whether the widget is actually **on screen** and where it sits, because a widget
can fail to open (a window manager saying no) and the list should not pretend otherwise.

## What a widget can show

| Reading      | Shows                                                               |
| ------------ | ------------------------------------------------------------------- |
| **CPU**      | Total processor load, with the effective clock and a temperature.   |
| **Memory**   | Memory in use, as a percentage and as used of total.                |
| **Disk**     | Activity of the busiest disk, plus total read and write throughput. |
| **Network**  | Total download and upload, with the busiest adapter's name.         |
| **Thermals** | The hottest sensor first, then CPU, GPU and disk temperatures.      |
| **Overview** | CPU, memory, GPU and network together, at a glance.                 |

A reading the machine cannot measure — a temperature on a PC with no sensor, for example — is shown
as an **em dash** (—), never as zero. That is the same rule the rest of Vitals keeps: an unmeasured
reading and a genuine zero are different facts.

## Size, style and layout

Every widget is one of three sizes, in logical pixels:

| Size       | Panel     |
| ---------- | --------- |
| **Small**  | 190 × 120 |
| **Medium** | 280 × 170 |
| **Large**  | 400 × 260 |

Two styles are offered. **Plain** is nearly opaque and reads over anything. **Glass** is lighter,
with a hairline edge, for when you want the desktop to show through. Neither uses a backdrop blur:
it is expensive on a transparent Windows window and renders differently over a full-screen game,
which is exactly where an overlay earns its keep.

The readings inside the panel are arranged one of three ways:

- **Stack** — the readings as rows, one under another.
- **Split** — the readings arranged side by side.
- **Ring** — a ring gauge for a single 0–100 reading.

Only **CPU** and **Memory** are a single 0–100 percentage, so only they offer the ring; the other
readings offer the two row arrangements. A new CPU widget opens as a ring and everything else as a
column of rows. You can pick any arrangement a widget offers, at any time.

## Per-widget options

Every widget has its own settings, in its row on the Widgets screen:

- **Opacity** — how solid the panel is, from fully opaque down to 25 %. Below that the numbers stop
  being readable over a bright desktop, so that is the floor.
- **Keep on top** — whether the widget stays above other windows, including full-screen
  applications. On by default.
- **Click-through** — the widget stops taking mouse input, so a click lands on whatever is behind
  it. Off by default.

A click-through widget cannot receive the click that would turn click-through off again, so the
Widgets screen — not the widget — is how you switch it back. The same options are on the widget
itself, on hover: pin or unpin it, toggle click-through, open its settings, or close it. Closing the
panel from the widget removes it from the list too.

## Position and motion

Widgets remember where you dragged them. A new widget opens at a cascade — each one a step down and
to the right of the last — so several do not land in one spot and read as a single panel.

Movement follows the Windows **reduce motion** setting: with it on, widgets change state instantly
instead of animating.

## What the numbers are

Widgets read from the same sampler as the rest of the app, at **1 Hz** — one reading a second. The
small trace under a reading keeps about forty seconds of history, deliberately short: a widget is
glanced at, and a longer window would flatten a real spike until it stopped reading as one. When the
sampler has no reading to give, the trace breaks rather than drawing a plunge to zero that never
happened.

Each widget is a separate window with its own webview, so widgets only cost what you actually
place — none run while the screen is empty.

## Remove a widget

**Remove** in the widget's row on the Widgets screen closes its window and forgets it. Closing the
panel from the widget itself does the same.
