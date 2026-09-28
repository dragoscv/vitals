---
title: Getting started
description: Install Vitals and find your way around the first five minutes.
---

## Install

Download [`Vitals_x64-setup.exe`](https://github.com/dragoscv/vitals/releases/latest/download/Vitals_x64-setup.exe)
and run it. There is no wizard. If Windows SmartScreen warns you, see
[Windows SmartScreen](/download/#windows-smartscreen) on the download page.

Afterwards Vitals is in the Start menu as **Vitals**.

## The first screen

The **Dashboard** shows the machine at a glance: CPU, memory, disk, network and GPU load, plus an
attention panel. When something is wrong, that panel names the cause ("a single process is
holding the disk") rather than repeating the numbers above it.

Choose **Why is my PC slow?** at any time. Vitals looks at the last minute across CPU, disk,
memory pressure, network and thermal throttling, and gives you a verdict, the processes
responsible and a chart. **Copy** puts the whole report on the clipboard for a bug report or a
support request.

## Finding your way

- **Performance** — CPU with a cell per logical processor, memory, GPU, each disk and network
  adapter, and a Thermals tab.
- **Processes** — every running program, grouped by application. Ending, suspending or resuming a
  process tells you the real consequence first; system-critical processes are protected.
- **Network** — every open connection, grouped by the program that owns it.
- **Startup** and **Services** — what runs when you sign in, and the Windows services.
- **Installed apps**, **Disk storage**, **Devices & sensors**, **Benchmarks**, **App history** and
  **Users**.

Press `Ctrl+K` to open the command palette and jump anywhere by name. Press `?` to list every
keyboard shortcut.

## What an em dash means

A value shown as **—** is one Vitals could not measure on this machine: the hardware does not
report it, or reading it needs permissions Vitals does not have. It is never a stand-in for zero.
Hover over it, or look at the section's notes, for the reason.

## Useful settings

- **Settings → General → Replace Task Manager** makes `Ctrl+Shift+Esc`, the taskbar menu and
  `Win+X` open Vitals. Windows asks for permission once. The tray menu keeps an entry to open the
  built-in Task Manager.
- **Settings → General** can keep Vitals in the tray when you close the window.
- `Ctrl+Shift+H` toggles the **HUD**, an always-on-top overlay of CPU, memory and GPU that clicks
  through to whatever is underneath.
- **Settings → Remote access** lets a phone on your network watch this PC. It is off until you turn
  it on; see [Remote access](/guides/remote-access/).
- **Settings → About** controls the automatic update check.

## Where your data lives

Everything Vitals stores — settings, history, logs — is in `%LOCALAPPDATA%\Vitals` on this machine.
Nothing is uploaded. See the [privacy summary](/privacy/).
