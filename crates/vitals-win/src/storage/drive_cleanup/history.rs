//! What the drive history needs from Windows: a name for a volume that
//! survives a change of drive letter, and the sizes an analysis measured.

use std::path::Path;

use windows_sys::Win32::Storage::FileSystem::GetVolumeNameForVolumeMountPointW;

use super::super::devclean as dc;
use super::super::drive_role::{self, is_on};
use super::Plan;

/// `\\?\Volume{GUID}\` for `mount` (`C:` or `C:\`).
///
/// The GUID belongs to the volume, not the letter: a USB disk that comes
/// back as `F:` instead of `E:` keeps its history and its schedule. `None`
/// for a network share or a drive Windows will not name, which then falls
/// back to the letter.
#[must_use]
pub fn volume_id(mount: &str) -> Option<String> {
    let root: Vec<u16> = format!("{}\\", drive_role::normalise_mount(mount))
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let mut name = [0_u16; 64];
    // SAFETY: `root` is NUL-terminated and outlives the call; the buffer
    // length passed is the buffer's own (50 characters are required).
    let ok = unsafe {
        GetVolumeNameForVolumeMountPointW(
            root.as_ptr(),
            name.as_mut_ptr(),
            u32::try_from(name.len()).unwrap_or(0),
        )
    };
    if ok == 0 {
        return None;
    }
    let end = name.iter().position(|&c| c == 0).unwrap_or(name.len());
    let id = String::from_utf16_lossy(&name[..end]);
    id.starts_with(r"\\?\Volume{").then_some(id)
}

/// The key the history and the schedule use for `mount`: the volume GUID
/// where Windows gives one, else `letter:<C:>`.
#[must_use]
pub fn volume_key(mount: &str) -> String {
    volume_id(mount).unwrap_or_else(|| {
        format!(
            "letter:{}",
            drive_role::normalise_mount(mount).to_ascii_uppercase()
        )
    })
}

/// One size worth remembering from an analysis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeasuredSize {
    pub kind: String,
    pub label: Option<String>,
    pub size: Option<u64>,
}

/// The kinds of the virtual disk files themselves, which no plan item
/// measures (compaction measures what it would give back, not the file).
pub const DISK_KINDS: [&str; 2] = ["dockerDisk", "wslDisk"];

/// What to remember of `plan`: every item's measured size, plus the size of
/// each Docker and WSL disk file on the drive — the line that answers
/// "Docker disk +42 GB since 12 Oct".
#[must_use]
pub fn measured_sizes(plan: &Plan) -> Vec<MeasuredSize> {
    let mut out: Vec<MeasuredSize> = plan
        .items
        .iter()
        // A compaction's size is what it would return, which moves with
        // the disk's own use; the disk rows below are the honest figure.
        .filter(|i| i.kind != "compactVhd" && i.kind != "sparseVhd")
        .map(|i| MeasuredSize {
            kind: i.kind.to_owned(),
            label: i.label.clone(),
            size: i.size,
        })
        .collect();
    out.extend(disk_sizes(&plan.mount, &dc::virtual_disks()));
    // One row per (kind, label): two items of one kind and label would
    // otherwise overwrite each other in the history.
    out.sort_by(|a, b| (&a.kind, &a.label).cmp(&(&b.kind, &b.label)));
    out.dedup_by(|a, b| a.kind == b.kind && a.label == b.label);
    out
}

fn disk_sizes(mount: &str, disks: &[dc::VirtualDisk]) -> Vec<MeasuredSize> {
    disks
        .iter()
        .filter(|d| is_on(&d.path, mount))
        .map(|d| MeasuredSize {
            kind: if d.docker { "dockerDisk" } else { "wslDisk" }.to_owned(),
            label: Some(
                d.distro
                    .clone()
                    .unwrap_or_else(|| file_name(&d.path).unwrap_or_default()),
            ),
            // The file's length as found; 0 there would be a real empty file.
            size: std::fs::metadata(&d.path).ok().map(|m| m.len()),
        })
        .collect()
}

fn file_name(path: &Path) -> Option<String> {
    path.file_name().map(|n| n.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn the_system_drive_has_a_volume_guid_and_the_key_uses_it() {
        let id = volume_id("C:").expect("C: has a volume name");
        assert!(
            id.starts_with(r"\\?\Volume{") && id.ends_with("}\\"),
            "{id}"
        );
        assert_eq!(volume_key("c:\\"), id);
    }

    #[test]
    fn a_letter_with_no_volume_falls_back_to_the_letter() {
        // No machine mounts every letter; find one that is free.
        let free = (b'D'..=b'Z')
            .map(char::from)
            .find(|l| !std::path::Path::new(&format!("{l}:\\")).exists())
            .expect("a free letter");
        assert_eq!(volume_id(&format!("{free}:")), None);
        assert_eq!(volume_key(&format!("{free}:")), format!("letter:{free}:"));
    }

    #[test]
    fn disk_files_on_other_drives_are_left_out_and_an_unreadable_one_is_unmeasured() {
        let disks = [
            dc::VirtualDisk {
                path: PathBuf::from(r"Q:\nowhere\docker_data.vhdx"),
                docker: true,
                distro: None,
                size: 1,
            },
            dc::VirtualDisk {
                path: PathBuf::from(r"R:\wsl\ext4.vhdx"),
                docker: false,
                distro: Some("Ubuntu".into()),
                size: 1,
            },
        ];
        let sizes = disk_sizes("Q:", &disks);
        assert_eq!(
            sizes,
            [MeasuredSize {
                kind: "dockerDisk".into(),
                label: Some("docker_data.vhdx".into()),
                size: None,
            }]
        );
    }
}
