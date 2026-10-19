//! Pure readers for what `wsl.exe`, `docker`, `df`, `powershell.exe` and the
//! elevated child print. No process is started here, so every rule is
//! tested against the bytes those tools really produce.

use std::path::PathBuf;

use super::super::devclean::parse_docker_size;

// ---------------------------------------------------------------------------
// wsl.exe
// ---------------------------------------------------------------------------

/// `wsl.exe` writes UTF-16LE to a pipe, without a BOM, while `wsl.exe
/// --exec` relays the Linux side's UTF-8. Decoding the first as UTF-8 leaves
/// a NUL between every letter, which is how a parser ends up comparing
/// `U\0b\0u\0n\0t\0u` with `Ubuntu` and finding no distro at all.
#[must_use]
pub fn decode_wsl(raw: &[u8]) -> String {
    let bytes = raw.strip_prefix(&[0xFF, 0xFE]).unwrap_or(raw);
    let zeros = bytes.iter().skip(1).step_by(2).filter(|b| **b == 0).count();
    let text = if bytes.len() >= 2 && zeros * 2 >= bytes.len() / 2 {
        let units: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    };
    text.replace('\0', "")
}

/// One row of `wsl -l -v`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WslDistro {
    pub name: String,
    pub default: bool,
    /// The `STATE` column as printed. Localised on some systems, so the
    /// running set comes from [`parse_wsl_names`] over `--running` instead.
    pub state: String,
}

/// Parses `wsl -l -v`: a header, then `[*] NAME STATE VERSION` rows.
#[must_use]
pub fn parse_wsl_list_verbose(raw: &[u8]) -> Vec<WslDistro> {
    decode_wsl(raw)
        .lines()
        .map(str::trim_end)
        .filter(|l| !l.trim().is_empty())
        .skip(1)
        .filter_map(|line| {
            let default = line.trim_start().starts_with('*');
            let mut cols = line.trim_start().trim_start_matches('*').split_whitespace();
            let name = cols.next()?.to_owned();
            let state = cols.next()?.to_owned();
            cols.next()?;
            Some(WslDistro {
                name,
                default,
                state,
            })
        })
        .collect()
}

/// Parses `wsl -l -q` / `wsl -l --running -q`: one name a line.
///
/// When nothing is running, `wsl` prints a sentence instead of an empty
/// list (localised, with spaces); a distro name never has a space, so a
/// line with one is that sentence and not a name.
#[must_use]
pub fn parse_wsl_names(raw: &[u8]) -> Vec<String> {
    decode_wsl(raw)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.contains(char::is_whitespace))
        .map(str::to_owned)
        .collect()
}

// ---------------------------------------------------------------------------
// Inside a distro
// ---------------------------------------------------------------------------

/// What the measurement script found inside one distro. Every figure is
/// `None` when the script could not read it, never zero.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DistroMeasure {
    pub apt: Option<u64>,
    pub dnf: Option<u64>,
    pub journal: Option<u64>,
    pub tmp_old: Option<u64>,
    /// Bytes used on the distro's root filesystem.
    pub root_used: Option<u64>,
    /// `~/.cache/<x>` folders over the threshold, with their size.
    pub caches: Vec<(u64, String)>,
    /// Rust target folders over the threshold.
    pub targets: Vec<(u64, String)>,
    /// `build` folders over the threshold.
    pub builds: Vec<(u64, String)>,
    /// Reclaimable bytes of a Docker engine running inside the distro
    /// (not Docker Desktop's, which is measured on the host).
    pub docker: Option<u64>,
}

/// Parses the measurement script's `key<TAB>value[<TAB>path]` lines.
#[must_use]
pub fn parse_distro_measure(text: &str) -> DistroMeasure {
    let mut m = DistroMeasure::default();
    for line in text.lines() {
        let mut cols = line.trim_end_matches('\r').splitn(3, '\t');
        let (Some(key), Some(value)) = (cols.next(), cols.next()) else {
            continue;
        };
        let number = value.trim().parse::<u64>().ok();
        let path = cols.next().map(str::to_owned);
        match (key, number, path) {
            ("apt", n, _) => m.apt = n,
            ("dnf", n, _) => m.dnf = n,
            ("journal", n, _) => m.journal = n,
            ("tmp", n, _) => m.tmp_old = n,
            ("root", n, _) => m.root_used = n,
            ("cache", Some(n), Some(p)) => m.caches.push((n, p)),
            ("target", Some(n), Some(p)) => m.targets.push((n, p)),
            ("build", Some(n), Some(p)) => m.builds.push((n, p)),
            ("docker", _, _) => m.docker = parse_docker_reclaimable(value),
            _ => {}
        }
    }
    m
}

/// `Images=1.2GB (40%);Build Cache=3GB;...` → the images and build cache
/// reclaimable bytes added up. `None` when neither could be read.
fn parse_docker_reclaimable(value: &str) -> Option<u64> {
    let mut total = None::<u64>;
    for part in value.split(';') {
        let Some((kind, size)) = part.split_once('=') else {
            continue;
        };
        if matches!(kind.trim(), "Images" | "Build Cache")
            && let Some(bytes) = parse_docker_size(size.trim())
        {
            total = Some(total.unwrap_or(0) + bytes);
        }
    }
    total
}

/// The `FREED <bytes>` line each inside-distro clean prints last.
#[must_use]
pub fn parse_freed(text: &str) -> Option<u64> {
    text.lines()
        .rev()
        .find_map(|l| l.trim().strip_prefix("FREED "))
        .and_then(|n| n.trim().parse().ok())
}

/// The `Used` column of `df -B1 -P` for the filesystem mounted at `mount`.
#[must_use]
pub fn parse_df_used(text: &str, mount: &str) -> Option<u64> {
    text.lines().skip(1).find_map(|line| {
        let cols: Vec<&str> = line.split_whitespace().collect();
        (cols.len() >= 6 && cols[cols.len() - 1] == mount)
            .then(|| cols[2].parse().ok())
            .flatten()
    })
}

/// Unit names of the GitHub Actions runners `systemctl list-units` shows.
#[must_use]
pub fn parse_runner_units(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| {
            l.split_whitespace()
                .find(|w| w.ends_with(".service"))
                .filter(|u| u.starts_with("actions.runner"))
        })
        .map(str::to_owned)
        .collect()
}

// ---------------------------------------------------------------------------
// Docker
// ---------------------------------------------------------------------------

/// One image row of `docker system df -v`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DockerImage {
    pub id: String,
    pub containers: Option<u64>,
    pub unique: Option<u64>,
    /// Age from `CreatedSince`, rounded down; `None` when unreadable.
    pub age_hours: Option<u64>,
}

/// One volume row of `docker system df -v`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DockerVolume {
    pub name: String,
    /// Created for a container's `VOLUME` without a name.
    pub anonymous: bool,
    /// How many containers (any state) reference it.
    pub links: Option<u64>,
    pub size: Option<u64>,
}

/// What `docker system df -v --format "{{json .}}"` reports.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DockerUsage {
    pub images: Vec<DockerImage>,
    pub volumes: Vec<DockerVolume>,
    /// Writable-layer size per container id.
    pub container_sizes: Vec<(String, Option<u64>)>,
    /// Total size of the build cache.
    pub build_cache: Option<u64>,
}

/// Parses the single JSON object `docker system df -v` prints.
#[must_use]
pub fn parse_docker_usage(text: &str) -> Option<DockerUsage> {
    let v: serde_json::Value = serde_json::from_str(text.trim()).ok()?;
    let s = |v: &serde_json::Value, k: &str| v[k].as_str().map(str::to_owned);
    let rows = |k: &str| v[k].as_array().cloned().unwrap_or_default();
    let images = rows("Images")
        .iter()
        .map(|i| DockerImage {
            id: s(i, "ID").unwrap_or_default(),
            containers: i["Containers"].as_str().and_then(|c| c.parse().ok()),
            unique: i["UniqueSize"].as_str().and_then(parse_docker_size),
            age_hours: i["CreatedSince"].as_str().and_then(parse_age_hours),
        })
        .collect();
    let volumes = rows("Volumes")
        .iter()
        .map(|vol| DockerVolume {
            name: s(vol, "Name").unwrap_or_default(),
            anonymous: vol["Labels"].as_str().is_some_and(|l| {
                l.split(',')
                    .any(|kv| kv.starts_with("com.docker.volume.anonymous"))
            }),
            links: vol["Links"].as_str().and_then(|c| c.parse().ok()),
            size: vol["Size"].as_str().and_then(parse_docker_size),
        })
        .collect();
    let container_sizes = rows("Containers")
        .iter()
        .map(|c| {
            (
                s(c, "ID").unwrap_or_default(),
                c["Size"].as_str().and_then(parse_docker_size),
            )
        })
        .collect();
    let cache = rows("BuildCache");
    let build_cache = (!cache.is_empty()).then(|| {
        cache
            .iter()
            .filter_map(|c| c["Size"].as_str().and_then(parse_docker_size))
            .sum()
    });
    Some(DockerUsage {
        images,
        volumes,
        container_sizes,
        build_cache,
    })
}

/// Docker's `CreatedSince` (`11 hours ago`, `About an hour ago`, `3 weeks
/// ago`) in whole hours.
#[must_use]
pub fn parse_age_hours(text: &str) -> Option<u64> {
    let text = text.trim().strip_suffix(" ago")?.to_ascii_lowercase();
    if text.starts_with("less than") {
        return Some(0);
    }
    let mut words = text.split_whitespace();
    let count = match words.next()? {
        "about" | "an" | "a" => {
            let next = words.next()?;
            if next == "an" || next == "a" {
                1
            } else {
                next.parse().ok()?
            }
        }
        n => n.parse::<u64>().ok()?,
    };
    let unit = words.next()?.trim_end_matches('s');
    let hours = match unit {
        "second" | "minute" => 0,
        "hour" => 1,
        "day" => 24,
        "week" => 24 * 7,
        "month" => 24 * 30,
        "year" => 24 * 365,
        _ => return None,
    };
    Some(count * hours)
}

/// One container from `docker inspect`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectedContainer {
    pub id: String,
    pub name: String,
    pub running: bool,
    pub restart_policy: String,
    /// Names of the volumes it mounts (anonymous or named).
    pub volumes: Vec<String>,
}

/// Parses the JSON array `docker inspect <ids>` prints.
#[must_use]
pub fn parse_inspect(text: &str) -> Vec<InspectedContainer> {
    let Ok(serde_json::Value::Array(rows)) = serde_json::from_str(text.trim()) else {
        return Vec::new();
    };
    rows.iter()
        .map(|c| InspectedContainer {
            id: c["Id"].as_str().unwrap_or_default().to_owned(),
            name: c["Name"]
                .as_str()
                .unwrap_or_default()
                .trim_start_matches('/')
                .to_owned(),
            running: c["State"]["Running"].as_bool().unwrap_or(true),
            restart_policy: c["HostConfig"]["RestartPolicy"]["Name"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            volumes: c["Mounts"]
                .as_array()
                .map(|ms| {
                    ms.iter()
                        .filter(|m| m["Type"].as_str() == Some("volume"))
                        .filter_map(|m| m["Name"].as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default(),
        })
        .collect()
}

/// Stopped containers that may be offered for removal: nothing brings them
/// back (`restart: no`), and every volume they mount is anonymous **and
/// measured empty**. A container that mounts `brivio_postgres_data` is how
/// someone gets their database back, even stopped — and so is one whose
/// anonymous volume holds data: the `postgres` image declares its data
/// folder a `VOLUME`, so `docker run postgres` keeps a whole database in
/// an anonymous volume (codai-pg-dev, 2026-10-19, which the first version
/// of this rule offered).
#[must_use]
pub fn removable_containers<'a>(
    containers: &'a [InspectedContainer],
    volumes: &[DockerVolume],
) -> Vec<&'a InspectedContainer> {
    containers
        .iter()
        .filter(|c| !c.running)
        .filter(|c| matches!(c.restart_policy.as_str(), "no" | ""))
        .filter(|c| {
            c.volumes.iter().all(|name| {
                volumes
                    .iter()
                    .find(|v| &v.name == name)
                    .is_some_and(|v| v.anonymous && v.size == Some(0))
            })
        })
        .collect()
}

/// Volumes no container references at all — running or stopped. Docker's
/// own `Links` count must say zero **and** no inspected container may
/// mount it: either source alone has been wrong (a stale count, a
/// container created after the listing), and a volume removed by mistake
/// is data gone.
#[must_use]
pub fn unused_volumes<'a>(
    volumes: &'a [DockerVolume],
    containers: &[InspectedContainer],
) -> Vec<&'a DockerVolume> {
    volumes
        .iter()
        .filter(|v| v.links == Some(0))
        .filter(|v| !containers.iter().any(|c| c.volumes.contains(&v.name)))
        .collect()
}

// ---------------------------------------------------------------------------
// Windows
// ---------------------------------------------------------------------------

/// A non-Microsoft scheduled task whose action runs WSL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WslTask {
    pub path: String,
    pub name: String,
    pub state: String,
}

/// Parses the `TaskPath|TaskName|State` lines the task query prints.
#[must_use]
pub fn parse_tasks(text: &str) -> Vec<WslTask> {
    text.lines()
        .filter_map(|l| {
            let mut cols = l.trim().splitn(3, '|');
            let (path, name, state) = (cols.next()?, cols.next()?, cols.next()?);
            (path.starts_with('\\') && !path.starts_with("\\Microsoft\\") && !name.is_empty()).then(
                || WslTask {
                    path: path.to_owned(),
                    name: name.to_owned(),
                    state: state.to_owned(),
                },
            )
        })
        .collect()
}

/// A running Hyper-V VM and the disks it has attached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunningVm {
    pub name: String,
    pub disks: Vec<PathBuf>,
}

/// Parses `Name|disk1;disk2` lines; `None` when the query itself failed
/// (`ERROR` on the first line), which is different from "no VM running".
#[must_use]
pub fn parse_vms(text: &str) -> Option<Vec<RunningVm>> {
    if text.trim_start().starts_with("ERROR") {
        return None;
    }
    Some(
        text.lines()
            .filter_map(|l| {
                let (name, disks) = l.trim().split_once('|')?;
                Some(RunningVm {
                    name: name.to_owned(),
                    disks: disks
                        .split(';')
                        .filter(|d| !d.is_empty())
                        .map(PathBuf::from)
                        .collect(),
                })
            })
            .collect(),
    )
}

/// What can be done about `hiberfil.sys`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hiberfil {
    /// No file, nothing to offer.
    Absent,
    /// Hibernation is on: turning it off removes the file.
    InUse { size: Option<u64> },
    /// Hibernation is off yet the file stayed. Windows removes it only when
    /// hibernation is switched on and off again; `removable` is false when
    /// that is impossible here.
    Orphan { size: Option<u64>, removable: bool },
}

/// Decides from the facts. `s4_supported` false means Windows refuses to
/// turn hibernation on at all: on a Guarded Host (VBS / Hyper-V) `powercfg
/// /h on` answers "The request is not supported", so the orphan cannot be
/// removed that way, and offering it would be offering a failure.
#[must_use]
pub fn assess_hiberfil(
    exists: bool,
    size: Option<u64>,
    enabled: bool,
    s4_supported: bool,
) -> Hiberfil {
    match (exists, enabled) {
        (false, _) => Hiberfil::Absent,
        (true, true) => Hiberfil::InUse { size },
        (true, false) => Hiberfil::Orphan {
            size,
            removable: s4_supported,
        },
    }
}

// ---------------------------------------------------------------------------
// The elevated child's progress file
// ---------------------------------------------------------------------------

/// One line the elevated child appended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgressLine {
    pub stage: String,
    pub current: Option<String>,
    pub disk_index: Option<u64>,
    pub disk_count: Option<u64>,
    /// A step's own exit code, on the line that reports it finished.
    pub code: Option<i64>,
}

/// Parses one JSON line; `None` for a malformed one, which is skipped
/// rather than ending the tail — the file is written by another process.
#[must_use]
pub fn parse_progress_line(line: &str) -> Option<ProgressLine> {
    let v: serde_json::Value =
        serde_json::from_str(line.trim().trim_start_matches('\u{feff}')).ok()?;
    let stage = v["stage"].as_str()?.to_owned();
    Some(ProgressLine {
        stage,
        current: v["current"].as_str().map(str::to_owned),
        disk_index: v["diskIndex"].as_u64(),
        disk_count: v["diskCount"].as_u64(),
        code: v["code"].as_i64(),
    })
}

/// Splits what has been read so far into complete lines and the unfinished
/// tail, which waits for the rest of its bytes.
#[must_use]
pub fn split_complete(buffer: &str) -> (Vec<&str>, &str) {
    match buffer.rfind('\n') {
        Some(end) => (
            buffer[..end]
                .lines()
                .filter(|l| !l.trim().is_empty())
                .collect(),
            &buffer[end + 1..],
        ),
        None => (Vec::new(), buffer),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf16(text: &str) -> Vec<u8> {
        text.encode_utf16().flat_map(u16::to_le_bytes).collect()
    }

    #[test]
    fn wsl_list_verbose_in_utf16_yields_each_distro_its_state_and_the_default() {
        let raw = utf16(
            "  NAME              STATE           VERSION\r\n* Ubuntu            Stopped         2\r\n  Ubuntu-24.04      Running         2\r\n  docker-desktop    Running         2\r\n",
        );
        let list = parse_wsl_list_verbose(&raw);
        assert_eq!(list.len(), 3);
        assert_eq!(
            list[0],
            WslDistro {
                name: "Ubuntu".into(),
                default: true,
                state: "Stopped".into()
            }
        );
        assert_eq!(list[1].name, "Ubuntu-24.04");
        assert!(!list[1].default);
        assert_eq!(list[2].state, "Running");
    }

    #[test]
    fn wsl_running_names_survive_a_bom_and_nothing_running_is_an_empty_list() {
        let mut raw = vec![0xFF, 0xFE];
        raw.extend(utf16("Ubuntu-24.04\r\ndocker-desktop\r\n"));
        assert_eq!(parse_wsl_names(&raw), ["Ubuntu-24.04", "docker-desktop"]);
        let none = utf16("There are no running distributions.\r\n");
        assert!(parse_wsl_names(&none).is_empty());
        assert_eq!(parse_wsl_names(b"Ubuntu\n"), ["Ubuntu"]);
    }

    #[test]
    fn the_distro_measurement_keeps_unreadable_figures_as_none_not_zero() {
        let m = parse_distro_measure(
            "apt\t1048576\ndnf\t\njournal\t734003200\ntmp\t0\nroot\t5000\ncache\t600000000\t/home/u/.cache/pip\ntarget\t9000000000\t/home/u/scrin-target\nbuild\t2000000000\t/home/u/build\ndocker\tImages=1.5GB (40%);Build Cache=500MB;Containers=1MB\n",
        );
        assert_eq!(m.apt, Some(1_048_576));
        assert_eq!(m.dnf, None);
        assert_eq!(m.tmp_old, Some(0));
        assert_eq!(m.caches, [(600_000_000, "/home/u/.cache/pip".to_owned())]);
        assert_eq!(m.targets[0].1, "/home/u/scrin-target");
        assert_eq!(m.builds[0].0, 2_000_000_000);
        assert_eq!(m.docker, Some(2_000_000_000));
        assert_eq!(parse_distro_measure("").docker, None);
    }

    #[test]
    fn df_used_is_read_for_the_exact_mount_point_only() {
        let df = "Filesystem 1-blocks Used Available Capacity Mounted on\n/dev/sdf 131798016 65289216 56232960 54% /\n/dev/sde 1081101176832 148749955072 877358866432 14% /mnt/docker-desktop-disk\n/dev/sde 1 2 3 4% /tmp/x/mnt/docker-desktop-disk\n";
        assert_eq!(parse_df_used(df, "/"), Some(65_289_216));
        assert_eq!(
            parse_df_used(df, "/mnt/docker-desktop-disk"),
            Some(148_749_955_072)
        );
        assert_eq!(parse_df_used(df, "/nowhere"), None);
    }

    #[test]
    fn docker_ages_are_read_in_hours_including_the_about_forms() {
        assert_eq!(parse_age_hours("11 hours ago"), Some(11));
        assert_eq!(parse_age_hours("About an hour ago"), Some(1));
        assert_eq!(parse_age_hours("3 weeks ago"), Some(504));
        assert_eq!(parse_age_hours("45 seconds ago"), Some(0));
        assert_eq!(parse_age_hours("Less than a second ago"), Some(0));
        assert_eq!(parse_age_hours("yesterday"), None);
    }

    fn volume(name: &str, anonymous: bool, links: Option<u64>) -> DockerVolume {
        DockerVolume {
            name: name.into(),
            anonymous,
            links,
            size: Some(1),
        }
    }

    fn container(name: &str, running: bool, policy: &str, vols: &[&str]) -> InspectedContainer {
        InspectedContainer {
            id: format!("id-{name}"),
            name: name.into(),
            running,
            restart_policy: policy.into(),
            volumes: vols.iter().map(|v| (*v).to_owned()).collect(),
        }
    }

    #[test]
    fn a_volume_any_container_uses_is_never_offered_even_when_docker_counts_zero_links() {
        let volumes = [
            volume("brivio_postgres_data", false, Some(1)),
            volume("stale-count", false, Some(0)),
            volume("orphan", false, Some(0)),
            volume("unknown", true, None),
        ];
        let containers = [container("pg", false, "no", &["stale-count"])];
        let names: Vec<&str> = unused_volumes(&volumes, &containers)
            .iter()
            .map(|v| v.name.as_str())
            .collect();
        assert_eq!(names, ["orphan"]);
    }

    #[test]
    fn only_stopped_restart_no_containers_without_named_volumes_are_removable() {
        let mut empty = volume("anon", true, Some(1));
        empty.size = Some(0);
        let volumes = [
            empty,
            volume("db", false, Some(1)),
            volume("anon-db", true, Some(1)),
        ];
        let containers = [
            container("probe", false, "no", &["anon"]),
            container("database", false, "no", &["db"]),
            container("codai-pg-dev", false, "no", &["anon-db"]),
            container("service", false, "unless-stopped", &[]),
            container("live", true, "no", &[]),
        ];
        let names: Vec<&str> = removable_containers(&containers, &volumes)
            .iter()
            .map(|c| c.name.as_str())
            .collect();
        assert_eq!(names, ["probe"]);
    }

    #[test]
    fn docker_inspect_reads_names_policies_and_only_volume_mounts() {
        let json = r#"[{"Id":"e937","Name":"/marcai-ch-test","State":{"Running":false},"HostConfig":{"RestartPolicy":{"Name":"no"}},"Mounts":[{"Type":"volume","Name":"9170b3"},{"Type":"bind","Source":"/x"}]}]"#;
        let list = parse_inspect(json);
        assert_eq!(list, [container_with_id("e937", "marcai-ch-test")]);
    }

    fn container_with_id(id: &str, name: &str) -> InspectedContainer {
        InspectedContainer {
            id: id.into(),
            name: name.into(),
            running: false,
            restart_policy: "no".into(),
            volumes: vec!["9170b3".into()],
        }
    }

    #[test]
    fn docker_usage_reads_anonymous_labels_links_and_the_build_cache_total() {
        let json = r#"{"Images":[{"ID":"sha256:1","Containers":"0","UniqueSize":"94.9MB","CreatedSince":"2 days ago"}],"Volumes":[{"Name":"9170","Labels":"com.docker.volume.anonymous=","Links":"0","Size":"870.2MB"},{"Name":"brivio_dss","Labels":"com.docker.compose.project=brivio","Links":"1","Size":"0B"}],"Containers":[{"ID":"27cd","Size":"63B"}],"BuildCache":[{"Size":"1GB"},{"Size":"0B"}]}"#;
        let u = parse_docker_usage(json).expect("parses");
        assert_eq!(u.images[0].age_hours, Some(48));
        assert_eq!(u.images[0].unique, Some(94_900_000));
        assert!(u.volumes[0].anonymous && u.volumes[0].links == Some(0));
        assert!(!u.volumes[1].anonymous);
        assert_eq!(u.container_sizes, [("27cd".to_owned(), Some(63))]);
        assert_eq!(u.build_cache, Some(1_000_000_000));
    }

    #[test]
    fn scheduled_tasks_under_microsoft_are_never_listed_as_blockers() {
        let tasks = parse_tasks(
            "\\|brivio-wsl-runner-keepalive|Running\n\\Microsoft\\Windows\\x|wsl|Ready\n\\codai\\|S1|Ready\ngarbage\n",
        );
        let names: Vec<&str> = tasks.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["brivio-wsl-runner-keepalive", "S1"]);
    }

    #[test]
    fn runner_units_and_a_failed_vm_query_are_told_apart_from_none() {
        assert_eq!(
            parse_runner_units(
                "  actions.runner.codai-ro.ws1.service loaded active running GitHub Actions Runner\n  ssh.service loaded active running OpenSSH\n"
            ),
            ["actions.runner.codai-ro.ws1.service"]
        );
        assert_eq!(
            parse_vms("ERROR: You do not have the required permission"),
            None
        );
        assert_eq!(parse_vms(""), Some(Vec::new()));
        let vms = parse_vms("vmui-mac|E:\\Hyper-V\\mac.vhdx;\n").expect("ok");
        assert_eq!(vms[0].disks, [PathBuf::from("E:\\Hyper-V\\mac.vhdx")]);
    }

    #[test]
    fn an_orphan_hiberfil_on_a_guarded_host_is_reported_but_not_removable() {
        assert_eq!(
            assess_hiberfil(true, Some(76_700_000_000), false, false),
            Hiberfil::Orphan {
                size: Some(76_700_000_000),
                removable: false
            }
        );
        assert_eq!(
            assess_hiberfil(true, None, false, true),
            Hiberfil::Orphan {
                size: None,
                removable: true
            }
        );
        assert_eq!(
            assess_hiberfil(true, Some(1), true, true),
            Hiberfil::InUse { size: Some(1) }
        );
        assert_eq!(assess_hiberfil(false, None, false, false), Hiberfil::Absent);
    }

    #[test]
    fn progress_lines_are_read_whole_and_a_half_written_tail_waits() {
        let (lines, rest) = split_complete(
            "{\"stage\":\"stopping\",\"current\":null}\n{\"stage\":\"compacting\",\"current\":\"E:\\\\a.vhdx\",\"diskIndex\":1,\"diskCount\":2}\nnot json\n{\"stage\":\"comp",
        );
        assert_eq!(rest, "{\"stage\":\"comp");
        let parsed: Vec<ProgressLine> = lines
            .iter()
            .filter_map(|l| parse_progress_line(l))
            .collect();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[1].disk_index, Some(1));
        assert_eq!(parsed[1].disk_count, Some(2));
        assert_eq!(parsed[1].current.as_deref(), Some("E:\\a.vhdx"));
        assert_eq!(
            parse_progress_line("\u{feff}{\"stage\":\"cleaning\",\"code\":3010}")
                .and_then(|p| p.code),
            Some(3010)
        );
    }

    #[test]
    fn freed_is_read_from_the_last_freed_line() {
        assert_eq!(parse_freed("x\nFREED 12\nFREED 34\n"), Some(34));
        assert_eq!(parse_freed("nothing"), None);
    }
}
