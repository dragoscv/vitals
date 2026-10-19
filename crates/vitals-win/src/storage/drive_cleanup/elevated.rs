//! The one elevated pass of a drive clean-up.
//!
//! It is the compaction child (`--elevated-compact-vhd`) with options in
//! front of the disks, so there is still exactly one elevated entry point
//! for virtual disks and one UAC prompt per run:
//!
//! ```text
//! --elevated-compact-vhd --progress <file> [--tool <managed>]... [--hiberfil]
//!                        [--sparse <distro>]... [--disk <vhdx>]...
//! ```
//!
//! The child cannot return more than an exit code, so it reports through a
//! progress file the unelevated parent created: one JSON line per stage and
//! one `result` line per item with that item's own code. The parent tails
//! it for the progress bar and reads it again for the report.
//!
//! Everything it is handed is checked again here, elevated: a disk must be
//! one [`is_known_disk`] finds, a distro one this user has registered, a
//! tool one of the closed list, the progress file a fresh file in Vitals'
//! own folder. The arguments come from our own parent, but an elevated
//! process must not become a way to compact, attach or write an arbitrary
//! path.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use super::super::devclean::{self as dc, COMPACT_VHD_ARG, hidden, is_known_disk, system32};
use super::super::managed::{self, ManagedTool};

/// Exit codes of the options form. The legacy form keeps its own.
pub mod exit {
    pub const OK: u32 = 0;
    pub const BAD_ARGS: u32 = 0xE5C2_0001;
    pub const NOT_A_DISK: u32 = 0xE5C2_0002;
    pub const NOT_A_DISTRO: u32 = 0xE5C2_0003;
    pub const BAD_PROGRESS: u32 = 0xE5C2_0004;
    /// Some items failed; their codes are in the progress file.
    pub const SOME_FAILED: u32 = 0xE5C2_0005;
    /// WSL never stopped, so no disk was touched.
    pub const WSL_STILL_RUNNING: u32 = 0xE5C2_0006;
}

/// The script's own code when WSL would not stop.
const SCRIPT_WSL_RUNNING: i32 = 32;
/// A sparse step's `result` code when WSL refused it for wanting
/// `--allow-unsafe` (WSL 2.5.6 and later), as opposed to any other failure.
/// The planner already withholds the step there; this covers a WSL updated
/// between the analysis and the run.
pub const SCRIPT_SPARSE_REFUSED: i64 = 33;
const MAX_DISKS: usize = 8;
const MAX_DISTROS: usize = 16;
const MAX_TOOLS: usize = 16;

/// What one elevated pass does.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Pass {
    pub progress: PathBuf,
    pub tools: Vec<ManagedTool>,
    pub hiberfil: bool,
    pub sparse: Vec<String>,
    pub disks: Vec<PathBuf>,
}

impl Pass {
    /// The command line for `run_elevated`, every value quoted.
    #[must_use]
    pub fn to_args(&self) -> String {
        use std::fmt::Write as _;
        let mut a = format!(
            "{COMPACT_VHD_ARG} --progress \"{}\"",
            self.progress.display()
        );
        for t in &self.tools {
            let _ = write!(a, " --tool {}", t.as_arg());
        }
        if self.hiberfil {
            a.push_str(" --hiberfil");
        }
        for d in &self.sparse {
            let _ = write!(a, " --sparse \"{d}\"");
        }
        for d in &self.disks {
            let _ = write!(a, " --disk \"{}\"", d.display());
        }
        a
    }

    /// Reads the arguments after [`COMPACT_VHD_ARG`]. `None` for anything
    /// malformed or over the limits.
    #[must_use]
    pub fn parse<I, S>(rest: I) -> Option<Self>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut pass = Self::default();
        let mut progress = None;
        let mut it = rest.into_iter();
        while let Some(flag) = it.next() {
            match flag.as_ref() {
                "--progress" => progress = Some(PathBuf::from(it.next()?.as_ref())),
                "--tool" => pass
                    .tools
                    .push(managed::parse_args([it.next()?.as_ref(), "confirmed"])?.0),
                "--hiberfil" => pass.hiberfil = true,
                "--sparse" => pass.sparse.push(it.next()?.as_ref().to_owned()),
                "--disk" => pass.disks.push(PathBuf::from(it.next()?.as_ref())),
                _ => return None,
            }
        }
        pass.progress = progress?;
        let empty = pass.tools.is_empty()
            && !pass.hiberfil
            && pass.sparse.is_empty()
            && pass.disks.is_empty();
        (!empty
            && pass.disks.len() <= MAX_DISKS
            && pass.sparse.len() <= MAX_DISTROS
            && pass.tools.len() <= MAX_TOOLS)
            .then_some(pass)
    }

    /// Whether WSL has to stop.
    #[must_use]
    pub fn stops_wsl(&self) -> bool {
        !self.disks.is_empty() || !self.sparse.is_empty()
    }
}

/// Whether the first argument after the mode is the options form.
#[must_use]
pub fn is_options_form<S: AsRef<str>>(first: Option<&S>) -> bool {
    first.is_some_and(|f| f.as_ref().starts_with("--"))
}

/// The folder progress files live in.
#[must_use]
pub fn progress_dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|l| PathBuf::from(l).join("Vitals"))
}

/// `cleanup-progress-<scan>.jsonl` in [`progress_dir`]; `scan` is digits
/// and dashes only, so it cannot climb out of the folder.
#[must_use]
pub fn progress_path(scan: &str) -> Option<PathBuf> {
    (!scan.is_empty() && scan.bytes().all(|b| b.is_ascii_digit() || b == b'-'))
        .then(|| progress_dir().map(|d| d.join(format!("cleanup-progress-{scan}.jsonl"))))
        .flatten()
}

/// Whether `path` is one a parent could have created: in Vitals' folder,
/// named like one, an existing regular file and not a link.
#[must_use]
pub fn is_progress_file(path: &Path) -> bool {
    let Some(dir) = progress_dir() else {
        return false;
    };
    let named = path
        .file_name()
        .and_then(|n| n.to_str())
        .and_then(|n| n.strip_prefix("cleanup-progress-"))
        .and_then(|n| n.strip_suffix(".jsonl"))
        .is_some_and(|scan| {
            !scan.is_empty() && scan.bytes().all(|b| b.is_ascii_digit() || b == b'-')
        });
    named
        && path
            .parent()
            .is_some_and(|p| p.as_os_str().eq_ignore_ascii_case(dir.as_os_str()))
        && std::fs::symlink_metadata(path).is_ok_and(|m| m.is_file() && !m.file_type().is_symlink())
}

/// Creates the progress file with a DACL that lets only this user (as its
/// owner), administrators and SYSTEM touch it. Inheritance is cut
/// (`D:P`), so a permissive `%LOCALAPPDATA%` ACL cannot let another account
/// feed the parent fake progress or read the paths in it.
///
/// # Errors
///
/// The Windows error when the folder or the file could not be created.
pub fn create_progress_file(path: &Path) -> std::io::Result<()> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE, LocalFree};
    use windows_sys::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows_sys::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
    use windows_sys::Win32::Storage::FileSystem::{
        CREATE_NEW, CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_READ, FILE_GENERIC_WRITE,
    };

    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let sddl: Vec<u16> = "D:P(A;;FA;;;OW)(A;;FA;;;BA)(A;;FA;;;SY)"
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let mut sd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    // SAFETY: `sddl` is NUL-terminated; `sd` is freed with LocalFree below.
    let ok = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &raw mut sd,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(std::io::Error::last_os_error());
    }
    let sa = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: sd,
        bInheritHandle: 0,
    };
    let wide: Vec<u16> = path
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain(Some(0))
        .collect();
    // SAFETY: `wide` is NUL-terminated and `sa` points at a live descriptor.
    // CREATE_NEW: a file planted beforehand (with its own ACL) is refused.
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            FILE_GENERIC_READ | FILE_GENERIC_WRITE,
            0,
            &raw const sa,
            CREATE_NEW,
            FILE_ATTRIBUTE_NORMAL,
            std::ptr::null_mut(),
        )
    };
    let err = std::io::Error::last_os_error();
    // SAFETY: allocated by ConvertStringSecurityDescriptorToSecurityDescriptorW.
    unsafe { LocalFree(sd.cast()) };
    if handle == INVALID_HANDLE_VALUE {
        return Err(err);
    }
    // SAFETY: a handle CreateFileW returned, closed exactly once.
    unsafe { CloseHandle(handle) };
    Ok(())
}

/// Appends one line from the Rust part of the child.
fn append(progress: &Path, value: &serde_json::Value) {
    if let Ok(mut f) = std::fs::OpenOptions::new().append(true).open(progress) {
        let _ = writeln!(f, "{value}");
    }
}

/// The child's whole job for the options form. Returns the exit code.
#[must_use]
pub fn perform<I, S>(rest: I) -> u32
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let Some(pass) = Pass::parse(rest) else {
        return exit::BAD_ARGS;
    };
    if !is_progress_file(&pass.progress) {
        return exit::BAD_PROGRESS;
    }
    if pass.disks.iter().any(|p| {
        p.extension()
            .is_none_or(|e| !e.eq_ignore_ascii_case("vhdx"))
            || !is_known_disk(p)
    }) {
        return exit::NOT_A_DISK;
    }
    let distros: Vec<String> = dc::wsl_distros().into_iter().map(|(n, _)| n).collect();
    if pass.sparse.iter().any(|d| !distros.iter().any(|x| x == d)) {
        return exit::NOT_A_DISTRO;
    }
    let mut failed = 0_u32;
    for tool in &pass.tools {
        append(
            &pass.progress,
            &serde_json::json!({"stage": "cleaning", "current": tool.as_arg()}),
        );
        let code = managed::perform([tool.as_arg().as_str(), "confirmed"]);
        // 3010: done, Windows finishes it at the next restart.
        if code != 0 && code != 3010 {
            failed += 1;
        }
        append(
            &pass.progress,
            &serde_json::json!({"stage": "result", "current": tool.as_arg(), "code": code}),
        );
    }
    if pass.hiberfil {
        append(
            &pass.progress,
            &serde_json::json!({"stage": "cleaning", "current": "hiberfil"}),
        );
        let code = hiberfil_cycle();
        failed += u32::from(code != 0);
        append(
            &pass.progress,
            &serde_json::json!({"stage": "result", "current": "hiberfil", "code": code}),
        );
    }
    if pass.stops_wsl() {
        let status = hidden(system32("WindowsPowerShell\\v1.0\\powershell.exe"))
            .args(["-NoProfile", "-NonInteractive", "-Command", &script(&pass)])
            .status();
        match status.map(|s| s.code()) {
            Ok(Some(0)) => {}
            Ok(Some(SCRIPT_WSL_RUNNING)) => return exit::WSL_STILL_RUNNING,
            _ => failed += 1,
        }
    }
    if failed == 0 {
        exit::OK
    } else {
        exit::SOME_FAILED
    }
}

/// `powercfg /h on` then `/h off`: the only way Windows deletes a
/// `hiberfil.sys` left behind while hibernation was already off. The first
/// code that is not zero is the result (on a Guarded Host `/h on` fails,
/// and the file stays).
fn hiberfil_cycle() -> u32 {
    let run = |arg: &str| {
        hidden(system32("powercfg.exe"))
            .args(["/hibernate", arg])
            .status()
            .ok()
            .and_then(|s| s.code())
            .map_or(u32::MAX, i32::cast_unsigned)
    };
    match run("on") {
        0 => run("off"),
        code => code,
    }
}

fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

/// The WSL part, built on the same steps as the plain compaction script —
/// pause what restarts WSL, close Docker Desktop, wait for `vmmemWSL` to go,
/// refuse with 32 if it never does, put the tasks back in `finally` — with
/// a progress line per stage and a `result` line per disk and distro.
#[must_use]
pub fn script(pass: &Pass) -> String {
    let disks = pass
        .disks
        .iter()
        .map(|p| quote(&p.to_string_lossy()))
        .collect::<Vec<_>>()
        .join(",");
    let sparse = pass
        .sparse
        .iter()
        .map(|d| quote(d))
        .collect::<Vec<_>>()
        .join(",");
    let progress = quote(&pass.progress.to_string_lossy());
    format!(
        r#"$ErrorActionPreference = 'Continue'
$pf = {progress}
$utf8 = New-Object System.Text.UTF8Encoding($false)
function Say($stage, $current, $index, $count, $code) {{
    $o = [ordered]@{{ stage = $stage; current = $current }}
    if ($null -ne $index) {{ $o.diskIndex = $index; $o.diskCount = $count }}
    if ($null -ne $code) {{ $o.code = $code }}
    [System.IO.File]::AppendAllText($pf, (($o | ConvertTo-Json -Compress) + "`n"), $utf8)
}}
$disks = @({disks})
$sparse = @({sparse})
$paused = @()
$fail = 0
try {{
    Say 'stopping' $null
    foreach ($t in @(Get-ScheduledTask -ErrorAction SilentlyContinue)) {{
        if ($t.State -eq 'Disabled' -or $t.TaskPath -like '\Microsoft\*') {{ continue }}
        $a = ($t.Actions | ForEach-Object {{ "$($_.Execute) $($_.Arguments)" }}) -join ' '
        if ($a -notmatch '(?i)wsl') {{ continue }}
        $wasRunning = $t.State -eq 'Running'
        try {{
            Disable-ScheduledTask -TaskName $t.TaskName -TaskPath $t.TaskPath -ErrorAction Stop | Out-Null
            $paused += [pscustomobject]@{{ Name = $t.TaskName; Path = $t.TaskPath; Running = $wasRunning }}
            if ($wasRunning) {{ Stop-ScheduledTask -TaskName $t.TaskName -TaskPath $t.TaskPath -ErrorAction SilentlyContinue }}
        }} catch {{ }}
    }}
    Get-Process 'Docker Desktop', 'com.docker.backend', 'com.docker.build', 'com.docker.proxy' -ErrorAction SilentlyContinue |
        Stop-Process -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 3
    $down = $false
    for ($i = 0; $i -lt 8 -and -not $down; $i++) {{
        & wsl.exe --shutdown 2>$null | Out-Null
        for ($w = 0; $w -lt 10 -and (Get-Process vmmemWSL -ErrorAction SilentlyContinue); $w++) {{ Start-Sleep -Seconds 2 }}
        $down = -not (Get-Process vmmemWSL -ErrorAction SilentlyContinue)
        if (-not $down) {{
            Get-Process wsl, wslhost -ErrorAction SilentlyContinue | Where-Object SessionId -ne 0 |
                Stop-Process -Force -ErrorAction SilentlyContinue
        }}
    }}
    if (-not $down) {{ Say 'stopping' 'wsl' $null $null 32; exit 32 }}
    Start-Sleep -Seconds 3
    $hyperv = [bool](Get-Command Optimize-VHD -ErrorAction SilentlyContinue)
    $n = $disks.Count
    for ($k = 0; $k -lt $n; $k++) {{
        $p = $disks[$k]
        Say 'compacting' $p ($k + 1) $n
        $ok = $false
        if ($hyperv) {{
            try {{ Optimize-VHD -Path $p -Mode Full -ErrorAction Stop; $ok = $true }} catch {{ }}
        }}
        if (-not $ok) {{
            $f = Join-Path $env:TEMP 'vitals-compact.txt'
            Set-Content -Path $f -Encoding ascii -Value @("select vdisk file=`"$p`"", 'attach vdisk readonly', 'compact vdisk', 'detach vdisk')
            diskpart /s $f | Out-Null
            $ok = $LASTEXITCODE -eq 0
            Remove-Item $f -ErrorAction SilentlyContinue
        }}
        if (-not $ok) {{ $fail++ }}
        Say 'result' $p ($k + 1) $n ([int](-not $ok))
    }}
    foreach ($d in $sparse) {{
        Say 'compacting' "sparse:$d"
        $env:WSL_UTF8 = '1'
        $o = (& wsl.exe --manage $d --set-sparse true 2>&1 | Out-String) -replace "`0", ''
        $c = $LASTEXITCODE
        if ($c -ne 0 -and $o -match 'allow-unsafe') {{ $c = {SCRIPT_SPARSE_REFUSED} }}
        & wsl.exe --shutdown 2>$null | Out-Null
        if ($c -ne 0) {{ $fail++ }}
        Say 'result' "sparse:$d" $null $null $c
    }}
}} finally {{
    Say 'restarting' 'tasks'
    foreach ($t in $paused) {{
        Enable-ScheduledTask -TaskName $t.Name -TaskPath $t.Path -ErrorAction SilentlyContinue | Out-Null
        if ($t.Running) {{ Start-ScheduledTask -TaskName $t.Name -TaskPath $t.Path -ErrorAction SilentlyContinue }}
    }}
}}
exit $fail
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::managed::Handler;

    fn pass() -> Pass {
        Pass {
            progress: PathBuf::from(r"C:\Users\x\AppData\Local\Vitals\cleanup-progress-3.jsonl"),
            tools: vec![
                ManagedTool::ComponentCleanup,
                ManagedTool::DiskCleanup(Handler::UpdateCleanup),
            ],
            hiberfil: true,
            sparse: vec!["Ubuntu-24.04".into()],
            disks: vec![PathBuf::from(r"C:\Users\x\Docker\docker_data.vhdx")],
        }
    }

    fn split(args: &str) -> Vec<String> {
        // What the child's argv looks like: quoted values are one argument.
        let mut out = Vec::new();
        let mut cur = String::new();
        let mut quoted = false;
        for c in args.chars() {
            match c {
                '"' => quoted = !quoted,
                ' ' if !quoted => {
                    if !cur.is_empty() {
                        out.push(std::mem::take(&mut cur));
                    }
                }
                c => cur.push(c),
            }
        }
        out.push(cur);
        out
    }

    #[test]
    fn the_options_survive_the_command_line_round_trip() {
        let p = pass();
        let argv = split(&p.to_args());
        assert_eq!(argv[0], COMPACT_VHD_ARG);
        assert!(is_options_form(argv.get(1)));
        assert_eq!(Pass::parse(&argv[1..]), Some(p));
        // The legacy form is a bare path.
        assert!(!is_options_form(Some(&r"C:\x.vhdx")));
    }

    #[test]
    fn a_pass_without_a_progress_file_or_with_nothing_to_do_or_an_unknown_flag_is_refused() {
        assert_eq!(Pass::parse(["--disk", "x.vhdx"]), None);
        assert_eq!(Pass::parse(["--progress", "p"]), None);
        assert_eq!(Pass::parse(["--progress", "p", "--disk"]), None);
        assert_eq!(
            Pass::parse(["--progress", "p", "--tool", "rm.everything"]),
            None
        );
        assert_eq!(Pass::parse(["--progress", "p", "--delete", "C:\\"]), None);
        let many: Vec<String> = std::iter::once("--progress".to_owned())
            .chain(std::iter::once("p".to_owned()))
            .chain((0..=MAX_DISKS).flat_map(|i| ["--disk".to_owned(), format!("{i}.vhdx")]))
            .collect();
        assert_eq!(Pass::parse(&many), None);
    }

    #[test]
    fn the_child_refuses_a_progress_file_it_did_not_expect_before_touching_anything() {
        let args = [
            "--progress",
            r"C:\Windows\System32\config\SAM",
            "--hiberfil",
        ];
        assert_eq!(perform(args), exit::BAD_PROGRESS);
        let traversal = [
            "--progress",
            r"C:\x\cleanup-progress-..\..\a.jsonl",
            "--hiberfil",
        ];
        assert_eq!(perform(traversal), exit::BAD_PROGRESS);
        assert_eq!(progress_path(r"..\x"), None);
        assert!(progress_path("12-3").is_some());
    }

    #[test]
    fn the_child_refuses_a_disk_it_did_not_find_even_with_a_valid_progress_file() {
        let path = progress_path("990001").expect("LOCALAPPDATA");
        let _ = std::fs::remove_file(&path);
        create_progress_file(&path).expect("creates");
        assert!(is_progress_file(&path));
        // A second create is refused: a planted file is never adopted.
        assert!(create_progress_file(&path).is_err());
        let p = path.to_string_lossy().into_owned();
        assert_eq!(
            perform(["--progress", &p, "--disk", r"C:\evil\made-up.vhdx"]),
            exit::NOT_A_DISK
        );
        assert_eq!(
            perform(["--progress", &p, "--sparse", "no-such-distro-7f3a"]),
            exit::NOT_A_DISTRO
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn the_script_reports_each_stage_and_disk_and_still_puts_tasks_back() {
        let s = script(&pass());
        let stop = s.find("Say 'stopping'").expect("stopping");
        let refuse = s.find("exit 32").expect("refuses");
        let compact = s.find("Say 'compacting' $p").expect("compacting");
        let sparse = s.find("--set-sparse true").expect("sparse");
        let finally = s.find("} finally {").expect("finally");
        let restore = s.find("Enable-ScheduledTask").expect("restores");
        assert!(
            stop < refuse
                && refuse < compact
                && compact < sparse
                && sparse < finally
                && finally < restore
        );
        assert!(s.contains("'Ubuntu-24.04'"));
        assert!(s.contains("Say 'result' $p ($k + 1) $n"));
        assert!(s.contains("AppendAllText"));
    }

    #[test]
    fn the_script_reports_a_wsl_refusal_of_sparse_apart_and_never_forces_it() {
        let s = script(&pass());
        assert!(s.contains("-match 'allow-unsafe') { $c = 33 }"));
        assert!(!s.contains("--allow-unsafe"));
    }
}
