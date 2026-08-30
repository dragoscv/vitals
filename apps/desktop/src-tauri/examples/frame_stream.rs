//! Runs the sampler exactly as the app does, without a window.
//!
//! Run with:
//!   `cargo run -p vitals-desktop --release --example frame_stream`
//!
//! Proves the whole pipeline — sample, convert, delta-encode, serialise —
//! works end to end and produces sane payloads, without needing a GUI or a
//! human to watch it. If this is healthy, anything wrong is in the frontend.

use std::thread;
use std::time::{Duration, Instant};

use vitals_core::sample::FramePayload;
use vitals_win::{FrameBuilder, SystemSampler};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut sampler = SystemSampler::new();
    let mut frames = FrameBuilder::new();

    println!(
        "{:>4}  {:>9}  {:>8}  {:>8}  {:>7}  {:>9}  {:>7}",
        "SEQ", "KIND", "CHANGED", "EXITED", "CPU%", "BYTES", "SAMPLE"
    );
    println!("{}", "-".repeat(66));

    let mut total_bytes = 0_usize;
    let mut keyframe_bytes = 0_usize;

    for _ in 0..12 {
        let started = Instant::now();
        let sample = sampler.sample()?;
        let frame = frames.build(sample);
        let sample_time = started.elapsed();

        let json = serde_json::to_string(&frame)?;
        total_bytes += json.len();

        let (kind, changed, exited, cpu) = match &frame.payload {
            FramePayload::Keyframe { system, processes } => {
                keyframe_bytes = keyframe_bytes.max(json.len());
                ("keyframe", processes.len(), 0, system.cpu.total.get())
            }
            FramePayload::Delta {
                system,
                changed,
                exited,
            } => ("delta", changed.len(), exited.len(), system.cpu.total.get()),
        };

        println!(
            "{:>4}  {kind:>9}  {changed:>8}  {exited:>8}  {cpu:>6.1}%  {:>9}  {:>6.1}ms",
            frame.seq.0,
            json.len(),
            sample_time.as_secs_f64() * 1000.0
        );

        thread::sleep(Duration::from_secs(1));
    }

    let average = total_bytes / 12;
    println!("\naverage frame: {average} bytes");
    println!("largest keyframe: {keyframe_bytes} bytes");
    println!(
        "at 1 Hz that is {:.1} KB/s across the bridge",
        average as f64 / 1024.0
    );

    Ok(())
}
