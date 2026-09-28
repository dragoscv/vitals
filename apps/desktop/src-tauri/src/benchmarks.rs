//! Benchmark commands.
//!
//! Request/response for the same reason the inventories in [`crate::inventory`]
//! are: this is slow, user-initiated work that only happens while its own
//! screen is open. It is the extreme case of that argument — a suite occupies
//! every core for several seconds — and it is precisely why it must never be
//! anywhere near the sampler's 30 ms tick.
//!
//! # These DTOs are the frontend contract
//!
//! As in [`crate::inventory`], nothing from `vitals-bench` is serialised
//! directly. [`vitals_bench::BenchmarkKind`] serialises as `kebab-case`
//! because that is its persisted form, while the UI's `BenchmarkId` union is
//! camelCase; and [`vitals_bench::BenchmarkResult`] carries `variability` and
//! `trustworthy` as *methods*, which the UI would otherwise have to
//! reimplement — and would eventually reimplement differently. Both are
//! resolved here, at the boundary, where the conversion is visible.
//!
//! # What is deliberately absent
//!
//! Six of the ten ids report `available: false`. Disk benchmarks write
//! gigabytes to hardware the user owns, consuming rated endurance and evicting
//! their working set from the filesystem cache; that is an irreversible side
//! effect requiring consent and a chosen volume, not a default. GPU benchmarks
//! need a D3D12 or Vulkan device and a compiled shader, and the only graphics
//! context in this application belongs to the webview. Each is listed with its
//! reason rather than hidden, so the UI can explain the gap the same way the
//! sensors screen explains a missing core temperature.

use serde::Serialize;

#[cfg(windows)]
use vitals_bench::{BenchmarkKind, BenchmarkResult, RunConditions, Runner};

#[cfg(windows)]
use crate::commands::CommandError;

#[cfg(windows)]
type CommandResult<T> = std::result::Result<T, CommandError>;

/// One catalogue entry, for the pre-run list.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkInfoDto {
    pub id: &'static str,
    pub available: bool,
    /// A short key the UI translates, e.g. `needsConsent`. Null when the
    /// benchmark can run.
    pub unavailable_reason: Option<&'static str>,
    /// Rough seconds, so the UI can warn before committing the machine.
    pub estimated_seconds: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunConditionsDto {
    pub power_plan: Option<String>,
    pub on_battery: bool,
    pub throttled: bool,
    pub background_load: f32,
    pub ambient_start_temp: Option<f32>,
    pub ambient_end_temp: Option<f32>,
    /// Computed here rather than left to the UI, so "tainted" means one thing
    /// in the product instead of one thing per screen.
    pub tainted: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkResultDto {
    pub id: &'static str,
    pub score: f64,
    /// Every run, not just the median. The spread is the evidence that the
    /// headline figure means anything, and hiding it would make the honesty
    /// machinery in `vitals-bench` pointless.
    pub runs: Vec<f64>,
    pub unit: String,
    pub duration_ms: u64,
    /// Coefficient of variation as a percentage; null with fewer than two runs.
    pub variability: Option<f64>,
    pub trustworthy: bool,
    pub conditions: RunConditionsDto,
    pub timestamp_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkSuiteDto {
    pub results: Vec<BenchmarkResultDto>,
    pub total_duration_ms: u64,
}

#[cfg(windows)]
fn conditions_to_dto(conditions: &RunConditions) -> RunConditionsDto {
    RunConditionsDto {
        power_plan: conditions.power_plan.clone(),
        on_battery: conditions.on_battery,
        throttled: conditions.throttled,
        background_load: conditions.background_load,
        ambient_start_temp: conditions.ambient_start_temp,
        ambient_end_temp: conditions.ambient_end_temp,
        tainted: conditions.is_tainted(),
    }
}

#[cfg(windows)]
fn result_to_dto(result: &BenchmarkResult) -> BenchmarkResultDto {
    BenchmarkResultDto {
        id: result.kind.id(),
        score: result.score,
        runs: result.runs.clone(),
        unit: result.unit.clone(),
        duration_ms: result.duration_ms,
        variability: result.variability(),
        trustworthy: result.is_trustworthy(),
        conditions: conditions_to_dto(&result.conditions),
        timestamp_ms: result.timestamp_ms,
    }
}

/// The catalogue. Performs no measurement.
///
/// Cheap enough to call on every render of the benchmark screen: it is a walk
/// of a `const` slice. Separate from `run_benchmarks` so the UI can show what
/// exists, and what is missing and why, without committing the machine to
/// several seconds of full-load work first.
#[tauri::command]
#[cfg(windows)]
pub fn list_benchmarks() -> Vec<BenchmarkInfoDto> {
    vitals_bench::CATALOGUE
        .iter()
        .map(|info| BenchmarkInfoDto {
            id: info.kind.id(),
            available: info.is_available(),
            unavailable_reason: info.unavailable.map(vitals_bench::Unavailable::key),
            estimated_seconds: info.estimated_seconds,
        })
        .collect()
}

/// Runs the requested benchmarks and returns the whole suite.
///
/// Unknown and unavailable ids are refused rather than skipped. A caller that
/// asked for a disk benchmark and silently got a suite without one has been
/// misled about what was measured, which is the exact failure the honesty
/// requirements in `vitals-bench` exist to prevent.
///
/// # Errors
///
/// [`CommandError::NotFound`] for an id that is not in the catalogue, and
/// [`CommandError::Unsupported`] for one that is listed but cannot run here.
#[tauri::command]
#[cfg(windows)]
pub async fn run_benchmarks(ids: Vec<String>) -> CommandResult<BenchmarkSuiteDto> {
    // A synchronous command runs on the main thread, and a suite takes
    // seconds: the window froze — no repaint, no cancel, "Not responding" in
    // the title bar — for the whole run. On a blocking thread instead.
    tauri::async_runtime::spawn_blocking(move || run_benchmarks_blocking(&ids))
        .await
        .map_err(|err| CommandError::Internal {
            message: format!("the benchmark run was abandoned: {err}"),
        })?
}

#[cfg(windows)]
fn run_benchmarks_blocking(ids: &[String]) -> CommandResult<BenchmarkSuiteDto> {
    // Resolved before anything runs. Validating lazily would burn several
    // seconds of the user's CPU on the valid ids before refusing the request
    // as a whole.
    let kinds = ids
        .iter()
        .map(|id| {
            BenchmarkKind::from_id(id).ok_or_else(|| CommandError::NotFound {
                message: format!("no benchmark with id '{id}'"),
            })
        })
        .collect::<CommandResult<Vec<_>>>()?;

    if kinds.is_empty() {
        return Err(CommandError::NotFound {
            message: "no benchmarks were requested".into(),
        });
    }

    let mut runner = Runner::new();
    let suite = runner.run_many(&kinds).map_err(|reason| {
        // Named in the message as well as carried as a kind, because this is
        // the one error the user is most likely to see and "unsupported" on
        // its own does not say which of ten things was refused.
        let refused = kinds
            .iter()
            .find(|k| vitals_bench::unavailable_reason(**k) == Some(reason))
            .map_or("a benchmark", |k| k.id());

        CommandError::Unsupported {
            message: match reason {
                vitals_bench::Unavailable::NeedsConsent => format!(
                    "{refused} writes gigabytes to your drive, so it needs explicit consent \
                     and a chosen volume before it can run"
                ),
                vitals_bench::Unavailable::NeedsGraphicsContext => format!(
                    "{refused} needs a D3D12 or Vulkan device and a compiled shader, which \
                     this process does not create"
                ),
            },
        }
    })?;

    Ok(BenchmarkSuiteDto {
        results: suite.results.iter().map(result_to_dto).collect(),
        total_duration_ms: u64::try_from(suite.total_duration.as_millis()).unwrap_or(u64::MAX),
    })
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn the_catalogue_exposes_all_ten_ids_in_camel_case() {
        let list = list_benchmarks();
        assert_eq!(list.len(), 10);
        assert!(
            list.iter().all(|i| !i.id.contains('-')),
            "the frontend union is camelCase; a kebab id would never match"
        );
        assert_eq!(list.iter().filter(|i| i.available).count(), 4);
        assert_eq!(
            list.iter().filter(|i| !i.available).count(),
            6,
            "the four disk and two GPU benchmarks must be listed, not hidden"
        );
    }

    #[test]
    fn unavailable_entries_carry_a_reason_and_available_ones_do_not() {
        for info in list_benchmarks() {
            assert_eq!(
                info.available,
                info.unavailable_reason.is_none(),
                "{} disagrees with its own reason field",
                info.id
            );
            if !info.available {
                assert!(matches!(
                    info.unavailable_reason,
                    Some("needsConsent" | "needsGraphicsContext")
                ));
            }
        }
    }

    #[test]
    fn an_unknown_id_is_not_found() {
        let err = run_benchmarks_blocking(&["cpuSingleThreadd".into()])
            .expect_err("a typo must not silently run something else");
        assert!(matches!(err, CommandError::NotFound { .. }));
    }

    #[test]
    fn an_unavailable_id_is_refused_with_the_mechanism_named() {
        let err = run_benchmarks_blocking(&["diskSequentialWrite".into()])
            .expect_err("disk benchmarks need consent");

        match err {
            CommandError::Unsupported { message } => {
                assert!(message.contains("diskSequentialWrite"), "{message}");
                assert!(message.contains("consent"), "{message}");
            }
            other => panic!("expected Unsupported, got {other:?}"),
        }
    }

    #[test]
    fn an_empty_request_is_refused() {
        assert!(matches!(
            run_benchmarks_blocking(&[]),
            Err(CommandError::NotFound { .. })
        ));
    }

    #[test]
    fn a_cpu_suite_serialises_with_the_shape_the_frontend_expects() {
        let suite = run_benchmarks_blocking(&["cpuSingleThread".into()]).expect("CPU is available");
        assert_eq!(suite.results.len(), 1);
        assert!(suite.total_duration_ms > 0);

        let json = serde_json::to_value(&suite).expect("the DTO is plain data");
        let result = &json["results"][0];

        assert_eq!(result["id"], "cpuSingleThread");
        assert_eq!(result["unit"], "ops/s");
        assert!(result["score"].as_f64().unwrap_or(0.0) > 0.0);
        assert_eq!(
            result["runs"].as_array().map(Vec::len),
            Some(vitals_bench::RUNS_PER_BENCHMARK)
        );
        // Field names, not just values: a rename here breaks the UI silently.
        for key in [
            "id",
            "score",
            "runs",
            "unit",
            "durationMs",
            "variability",
            "trustworthy",
            "conditions",
            "timestampMs",
        ] {
            assert!(result.get(key).is_some(), "missing field {key}");
        }
        for key in [
            "powerPlan",
            "onBattery",
            "throttled",
            "backgroundLoad",
            "ambientStartTemp",
            "ambientEndTemp",
            "tainted",
        ] {
            assert!(
                result["conditions"].get(key).is_some(),
                "missing condition field {key}"
            );
        }
    }
}
