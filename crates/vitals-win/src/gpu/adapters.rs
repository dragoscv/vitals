//! GPU adapter enumeration via D3DKMT.

#![allow(non_snake_case)]

use std::ffi::c_void;

use vitals_core::ids::GpuId;
use vitals_core::units::Bytes;

use super::engines::EngineUsage;

/// `D3DKMT_HANDLE`
type D3dkmtHandle = u32;

/// Upper bound on adapters, to cap the allocation.
///
/// The kernel reports a generous capacity on the counting call — 34 on a
/// machine with two real GPUs — because it counts potential slots rather
/// than present devices. This bounds what we will allocate for.
const MAX_ADAPTERS: usize = 64;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Luid {
    low: u32,
    high: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct AdapterInfo {
    handle: D3dkmtHandle,
    luid: Luid,
    /// Video present sources — roughly, how many displays it can drive.
    ///
    /// Zero on a render-only adapter.
    num_sources: u32,
    precise_present_regions_preferred: i32,
}

#[repr(C)]
struct EnumAdapters2 {
    count: u32,
    adapters: *mut AdapterInfo,
}

#[repr(C)]
struct QueryAdapterInfo {
    handle: D3dkmtHandle,
    kind: i32,
    private_data: *mut c_void,
    private_data_size: u32,
}

#[repr(C)]
struct CloseAdapter {
    handle: D3dkmtHandle,
}

/// `KMTQAITYPE_ADAPTERREGISTRYINFO`
const QUERY_ADAPTER_REGISTRY_INFO: i32 = 8;

#[repr(C)]
#[derive(Clone, Copy)]
struct AdapterRegistryInfo {
    /// Driver-reported friendly name, e.g. `NVIDIA GeForce RTX 4090`.
    adapter_string: [u16; 260],
    bios_string: [u16; 260],
    dac_type: [u16; 260],
    chip_type: [u16; 260],
}

#[link(name = "gdi32")]
unsafe extern "system" {
    fn D3DKMTEnumAdapters2(descriptor: *mut EnumAdapters2) -> i32;
    fn D3DKMTQueryAdapterInfo(descriptor: *mut QueryAdapterInfo) -> i32;
    fn D3DKMTCloseAdapter(descriptor: *const CloseAdapter) -> i32;
}

/// A GPU as reported by the kernel.
#[derive(Debug, Clone)]
pub struct GpuAdapter {
    pub id: GpuId,
    /// Driver-reported name.
    pub name: String,
    /// Locally unique identifier, stable while the adapter is present.
    pub luid: u64,
    /// Video present sources — roughly, how many displays it can drive.
    ///
    /// Zero marks a render-only or virtual adapter, which the UI can
    /// de-emphasise rather than presenting alongside a real GPU.
    pub display_outputs: u32,
    /// Dedicated video memory, when the driver reports it.
    pub dedicated_memory: Option<Bytes>,
    /// Per-engine utilisation from the previous sample interval.
    pub engines: Vec<EngineUsage>,
}

/// An open adapter handle that closes itself.
struct AdapterHandle(D3dkmtHandle);

impl Drop for AdapterHandle {
    fn drop(&mut self) {
        let close = CloseAdapter { handle: self.0 };
        // SAFETY: `self.0` came from D3DKMTEnumAdapters2 and is closed once.
        unsafe { D3DKMTCloseAdapter(&raw const close) };
    }
}

/// Enumerates GPU adapters.
///
/// Returns an empty vector when D3DKMT is unavailable — a headless server, a
/// container, a remote session with no virtual adapter. That is a legitimate
/// machine configuration, not an error, and failing the whole sample tick
/// over it would blank every other metric.
#[must_use]
pub fn enumerate_adapters() -> Vec<GpuAdapter> {
    // Two-call protocol, and the reason a single call returns nothing:
    // passing a buffer with a guessed count makes the kernel treat that
    // count as authoritative. It must be asked first.
    //
    // Call one, with a null pointer, yields the capacity — 34 on a machine
    // with two GPUs, because it counts slots rather than devices. Call two,
    // with a buffer of that size, overwrites `count` with the real number.
    let mut descriptor = EnumAdapters2 {
        count: 0,
        adapters: std::ptr::null_mut(),
    };

    // SAFETY: a null `adapters` with `count` 0 is the documented way to
    // request the capacity.
    let status = unsafe { D3DKMTEnumAdapters2(&raw mut descriptor) };

    if status < 0 || descriptor.count == 0 {
        return Vec::new();
    }

    let capacity = (descriptor.count as usize).min(MAX_ADAPTERS);
    let mut raw = vec![AdapterInfo::default(); capacity];

    descriptor.count = u32::try_from(capacity).unwrap_or(0);
    descriptor.adapters = raw.as_mut_ptr();

    // SAFETY: `adapters` points to exactly `count` AdapterInfo structs that
    // outlive the call.
    let status = unsafe { D3DKMTEnumAdapters2(&raw mut descriptor) };

    if status < 0 {
        return Vec::new();
    }

    // The second call overwrites `count` with how many were actually
    // written, which is far smaller than the capacity.
    let count = (descriptor.count as usize).min(capacity);
    let mut out = Vec::with_capacity(count);

    for info in raw.iter().take(count) {
        // Closes when it drops, so an early continue below cannot leak.
        let handle = AdapterHandle(info.handle);

        let luid = (u64::from(info.luid.high.cast_unsigned()) << 32) | u64::from(info.luid.low);

        // Not every adapter has a registry name. Measured on this machine:
        // of three adapters the kernel reports, only the physical NVIDIA GPU
        // answers the query; the other two — a Parsec virtual display and a
        // render-only device — return OBJECT_NAME_NOT_FOUND.
        //
        // Skipping them would hide real adapters, so they get a synthetic
        // label instead. A GPU present but unnamed is still a GPU.
        let name = query_name(handle.0)
            .unwrap_or_else(|| format!("Display adapter {:#010x}", info.luid.low));

        out.push(GpuAdapter {
            id: GpuId(info.luid.low),
            name,
            luid,
            display_outputs: info.num_sources,
            // Memory needs a segment query per adapter, which is a separate
            // call; absent rather than guessed.
            dedicated_memory: None,
            engines: Vec::new(),
        });
    }

    out
}

/// Reads an adapter's friendly name.
fn query_name(handle: D3dkmtHandle) -> Option<String> {
    // SAFETY: zeroed is a valid initial state; the driver fills it in.
    let mut info: AdapterRegistryInfo = unsafe { std::mem::zeroed() };

    let mut query = QueryAdapterInfo {
        handle,
        kind: QUERY_ADAPTER_REGISTRY_INFO,
        private_data: (&raw mut info).cast(),
        private_data_size: u32::try_from(size_of::<AdapterRegistryInfo>()).unwrap_or(0),
    };

    // SAFETY: `private_data` points to a live struct of exactly the size we
    // declare in `private_data_size`.
    let status = unsafe { D3DKMTQueryAdapterInfo(&raw mut query) };

    if status < 0 {
        return None;
    }

    wide_to_string(&info.adapter_string)
}

/// Converts a NUL-terminated UTF-16 buffer to a `String`.
fn wide_to_string(buffer: &[u16]) -> Option<String> {
    let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    if end == 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buffer[..end]))
}

/// Samples GPU engine utilisation across ticks.
#[derive(Debug, Default)]
pub struct GpuSampler {
    /// The WDDM performance counters, when this machine has them.
    ///
    /// `None` on a machine with no WDDM driver — a server, a container, a VM
    /// with a basic display adapter. Adapters still enumerate there; they
    /// just report no engine data, which is the truth.
    counters: Option<super::counters::EngineCounters>,
}

impl GpuSampler {
    #[must_use]
    pub fn new() -> Self {
        Self {
            counters: super::counters::EngineCounters::open(),
        }
    }

    /// Discards baselines, so the first sample after a resume reports
    /// nothing rather than the whole paused interval.
    ///
    /// Reopening rather than clearing a map: PDH holds the baseline
    /// internally, and there is no way to reset it from outside.
    pub fn reset(&mut self) {
        self.counters = super::counters::EngineCounters::open();
    }

    /// Enumerates adapters and fills in engine utilisation.
    ///
    /// `elapsed_ticks` is unused now that utilisation comes from PDH, which
    /// does its own rate arithmetic against its own interval. It is kept in
    /// the signature because the parameter is part of the sampler contract
    /// and a vendor-SDK source would need it again.
    #[must_use]
    pub fn sample(&mut self, _elapsed_ticks: u64) -> Vec<GpuAdapter> {
        let mut adapters = enumerate_adapters();

        let Some(counters) = self.counters.as_mut() else {
            return adapters;
        };

        let samples = counters.sample();

        for adapter in &mut adapters {
            adapter.engines = engines_for(&samples, adapter.luid);
        }

        adapters
    }
}

/// Rolls per-process counter rows up into per-engine totals for one adapter.
///
/// Summed across processes, because two applications each using 40% of the 3D
/// engine leave it 80% busy. Clamped at 100 because PDH's per-process
/// percentages are computed independently and timer skew can push a sum
/// slightly past it — reporting 103% would look like a bug in us.
fn engines_for(samples: &[super::counters::EngineSample], luid: u64) -> Vec<EngineUsage> {
    let mut out: Vec<EngineUsage> = Vec::with_capacity(8);

    for sample in samples.iter().filter(|s| s.luid == luid) {
        if let Some(existing) = out.iter_mut().find(|e| e.kind == sample.kind) {
            let combined = f64::from(existing.utilisation.get()) + sample.utilisation;
            existing.utilisation = percent_clamped(combined);
        } else {
            out.push(EngineUsage {
                kind: sample.kind,
                // PDH reports an engine ordinal per instance, but the useful
                // grouping is by kind — a GPU with four copy engines is one
                // "Copy" row to a user. Node 0 is a placeholder for the
                // rolled-up row.
                node: 0,
                utilisation: percent_clamped(sample.utilisation),
            });
        }
    }

    out.sort_by(|a, b| b.utilisation.get().total_cmp(&a.utilisation.get()));
    out
}

/// Builds a `Percent` from a possibly out-of-range float.
fn percent_clamped(value: f64) -> vitals_core::units::Percent {
    #[allow(clippy::cast_possible_truncation)]
    vitals_core::units::Percent::new(value.clamp(0.0, 100.0) as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_info_matches_the_kernel_layout() {
        // The bug this catches, in full: declared at 12 bytes instead of 20,
        // the array is walked at the wrong stride. Entry 0 reads correctly
        // and every later entry is assembled from the middle of its
        // predecessor — producing handles that look plausible and fail every
        // subsequent query with INVALID_PARAMETER.
        //
        // It does not crash and it does not obviously misbehave; the GPU list
        // is simply empty. Pinning the size turns a silent wrong answer into
        // a build failure.
        assert_eq!(
            size_of::<AdapterInfo>(),
            20,
            "D3DKMT_ADAPTERINFO is 20 bytes"
        );
        assert_eq!(size_of::<EnumAdapters2>(), 16);
        assert_eq!(size_of::<AdapterRegistryInfo>(), 2080);
    }

    #[test]
    fn enumeration_does_not_panic_on_any_machine() {
        // Must work on a headless server or in a container, where D3DKMT is
        // absent entirely.
        let adapters = enumerate_adapters();
        println!("found {} adapter(s)", adapters.len());
    }

    #[test]
    fn adapters_have_names_when_present() {
        for adapter in enumerate_adapters() {
            assert!(
                !adapter.name.is_empty(),
                "an adapter reported an empty name"
            );
            assert!(
                adapter.name.len() < 260,
                "implausible adapter name: {}",
                adapter.name
            );
        }
    }

    #[test]
    fn a_machine_with_a_gpu_reports_at_least_one() {
        // Guards the regression that started this: enumeration silently
        // returned nothing on a machine with two GPUs. Skipped where there
        // genuinely is no adapter, so CI containers still pass.
        let adapters = enumerate_adapters();
        let windows_sees_one = std::path::Path::new(r"C:\Windows\System32\dxgi.dll").exists();

        if windows_sees_one {
            assert!(
                !adapters.is_empty(),
                "DXGI is present but no adapter was enumerated"
            );
        }
    }

    #[test]
    fn an_unnamed_adapter_is_still_reported() {
        // Two of three adapters on the development machine return
        // OBJECT_NAME_NOT_FOUND for the registry query. Dropping them would
        // hide real hardware, so every adapter must carry a usable label.
        for adapter in enumerate_adapters() {
            assert!(
                !adapter.name.trim().is_empty(),
                "adapter {:#x} has no usable label",
                adapter.luid
            );
        }
    }

    #[test]
    fn adapter_ids_are_unique() {
        let adapters = enumerate_adapters();
        let mut ids: Vec<_> = adapters.iter().map(|a| a.id).collect();
        let total = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), total, "duplicate GpuId");
    }

    #[test]
    fn luids_are_unique() {
        // GpuId truncates the LUID to its low word for compactness; the full
        // LUID must still distinguish adapters.
        let adapters = enumerate_adapters();
        let mut luids: Vec<_> = adapters.iter().map(|a| a.luid).collect();
        let total = luids.len();
        luids.sort_unstable();
        luids.dedup();
        assert_eq!(luids.len(), total, "duplicate LUID");
    }

    #[test]
    fn sampling_is_stable_across_calls() {
        let mut sampler = GpuSampler::new();
        let first = sampler.sample(0);
        std::thread::sleep(std::time::Duration::from_millis(100));
        let second = sampler.sample(1_000_000);

        assert_eq!(
            first.len(),
            second.len(),
            "the adapter count changed between two samples 100ms apart"
        );
    }

    #[test]
    fn unavailable_engine_data_is_absent_not_zero() {
        // Until the statistics query lands, engine data must be reported as
        // unavailable. A fabricated 0% is indistinguishable from an idle GPU
        // and would make the UI look finished when it is not.
        let mut sampler = GpuSampler::new();
        for adapter in sampler.sample(0) {
            assert!(
                adapter.engines.is_empty(),
                "engine data should be absent, not fabricated"
            );
        }
    }

    #[test]
    fn repeated_sampling_does_not_accumulate_state() {
        // The old version of this test watched a baseline map for unbounded
        // growth. PDH keeps the baseline internally now, so what is worth
        // asserting instead is that the engine list stays bounded: a leak
        // would show up as the same engine appearing once per tick.
        let mut sampler = GpuSampler::new();

        for _ in 0..50 {
            let _ = sampler.sample(1_000_000);
        }

        for adapter in sampler.sample(1_000_000) {
            assert!(
                adapter.engines.len() <= 16,
                "{} reported {} engines",
                adapter.name,
                adapter.engines.len()
            );

            // Rolled up by kind, so a kind must never appear twice.
            let mut kinds: Vec<_> = adapter.engines.iter().map(|e| e.kind).collect();
            let before = kinds.len();
            kinds.sort_unstable();
            kinds.dedup();
            assert_eq!(before, kinds.len(), "an engine kind was reported twice");
        }
    }

    #[test]
    fn wide_to_string_stops_at_the_terminator() {
        let buffer = [
            u16::from(b'R'),
            u16::from(b'T'),
            u16::from(b'X'),
            0,
            u16::from(b'Z'),
        ];
        assert_eq!(wide_to_string(&buffer).as_deref(), Some("RTX"));
    }

    #[test]
    fn wide_to_string_treats_empty_as_absent() {
        assert!(wide_to_string(&[0]).is_none());
        assert!(wide_to_string(&[]).is_none());
    }
}
