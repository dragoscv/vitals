//! Board fan speeds from the Super-I/O chip, through `PawnIO`'s `LpcIO`.
//!
//! # What this touches, and what it does not
//!
//! The monitoring chip on a desktop board (ITE IT86xx/IT87xx, Nuvoton
//! `NCT67xx`) sits on the LPC bus. It is found by writing a vendor "enter"
//! sequence to its config port (0x2E or 0x4E), reading the chip ID, and
//! reading the base address of its hardware-monitor block; the fan counters
//! are then read through that block's index/data port pair. The only
//! writes are those protocol writes — config-mode entry and exit, logical
//! device and bank selection, and register *indexes* — never a fan, voltage
//! or PWM register. The `LpcIO` module itself refuses any port outside the
//! config pair and the BARs it discovered, so a bug here cannot reach
//! arbitrary I/O space.
//!
//! # Sharing the bus
//!
//! The config sequence is a multi-write transaction; two tools interleaving
//! it (`LibreHardwareMonitor`, `FanControl`, `HWiNFO`) would corrupt each other's
//! reads. They coordinate on the global mutex `Access_ISABUS.HTP.Method`,
//! which is what `LpcIO`'s documentation asks callers to hold, so every
//! transaction here is inside it too.
//!
//! Register maps: Linux `it87.c` / `nct6775.c` and `LibreHardwareMonitor`'s
//! `IT87XX.cs` / `Nct677X.cs`, restricted to the chips whose fan counters
//! need no per-board divisor (see `vitals_sensors::ite_chip`).

use std::time::Duration;

use vitals_sensors::{
    Fan, ITE_FAN_16BIT_ENABLE, ITE_FAN_HIGH, ITE_FAN_HIGH_ALT6, ITE_FAN_LOW, ITE_FAN_LOW_ALT6,
    LPCIO_MODULE_SHA256, NUVOTON_FAN_COUNT, SuperIoFamily, ite_chip, ite_fan_enabled, ite_rpm,
    nuvoton_chip, nuvoton_rpm, sha256_hex,
};
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_ABANDONED, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject};
use windows::core::HSTRING;

use crate::pawnio::PawnIo;

const LPCIO_MODULE: &[u8] = include_bytes!("../modules/LpcIO.bin");

const CHIP_ID: u8 = 0x20;
const CHIP_REVISION: u8 = 0x21;
const DEVICE_SELECT: u8 = 0x07;
const BASE_ADDRESS: u8 = 0x60;

const ITE_EC_LDN: u8 = 0x04;
const NUVOTON_HWM_LDN: u8 = 0x0B;
/// Both vendors put the index register at base+5 and data at base+6.
const INDEX_OFFSET: u16 = 5;
const DATA_OFFSET: u16 = 6;
const NUVOTON_BANK_SELECT: u8 = 0x4E;
/// Nuvoton `NCT679x`: bit 4 of this config register locks the HWM I/O space.
const NUVOTON_IO_SPACE_LOCK: u8 = 0x28;

const ISA_MUTEX: &str = r"Global\Access_ISABUS.HTP.Method";

/// A recognised chip whose fans can be read.
#[derive(Debug)]
pub struct SuperIo {
    dev: PawnIo,
    mutex: IsaMutex,
    name: &'static str,
    family: SuperIoFamily,
    base: u16,
}

impl SuperIo {
    /// Probes both config slots. `Ok(None)` means no supported chip — a
    /// laptop, an MSI `NCT668x` board, an unknown ID — which is an answer, not
    /// an error: the fans are simply not reported.
    pub fn open() -> Result<Option<Self>, String> {
        if sha256_hex(LPCIO_MODULE) != LPCIO_MODULE_SHA256 {
            return Err("LpcIO module sha256 mismatch".to_owned());
        }
        let mutex = IsaMutex::open()?;
        for slot in [0_u64, 1] {
            let dev = PawnIo::open()?;
            dev.load(LPCIO_MODULE)?;
            let found = {
                let _bus = mutex.lock()?;
                dev.execute("ioctl_select_slot", &[slot], 0)?;
                probe(&dev, slot)
            };
            if let Some((name, family, base)) = found {
                return Ok(Some(Self {
                    dev,
                    mutex,
                    name,
                    family,
                    base,
                }));
            }
        }
        Ok(None)
    }

    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// One read of every connected fan header.
    pub fn fans(&self) -> Result<Vec<Fan>, String> {
        let _bus = self.mutex.lock()?;
        let regs = Hwm {
            dev: &self.dev,
            base: self.base,
        };
        let mut out = Vec::new();
        match self.family {
            SuperIoFamily::Ite { fans, alt_sixth } => {
                let enable = regs.read(ITE_FAN_16BIT_ENABLE)?;
                for i in 0..fans.min(ITE_FAN_LOW.len()) {
                    if !ite_fan_enabled(i, enable) {
                        continue;
                    }
                    let (lo, hi) = if alt_sixth && i == 5 {
                        (ITE_FAN_LOW_ALT6, ITE_FAN_HIGH_ALT6)
                    } else {
                        (ITE_FAN_LOW[i], ITE_FAN_HIGH[i])
                    };
                    let count = u16::from(regs.read(lo)?) | (u16::from(regs.read(hi)?) << 8);
                    if let Some(rpm) = ite_rpm(count) {
                        out.push(fan(i, rpm));
                    }
                }
            }
            SuperIoFamily::Nuvoton { fans } => {
                for (i, &reg) in NUVOTON_FAN_COUNT.iter().take(fans).enumerate() {
                    let high = regs.read_banked(reg)?;
                    let low = regs.read_banked(reg + 1)?;
                    if let Some(rpm) = nuvoton_rpm(high, low) {
                        out.push(fan(i, rpm));
                    }
                }
                // Leave bank 0 selected, as the BIOS and other tools expect.
                regs.select_bank(0)?;
            }
        }
        Ok(out)
    }
}

fn fan(index: usize, rpm: f32) -> Fan {
    Fan {
        name: format!("Fan {}", index + 1),
        rpm: rpm.round(),
    }
}

/// Identifies the chip in the selected slot and finds its monitor block.
fn probe(dev: &PawnIo, slot: u64) -> Option<(&'static str, SuperIoFamily, u16)> {
    let config = Config { dev, slot };
    if let Some(found) = probe_ite(&config) {
        return Some(found);
    }
    probe_nuvoton(&config)
}

fn probe_ite(config: &Config<'_>) -> Option<(&'static str, SuperIoFamily, u16)> {
    config.ite_enter().ok()?;
    let found = (|| {
        let id = config.read_word(CHIP_ID).ok()?;
        let (name, family) = ite_chip(id)?;
        config.bars().ok()?;
        config.select(ITE_EC_LDN).ok()?;
        let base = config.stable_base()?;
        Some((name, family, base))
    })();
    config.ite_exit();
    found
}

fn probe_nuvoton(config: &Config<'_>) -> Option<(&'static str, SuperIoFamily, u16)> {
    config.nuvoton_enter().ok()?;
    let found = (|| {
        let id = config.read(CHIP_ID).ok()?;
        let revision = config.read(CHIP_REVISION).ok()?;
        let (name, family) = nuvoton_chip(id, revision)?;
        config.bars().ok()?;
        config.select(NUVOTON_HWM_LDN).ok()?;
        let base = config.stable_base()?;
        // A locked HWM I/O space reads as 0xFF everywhere; clearing the lock
        // bit is the one config-register write the Linux driver also makes.
        let lock = config.read(NUVOTON_IO_SPACE_LOCK).ok()?;
        if lock & 0x10 != 0 {
            config.write(NUVOTON_IO_SPACE_LOCK, lock & !0x10).ok()?;
        }
        Some((name, family, base))
    })();
    config.nuvoton_exit();
    found
}

/// The chip's configuration port pair, through `LpcIO`.
struct Config<'a> {
    dev: &'a PawnIo,
    slot: u64,
}

impl Config<'_> {
    const fn port(&self) -> u64 {
        if self.slot == 0 { 0x2E } else { 0x4E }
    }

    fn out(&self, value: u8) -> Result<(), String> {
        self.dev
            .execute("ioctl_pio_outb", &[self.port(), u64::from(value)], 0)
            .map(|_| ())
    }

    fn ite_enter(&self) -> Result<(), String> {
        let last = if self.slot == 0 { 0x55 } else { 0xAA };
        for b in [0x87, 0x01, 0x55, last] {
            self.out(b)?;
        }
        Ok(())
    }

    /// Leaves config mode — but never on the secondary slot, where some
    /// Gigabyte boards' second ITE chip stops answering after an exit.
    fn ite_exit(&self) {
        if self.slot == 0 {
            let _ = self.write(0x02, 0x02);
        }
    }

    fn nuvoton_enter(&self) -> Result<(), String> {
        self.out(0x87)?;
        self.out(0x87)
    }

    fn nuvoton_exit(&self) {
        let _ = self.out(0xAA);
    }

    fn read(&self, reg: u8) -> Result<u8, String> {
        self.dev
            .execute("ioctl_superio_inb", &[u64::from(reg)], 1)?
            .first()
            .map(|v| (*v & 0xFF) as u8)
            .ok_or_else(|| format!("superio read {reg:#x}: empty"))
    }

    fn read_word(&self, reg: u8) -> Result<u16, String> {
        Ok((u16::from(self.read(reg)?) << 8) | u16::from(self.read(reg + 1)?))
    }

    fn write(&self, reg: u8, value: u8) -> Result<(), String> {
        self.dev
            .execute("ioctl_superio_outb", &[u64::from(reg), u64::from(value)], 0)
            .map(|_| ())
    }

    fn select(&self, ldn: u8) -> Result<(), String> {
        self.write(DEVICE_SELECT, ldn)
    }

    /// Asks `LpcIO` to discover the chip's BARs, which is what unlocks
    /// `ioctl_pio_inb` on the monitor block.
    fn bars(&self) -> Result<(), String> {
        self.dev.execute("ioctl_find_bars", &[], 0).map(|_| ())
    }

    /// The monitor block's base, read twice a millisecond apart and
    /// rejected unless both agree and it is a sane, 8-aligned I/O address.
    fn stable_base(&self) -> Option<u16> {
        let a = self.read_word(BASE_ADDRESS).ok()?;
        std::thread::sleep(Duration::from_millis(1));
        let b = self.read_word(BASE_ADDRESS).ok()?;
        (a == b && a >= 0x100 && a & 0xF007 == 0).then_some(a)
    }
}

/// The hardware-monitor index/data pair.
struct Hwm<'a> {
    dev: &'a PawnIo,
    base: u16,
}

impl Hwm<'_> {
    fn outb(&self, port: u16, value: u8) -> Result<(), String> {
        self.dev
            .execute("ioctl_pio_outb", &[u64::from(port), u64::from(value)], 0)
            .map(|_| ())
    }

    fn inb(&self, port: u16) -> Result<u8, String> {
        self.dev
            .execute("ioctl_pio_inb", &[u64::from(port)], 1)?
            .first()
            .map(|v| (*v & 0xFF) as u8)
            .ok_or_else(|| format!("port {port:#x}: empty"))
    }

    fn read(&self, reg: u8) -> Result<u8, String> {
        self.outb(self.base + INDEX_OFFSET, reg)?;
        self.inb(self.base + DATA_OFFSET)
    }

    fn select_bank(&self, bank: u8) -> Result<(), String> {
        self.outb(self.base + INDEX_OFFSET, NUVOTON_BANK_SELECT)?;
        self.outb(self.base + DATA_OFFSET, bank)
    }

    fn read_banked(&self, address: u16) -> Result<u8, String> {
        self.select_bank((address >> 8) as u8)?;
        self.read((address & 0xFF) as u8)
    }
}

/// The cross-tool ISA bus mutex.
#[derive(Debug)]
struct IsaMutex(HANDLE);

// SAFETY: a mutex handle is usable from any thread; waits and releases are
// always paired on the thread that waited (see `lock`).
unsafe impl Send for IsaMutex {}

impl IsaMutex {
    fn open() -> Result<Self, String> {
        // SAFETY: plain Win32 call; the name is NUL-terminated by HSTRING.
        unsafe { CreateMutexW(None, false, &HSTRING::from(ISA_MUTEX)) }
            .map(Self)
            .map_err(|e| format!("ISA bus mutex: {e}"))
    }

    /// Waits up to 100 ms. Another tool holding the bus longer than that is
    /// mid-transaction; skipping one read is better than stalling the pipe.
    fn lock(&self) -> Result<IsaGuard<'_>, String> {
        // SAFETY: the handle is live for `self`'s lifetime.
        let r = unsafe { WaitForSingleObject(self.0, 100) };
        if r == WAIT_OBJECT_0 || r == WAIT_ABANDONED {
            Ok(IsaGuard(self))
        } else {
            Err("ISA bus busy (another monitoring tool holds it)".to_owned())
        }
    }
}

impl Drop for IsaMutex {
    fn drop(&mut self) {
        // SAFETY: created in `open`, closed once.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

struct IsaGuard<'a>(&'a IsaMutex);

impl Drop for IsaGuard<'_> {
    fn drop(&mut self) {
        // SAFETY: this thread owns the mutex (acquired in `lock`).
        let _ = unsafe { ReleaseMutex((self.0).0) };
    }
}
