//! # vitals-linux
//!
//! Linux backend. Implements the [`vitals_core::provider`] traits over
//! `/proc`, `/sys`, netlink and eBPF.
//!
//! Deliberately unimplemented until the Windows backend is complete. The
//! crate exists now so the trait boundary in `vitals-core` is exercised by
//! more than one consumer from the start — an abstraction with a single
//! implementation is not an abstraction, it is a guess, and it will be wrong
//! in ways nobody notices until the second platform arrives.

#![cfg(target_os = "linux")]

// Planned mapping, recorded so the eventual implementation does not have to
// rediscover it:
//
//   ProcessProvider  -> /proc/[pid]/{stat,status,io,cmdline}
//   SystemProvider   -> /proc/{stat,meminfo,diskstats,net/dev}
//   SensorProvider   -> /sys/class/hwmon, /sys/class/thermal
//   NetworkProvider  -> netlink (sock_diag) for per-socket PID attribution,
//                       nftables for blocking
//   Per-process IO   -> eBPF where available, /proc/[pid]/io otherwise
//   PowerProvider    -> /sys/devices/system/cpu/cpufreq, tlp/power-profiles-daemon
