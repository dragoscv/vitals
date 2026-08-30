// Suppress the console window on Windows release builds. A task manager that
// flashes a terminal on every launch looks broken before it has drawn a pixel.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    vitals_desktop_lib::run();
}
