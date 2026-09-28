//! Prints the hardware inventory this machine reports, field by field.
//!
//! Cross-check against:
//! ```text
//! Get-CimInstance Win32_Processor, Win32_PhysicalMemory, Win32_VideoController
//! Get-CimInstance -Namespace root/Microsoft/Windows/Storage MSFT_PhysicalDisk
//! ```
//!
//! Serial numbers identify physical parts, so only their last four
//! characters are printed.
//!
//! Run with: `cargo run -p vitals-win --example prove_hardware`

#[cfg(not(windows))]
fn main() {}

#[cfg(windows)]
fn main() {
    use std::time::Instant;

    let inventory = vitals_win::hardware::read_inventory();
    println!("== inventory ({:.1} ms) ==", ms(inventory.elapsed));
    print_cpus(&inventory.cpu);
    print_memory(&inventory.memory);
    print_gpus(&inventory.gpus);
    print_drives(&inventory.drives);
    print_board(&inventory.board);

    let started = Instant::now();
    let temperatures = vitals_win::hardware::read_drive_temperatures();
    println!(
        "\n== drive temperatures ({:.1} ms) ==",
        ms(started.elapsed())
    );
    for (index, celsius) in temperatures {
        println!("  PhysicalDrive{index}  {celsius:.0} °C");
    }
}

#[cfg(windows)]
fn ms(duration: std::time::Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

#[cfg(windows)]
fn show<T: std::fmt::Display>(value: Option<T>) -> String {
    value.map_or_else(|| "—".to_owned(), |v| v.to_string())
}

#[cfg(windows)]
fn gib(bytes: Option<u64>) -> String {
    show(bytes.map(|b| format!("{:.1} GiB", b as f64 / f64::from(1_u32 << 30))))
}

#[cfg(windows)]
fn masked(serial: Option<&String>) -> String {
    show(serial.map(|s| {
        let tail: String = s
            .chars()
            .rev()
            .take(4)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        format!("…{tail}")
    }))
}

#[cfg(windows)]
fn print_cpus(cpus: &[vitals_win::hardware::CpuInfo]) {
    println!("\n== cpu ({}) ==", cpus.len());
    for cpu in cpus {
        println!("  {}", cpu.name);
        println!("    manufacturer    {}", show(cpu.manufacturer.as_ref()));
        println!("    socket          {}", show(cpu.socket.as_ref()));
        println!(
            "    cores/threads   {} / {}",
            show(cpu.cores),
            show(cpu.logical_processors)
        );
        println!("    base clock      {} MHz", show(cpu.base_clock_mhz));
        println!(
            "    L2 / L3         {} KB / {} KB",
            show(cpu.l2_cache_kb),
            show(cpu.l3_cache_kb)
        );
        println!("    virtualisation  {}", show(cpu.virtualization_enabled));
    }
}

#[cfg(windows)]
fn print_memory(memory: &vitals_win::hardware::MemoryInfo) {
    println!("\n== memory ==");
    println!("  usable          {}", gib(memory.total_bytes));
    println!("  slots           {}", show(memory.slots_total));
    println!("  max capacity    {}", gib(memory.max_capacity_bytes));
    for m in &memory.modules {
        println!(
            "  {} / {}: {} {} {} {} MT/s (configured {}) {} {} {} mV serial {}",
            show(m.slot.as_ref()),
            show(m.bank.as_ref()),
            gib(m.capacity_bytes),
            show(m.kind.as_ref()),
            show(m.form_factor.as_ref()),
            show(m.speed_mts),
            show(m.configured_speed_mts),
            show(m.manufacturer.as_ref()),
            show(m.part_number.as_ref()),
            show(m.voltage_mv),
            masked(m.serial.as_ref()),
        );
    }
}

#[cfg(windows)]
fn print_gpus(gpus: &[vitals_win::hardware::GpuInfo]) {
    println!("\n== gpus ({}) ==", gpus.len());
    for gpu in gpus {
        println!("  {}", gpu.name);
        println!("    manufacturer    {}", show(gpu.manufacturer.as_ref()));
        println!(
            "    driver          {} ({})",
            show(gpu.driver_version.as_ref()),
            show(gpu.driver_date.as_ref())
        );
        println!("    video memory    {}", gib(gpu.video_memory_bytes));
        println!(
            "    display         {} @ {} Hz",
            show(gpu.resolution.as_ref()),
            show(gpu.refresh_hz)
        );
        println!("    pnp id          {}", show(gpu.pnp_device_id.as_ref()));
    }
}

#[cfg(windows)]
fn print_drives(drives: &[vitals_win::hardware::DriveInfo]) {
    println!("\n== drives ({}) ==", drives.len());
    for d in drives {
        println!("  PhysicalDrive{}  {}", d.index, d.model);
        println!(
            "    {:?} on {}, {}, health {}, rpm {}",
            d.media,
            show(d.bus.as_ref()),
            gib(d.size_bytes),
            show(d.health.as_ref()),
            show(d.spindle_rpm)
        );
        println!(
            "    firmware {}, serial {}, temperature {}",
            show(d.firmware.as_ref()),
            masked(d.serial.as_ref()),
            show(d.temperature_celsius.map(|c| format!("{c:.0} °C")))
        );
    }
}

#[cfg(windows)]
fn print_board(board: &vitals_win::hardware::BoardInfo) {
    println!("\n== board ==");
    println!(
        "  board           {} {} {}",
        show(board.manufacturer.as_ref()),
        show(board.product.as_ref()),
        show(board.version.as_ref())
    );
    println!(
        "  bios            {} {} ({})",
        show(board.bios_vendor.as_ref()),
        show(board.bios_version.as_ref()),
        show(board.bios_date.as_ref())
    );
    println!(
        "  system          {} {}",
        show(board.system_manufacturer.as_ref()),
        show(board.system_model.as_ref())
    );
}
