//! Read-only questions to Windows, WSL and Docker for the analysis.
//!
//! Nothing here starts, stops or changes anything: a stopped distro is
//! not started to be measured, and a Hyper-V query that needs an
//! administrator answers "unknown" rather than asking for one.

use std::path::{Path, PathBuf};

use super::super::devclean::{self as dc, hidden, system32};
use super::parse;
use super::{Blocker, BlockerKind};

/// Inside-distro measurement thresholds, in bytes.
const CACHE_MIN: u64 = 500 * 1024 * 1024;
const TARGET_MIN: u64 = 1024 * 1024 * 1024;

/// The developer clean-up's project roots.
#[must_use]
pub fn project_roots() -> Vec<PathBuf> {
    dc::default_roots()
}

/// Runs `wsl.exe` with `args` and returns its raw stdout, or `None`.
pub(crate) fn wsl(args: &[&str]) -> Option<Vec<u8>> {
    let out = hidden(system32("wsl.exe")).args(args).output().ok()?;
    out.status.success().then_some(out.stdout)
}

/// The installed WSL's version, or `None` when `wsl --version` fails or
/// prints none. Read-only: it does not start the WSL VM.
#[must_use]
pub fn wsl_version() -> Option<parse::WslVersion> {
    wsl(&["--version"]).and_then(|raw| parse::parse_wsl_version(&raw))
}

/// Names of the running distros (`docker-desktop` included).
///
/// `--running -q` rather than the `STATE` column of `-l -v`, which is
/// localised ("En cours d'exécution") and would make a French Windows look
/// as if nothing ran.
#[must_use]
pub fn running_distros() -> Vec<String> {
    wsl(&["--list", "--running", "--quiet"])
        .map(|raw| parse::parse_wsl_names(&raw))
        .unwrap_or_default()
}

/// Every registered distro with its state.
#[must_use]
pub fn distros() -> Vec<parse::WslDistro> {
    wsl(&["--list", "--verbose"])
        .map(|raw| parse::parse_wsl_list_verbose(&raw))
        .unwrap_or_default()
}

/// Runs `script` as root inside a running distro via `sh -c`.
///
/// `--exec` keeps `wsl` from adding a login shell; the output is the Linux
/// side's UTF-8 (lossily decoded, a stray byte in a file name must not
/// lose the whole measurement).
pub(crate) fn in_distro(distro: &str, script: &str) -> Option<String> {
    let out = hidden(system32("wsl.exe"))
        .args(["-d", distro, "-u", "root", "--exec", "sh", "-c", script])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    (out.status.success() || !text.is_empty()).then_some(text)
}

/// The measurement script. `du -sb` per figure, and an empty value when a
/// figure could not be read (a distro without apt prints `apt<TAB>`), so
/// the parser keeps "unknown" apart from "empty".
fn measure_script() -> String {
    format!(
        r#"sz() {{ [ -e "$1" ] && du -sbx "$1" 2>/dev/null | cut -f1; }}
printf 'apt\t%s\n' "$(sz /var/cache/apt/archives)"
printf 'dnf\t%s\n' "$(sz /var/cache/dnf)"
printf 'journal\t%s\n' "$(sz /var/log/journal)"
printf 'tmp\t%s\n' "$(find /tmp -xdev -type f -mtime +2 -printf '%s\n' 2>/dev/null | awk '{{s+=$1}} END {{print s+0}}')"
printf 'root\t%s\n' "$(df -B1 -P / 2>/dev/null | awk 'NR==2 {{print $3}}')"
for d in /root/.cache/* /home/*/.cache/*; do
  [ -d "$d" ] || continue; s=$(du -sbx "$d" 2>/dev/null | cut -f1)
  [ "${{s:-0}}" -ge {CACHE_MIN} ] && printf 'cache\t%s\t%s\n' "$s" "$d"
done
find /home /root -xdev -maxdepth 6 -type d \( -name target -o -name build -o -name '*-target' \) -prune 2>/dev/null | while read -r d; do
  case "$d" in */.cache/*) continue;; esac
  p=$(dirname "$d"); n=$(basename "$d")
  if [ "$n" = target ] || [ "${{n%-target}}" != "$n" ]; then
    [ -f "$p/Cargo.toml" ] || [ -d "$d/debug" ] || [ -d "$d/release" ] || continue; k=target
  else
    [ -f "$p/package.json" ] || [ -f "$p/build.gradle" ] || [ -f "$p/build.gradle.kts" ] || [ -f "$p/CMakeLists.txt" ] || [ -f "$p/Cargo.toml" ] || continue; k=build
  fi
  s=$(du -sbx "$d" 2>/dev/null | cut -f1)
  [ "${{s:-0}}" -ge {TARGET_MIN} ] && printf '%s\t%s\t%s\n' "$k" "$s" "$d"
done
if command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1; then
  printf 'docker\t%s\n' "$(docker system df --format '{{{{.Type}}}}={{{{.Reclaimable}}}}' 2>/dev/null | tr '\n' ';')"
fi
"#
    )
}

/// Measures what can be cleaned inside a running distro.
#[must_use]
pub fn measure_distro(distro: &str) -> parse::DistroMeasure {
    in_distro(distro, &measure_script())
        .map(|t| parse::parse_distro_measure(&t))
        .unwrap_or_default()
}

/// Bytes used on `mount` inside a running distro.
#[must_use]
pub fn df_used(distro: &str, mount: &str) -> Option<u64> {
    let out = hidden(system32("wsl.exe"))
        .args(["-d", distro, "--exec", "df", "-B1", "-P", mount])
        .output()
        .ok()?;
    parse::parse_df_used(&String::from_utf8_lossy(&out.stdout), mount)
}

/// Whether the `.vhdx` is already sparse (then the sparse item is
/// pointless and is not offered).
#[must_use]
pub fn is_sparse(path: &Path) -> bool {
    use std::os::windows::fs::MetadataExt as _;
    const FILE_ATTRIBUTE_SPARSE_FILE: u32 = 0x200;
    std::fs::metadata(path).is_ok_and(|m| m.file_attributes() & FILE_ATTRIBUTE_SPARSE_FILE != 0)
}

/// `docker system df -v` from the Docker Desktop engine.
#[must_use]
pub fn docker_usage() -> Option<parse::DockerUsage> {
    let docker = dc::tool("docker")?;
    let out = hidden(docker)
        .args(["system", "df", "-v", "--format", "{{json .}}"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    parse::parse_docker_usage(&String::from_utf8_lossy(&out.stdout))
}

/// Every container, inspected (running and stopped).
#[must_use]
pub fn docker_containers() -> Vec<parse::InspectedContainer> {
    let Some(docker) = dc::tool("docker") else {
        return Vec::new();
    };
    let (ok, ids) = dc::run_tool(&docker, &["ps", "-aq", "--no-trunc"], None);
    let ids: Vec<&str> = ids
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && l.bytes().all(|b| b.is_ascii_hexdigit()))
        .collect();
    if !ok || ids.is_empty() {
        return Vec::new();
    }
    let mut args = vec!["inspect"];
    args.extend(ids);
    let out = hidden(docker).args(&args).output();
    out.map(|o| parse::parse_inspect(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default()
}

/// The size and item count of one drive's Recycle Bin.
#[must_use]
pub fn recycle_bin_size(root: &Path) -> Option<(u64, u64)> {
    use windows_sys::Win32::UI::Shell::{SHQUERYRBINFO, SHQueryRecycleBinW};
    let wide: Vec<u16> = root
        .to_string_lossy()
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let mut info = SHQUERYRBINFO {
        cbSize: size_of::<SHQUERYRBINFO>() as u32,
        i64Size: 0,
        i64NumItems: 0,
    };
    // SAFETY: `wide` is NUL-terminated; `info` is a live struct with cbSize set.
    let hr = unsafe { SHQueryRecycleBinW(wide.as_ptr(), &raw mut info) };
    (hr >= 0).then(|| {
        (
            u64::try_from(info.i64Size).unwrap_or(0),
            u64::try_from(info.i64NumItems).unwrap_or(0),
        )
    })
}

/// `HibernateEnabled` under the power key; off when the value is missing.
#[must_use]
pub fn hibernate_enabled() -> bool {
    let (ok, text) = dc::run_tool(
        &system32("reg.exe"),
        &[
            "query",
            r"HKLM\SYSTEM\CurrentControlSet\Control\Power",
            "/v",
            "HibernateEnabled",
        ],
        None,
    );
    ok && text
        .lines()
        .find(|l| l.contains("HibernateEnabled"))
        .and_then(|l| l.split_whitespace().last())
        .is_some_and(|v| v != "0x0")
}

/// Whether Windows would let hibernation be turned on at all.
///
/// `SystemS4` is reported false when "an internal system component has
/// disabled hibernation" — a Guarded Host, virtualisation-based security,
/// or the firmware. On such a machine `powercfg /h on` answers "The request
/// is not supported", which is the real case this was written for.
#[must_use]
pub fn hibernation_supported() -> bool {
    use windows::Win32::System::Power::{GetPwrCapabilities, SYSTEM_POWER_CAPABILITIES};
    let mut caps = SYSTEM_POWER_CAPABILITIES::default();
    // SAFETY: `caps` is a live, writable struct for the call.
    let ok = unsafe { GetPwrCapabilities(&raw mut caps) };
    ok && caps.SystemS4
}

/// What would be stopped to compact `disks`, and whether each comes back.
#[must_use]
pub fn blockers(running: &[String], disks: &[PathBuf]) -> Vec<Blocker> {
    let mut out = Vec::new();
    let push = |out: &mut Vec<Blocker>, kind, name: String, restart_after| {
        out.push(Blocker {
            kind,
            name,
            restart_after,
        });
    };
    for d in running {
        if d.eq_ignore_ascii_case("docker-desktop") || d.eq_ignore_ascii_case("docker-desktop-data")
        {
            continue;
        }
        push(&mut out, BlockerKind::WslDistro, d.clone(), true);
        if let Some(units) = in_distro(
            d,
            "systemctl list-units --type=service --state=running --no-legend --plain 'actions.runner*' 2>/dev/null",
        ) {
            for unit in parse::parse_runner_units(&units) {
                // A runner is a systemd service: it starts with its distro.
                push(
                    &mut out,
                    BlockerKind::GithubRunner,
                    format!("{d}: {unit}"),
                    true,
                );
            }
        }
        if in_distro(d, "pgrep -f vscode-server >/dev/null && echo yes")
            .is_some_and(|t| t.trim() == "yes")
        {
            // Not restarted by us: VS Code reconnects its own server.
            push(&mut out, BlockerKind::VscodeServer, d.clone(), false);
        }
    }
    if dc::docker_desktop_exe().is_some() {
        push(
            &mut out,
            BlockerKind::DockerDesktop,
            "Docker Desktop".to_owned(),
            true,
        );
        for c in docker_containers().into_iter().filter(|c| c.running) {
            push(&mut out, BlockerKind::Container, c.name, true);
        }
    }
    for t in wsl_tasks() {
        push(
            &mut out,
            BlockerKind::ScheduledTask,
            format!("{}{}", t.path, t.name),
            true,
        );
    }
    match running_vms() {
        Some(vms) => {
            for vm in vms.into_iter().filter(|vm| {
                vm.disks.iter().any(|d| {
                    disks
                        .iter()
                        .any(|x| x.as_os_str().eq_ignore_ascii_case(d.as_os_str()))
                })
            }) {
                // Never stopped: a VM using the disk makes the compaction fail
                // for that disk, which the report then says.
                push(&mut out, BlockerKind::HyperVVm, vm.name, false);
            }
        }
        // Hyper-V is there but would not answer this account: say so
        // rather than promise that no VM holds the disk.
        None => push(&mut out, BlockerKind::HyperVVm, "unknown".to_owned(), false),
    }
    out
}

fn powershell(script: &str) -> Option<String> {
    let (ok, text) = dc::run_tool(
        &system32("WindowsPowerShell\\v1.0\\powershell.exe"),
        &["-NoProfile", "-NonInteractive", "-Command", script],
        None,
    );
    ok.then_some(text)
}

/// Non-Microsoft scheduled tasks whose action mentions WSL — the same set
/// the elevated pass disables while WSL is down.
#[must_use]
pub fn wsl_tasks() -> Vec<parse::WslTask> {
    powershell(
        "Get-ScheduledTask -ErrorAction SilentlyContinue | Where-Object { $_.State -ne 'Disabled' -and $_.TaskPath -notlike '\\Microsoft\\*' -and ((($_.Actions | ForEach-Object { \"$($_.Execute) $($_.Arguments)\" }) -join ' ') -match '(?i)wsl') } | ForEach-Object { \"$($_.TaskPath)|$($_.TaskName)|$($_.State)\" }",
    )
    .map(|t| parse::parse_tasks(&t))
    .unwrap_or_default()
}

/// Running Hyper-V VMs and their disks; `None` when it cannot be asked
/// (Hyper-V absent, or this account is not an administrator).
#[must_use]
pub fn running_vms() -> Option<Vec<parse::RunningVm>> {
    let text = powershell(
        "if (-not (Get-Command Get-VM -ErrorAction SilentlyContinue)) { return }; try { Get-VM -ErrorAction Stop | Where-Object State -eq 'Running' | ForEach-Object { \"$($_.Name)|$((($_ | Get-VMHardDiskDrive).Path) -join ';')\" } } catch { 'ERROR ' + $_.Exception.Message }",
    )?;
    parse::parse_vms(&text)
}
