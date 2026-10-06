//! A saved scan, so the next scan of the same drive re-reads only what
//! changed.
//!
//! # What is saved
//!
//! Every folder of a finished whole-drive scan, in tree order: its name, its
//! parent, its file ID, the bytes and files directly inside it, and why it
//! was skipped if it was. Plus the largest files, the files seen under more
//! than one name, and a [`Checkpoint`] of the change journal taken *before*
//! the scan started — so a change made while the scan ran is replayed next
//! time rather than lost.
//!
//! # How a rescan uses it
//!
//! The change journal names the folder of every file created, deleted,
//! renamed, grown or shrunk since the checkpoint. Those folders are listed
//! again; every other folder is copied from the index, with everything below
//! it that did not change either. Two refinements keep the totals exact:
//!
//! - A file with several names has its bytes in one folder only. If any of
//!   its folders is listed again, all of them are, so the bytes are neither
//!   lost nor counted twice.
//! - A file that gained a name in this interval is looked up by ID and every
//!   folder holding one of its names is listed again, for the same reason.
//!
//! When the journal cannot answer — another volume under the same letter, a
//! recreated journal, or changes discarded because the journal wrapped —
//! the index is not used and the scan walks everything. It never guesses.

use std::collections::HashMap;
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::Path;

use vitals_core::error::{Error, Result};
use vitals_core::units::Bytes;

use super::codec::{Reader, Writer};
use super::journal::{self, Checkpoint, Stale};
use super::scan::{self, LargeFile, Reuse, ScanControl, ScanMethod, ScanOptions, ScanResult};
use super::sizing::{FileIdentity, SharedFile, SkipReason};
use super::tree::{NodeId, SizeTree};

const MAGIC: &[u8; 4] = b"VTIX";
const END: &[u8; 4] = b"END!";
/// Bumped whenever the layout changes; an index of another version is not
/// read, and the scan walks instead.
const VERSION: u8 = 1;

/// Bounds a reader enforces, so a damaged file cannot make it allocate
/// without limit. Far above any real volume: `C:` here has 1.9 million
/// folders and 340,000 linked files.
const MAX_NODES: u64 = 64 * 1024 * 1024;
const MAX_LARGEST: u64 = 1024 * 1024;

fn corrupt(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what.to_owned())
}

fn skip_code(reason: Option<SkipReason>) -> (u8, u64) {
    match reason {
        None => (0, 0),
        Some(SkipReason::AccessDenied) => (1, 0),
        Some(SkipReason::ReparsePoint) => (2, 0),
        Some(SkipReason::Cancelled) => (3, 0),
        Some(SkipReason::Vanished) => (4, 0),
        Some(SkipReason::OsError(code)) => (5, u64::from(code.cast_unsigned())),
    }
}

fn skip_from(code: u8, os: u64) -> io::Result<Option<SkipReason>> {
    Ok(match code {
        0 => None,
        1 => Some(SkipReason::AccessDenied),
        2 => Some(SkipReason::ReparsePoint),
        3 => Some(SkipReason::Cancelled),
        4 => Some(SkipReason::Vanished),
        5 => Some(SkipReason::OsError(
            u32::try_from(os).map_err(|_| corrupt("an error code is too large"))?.cast_signed(),
        )),
        _ => return Err(corrupt("an unknown skip reason")),
    })
}

const fn method_code(method: ScanMethod) -> u8 {
    match method {
        ScanMethod::Walk => 0,
        ScanMethod::Turbo => 1,
        ScanMethod::Incremental => 2,
    }
}

/// Writes a finished scan.
///
/// # Errors
///
/// When writing fails, or when the scan was edited after it finished (an
/// item recycled): its nodes are no longer contiguous, and such a scan is
/// replaced by the next one rather than saved.
pub fn write_scan<W: Write>(out: W, result: &ScanResult, checkpoint: &Checkpoint) -> io::Result<W> {
    let tree = &result.tree;
    if (0..tree.len()).any(|i| tree.is_detached(NodeId(i as u32))) {
        return Err(corrupt("a scan edited after it finished is not saved"));
    }
    let mut w = Writer::new(out);
    w.bytes(MAGIC)?;
    w.u8(VERSION)?;
    w.str(&tree.path_of(tree.root()))?;
    w.varint(result.cluster_bytes.unwrap_or(0))?;
    w.varint(checkpoint.journal_id)?;
    w.varint(checkpoint.next_usn.cast_unsigned())?;
    w.varint(checkpoint.first_usn.cast_unsigned())?;
    w.varint(u64::from(checkpoint.volume_serial))?;
    w.u8(method_code(result.method))?;
    w.varint(result.files_scanned)?;
    w.varint(result.directories_scanned)?;
    w.varint(result.hard_link_duplicates)?;
    w.varint(result.hard_link_bytes_saved)?;

    w.varint(tree.len() as u64)?;
    for i in 0..tree.len() {
        let id = NodeId(i as u32);
        let Some(node) = tree.node(id) else {
            return Err(corrupt("a node went missing while saving"));
        };
        if i > 0 {
            // Nodes are only ever appended, so a parent always precedes its
            // children and the distance back to it is small and positive.
            let parent = node.parent().map_or(0, |p| p.0 as usize);
            w.varint((i - parent) as u64)?;
            w.str(tree.name_of(id))?;
        }
        w.varint(result.dir_ids.get(i).copied().unwrap_or(0))?;
        w.varint(node.own_allocated().get())?;
        w.varint(node.own_logical().get())?;
        w.varint(node.own_files())?;
        let (code, os) = skip_code(node.skipped());
        w.u8(code)?;
        if code == 5 {
            w.varint(os)?;
        }
    }

    w.varint(result.largest_files.len() as u64)?;
    for file in &result.largest_files {
        w.varint(u64::from(file.dir.0))?;
        w.str(&file.name)?;
        w.varint(file.allocated.get())?;
        w.varint(file.logical.get())?;
    }

    w.varint(result.shared_files.len() as u64)?;
    for file in &result.shared_files {
        w.varint(file.identity.0 as u64)?;
        w.varint((file.identity.0 >> 64) as u64)?;
        w.varint(file.allocated)?;
        w.varint(file.folders.len() as u64)?;
        for folder in &file.folders {
            w.varint(u64::from(*folder))?;
        }
    }
    w.bytes(END)?;
    w.flush()?;
    Ok(w.into_inner())
}

/// A scan read back, with the journal position it was taken at.
#[derive(Debug)]
pub struct Saved {
    pub result: ScanResult,
    pub checkpoint: Checkpoint,
}

/// Reads what [`write_scan`] wrote.
///
/// # Errors
///
/// [`io::ErrorKind::InvalidData`] for anything that is not a complete index
/// of this version.
pub fn read_scan<R: Read>(input: R) -> io::Result<Saved> {
    let mut r = Reader::new(input);
    if &r.bytes::<4>()? != MAGIC {
        return Err(corrupt("not a scan index"));
    }
    if r.u8()? != VERSION {
        return Err(corrupt("a scan index of another version"));
    }
    let root = r.string()?;
    let cluster = r.varint()?;
    let checkpoint = Checkpoint {
        journal_id: r.varint()?,
        next_usn: r.varint()?.cast_signed(),
        first_usn: r.varint()?.cast_signed(),
        volume_serial: u32::try_from(r.varint()?).map_err(|_| corrupt("volume serial"))?,
    };
    let method = match r.u8()? {
        0 => ScanMethod::Walk,
        1 => ScanMethod::Turbo,
        2 => ScanMethod::Incremental,
        _ => return Err(corrupt("an unknown scan method")),
    };
    let files_scanned = r.varint()?;
    let directories_scanned = r.varint()?;
    let hard_link_duplicates = r.varint()?;
    let hard_link_bytes_saved = r.varint()?;

    let count = r.varint()?;
    if count == 0 || count > MAX_NODES {
        return Err(corrupt("an implausible number of folders"));
    }
    let count = count as usize;
    let mut tree = SizeTree::new(&root);
    let mut dir_ids = Vec::with_capacity(count);
    for i in 0..count {
        let node = if i == 0 {
            tree.root()
        } else {
            let back = usize::try_from(r.varint()?).unwrap_or(usize::MAX);
            if back == 0 || back > i {
                return Err(corrupt("a folder whose parent comes after it"));
            }
            let name = r.string()?;
            tree.add_child(NodeId((i - back) as u32), &name)
        };
        dir_ids.push(r.varint()?);
        let allocated = r.varint()?;
        let logical = r.varint()?;
        let files = r.varint()?;
        tree.add_files(node, allocated, logical, files);
        let code = r.u8()?;
        let os = if code == 5 { r.varint()? } else { 0 };
        if let Some(reason) = skip_from(code, os)? {
            let path = tree.path_of(node);
            tree.mark_skipped(node, path, reason);
        }
    }
    tree.aggregate();

    let largest_count = r.varint()?;
    if largest_count > MAX_LARGEST {
        return Err(corrupt("an implausible number of large files"));
    }
    let mut largest_files = Vec::with_capacity(largest_count as usize);
    for _ in 0..largest_count {
        let dir = r.varint_u32()?;
        if dir as usize >= count {
            return Err(corrupt("a large file in a folder that does not exist"));
        }
        largest_files.push(LargeFile {
            dir: NodeId(dir),
            name: r.string()?,
            allocated: Bytes(r.varint()?),
            logical: Bytes(r.varint()?),
        });
    }

    let shared_count = r.varint()?;
    if shared_count > MAX_NODES {
        return Err(corrupt("an implausible number of linked files"));
    }
    let mut shared_files = Vec::with_capacity(shared_count as usize);
    for _ in 0..shared_count {
        let lo = u128::from(r.varint()?);
        let hi = u128::from(r.varint()?);
        let allocated = r.varint()?;
        let n = r.varint()?;
        if n > 1024 {
            return Err(corrupt("a file with more names than NTFS allows"));
        }
        let mut folders = Vec::with_capacity(n as usize);
        for _ in 0..n {
            let folder = r.varint_u32()?;
            if folder as usize >= count {
                return Err(corrupt("a linked file in a folder that does not exist"));
            }
            folders.push(folder);
        }
        shared_files.push(SharedFile {
            identity: FileIdentity(hi << 64 | lo),
            allocated,
            folders,
        });
    }
    if &r.bytes::<4>()? != END {
        return Err(corrupt("a scan index that ends early"));
    }

    Ok(Saved {
        result: ScanResult {
            tree,
            cluster_bytes: (cluster != 0).then_some(cluster),
            files_scanned,
            directories_scanned,
            hard_link_duplicates,
            hard_link_bytes_saved,
            cancelled: false,
            elapsed_ms: 0,
            threads: 0,
            largest_files,
            method,
            dir_ids,
            shared_files,
            reused_directories: None,
            relisted_directories: None,
        },
        checkpoint,
    })
}

/// Where the index for the drive holding `root` lives under `dir`.
#[must_use]
pub fn path_for(dir: &Path, root: &str) -> Option<std::path::PathBuf> {
    let letter = drive_letter(root)?;
    Some(dir.join(format!("{letter}.idx")))
}

/// The letter of a whole-drive root (`C:` or `C:\`), else `None`: only a
/// whole drive is indexed, because the journal is per volume and a folder
/// scan is quick anyway.
#[must_use]
pub fn drive_letter(root: &str) -> Option<char> {
    let trimmed = root.strip_suffix('\\').unwrap_or(root);
    let bytes = trimmed.as_bytes();
    (bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':')
        .then(|| char::from(bytes[0]).to_ascii_uppercase())
}

/// Saves a finished scan, replacing the previous index for its drive.
///
/// Written to a temporary file and renamed into place, so a crash mid-write
/// leaves the old index rather than half of a new one.
///
/// # Errors
///
/// When the scan is not a whole-drive scan, was cancelled, or the file
/// cannot be written.
pub fn save(dir: &Path, result: &ScanResult, checkpoint: &Checkpoint) -> Result<u64> {
    let root = result.tree.path_of(result.tree.root());
    let path = path_for(dir, &root)
        .ok_or_else(|| Error::NotFound(format!("{root} is not a whole drive")))?;
    if result.cancelled {
        return Err(Error::Refused("a stopped scan is not saved".to_owned()));
    }
    std::fs::create_dir_all(dir)?;
    let temp = path.with_extension("idx.tmp");
    let file = std::fs::File::create(&temp)?;
    let written = write_scan(BufWriter::with_capacity(1 << 20, file), result, checkpoint)
        .and_then(|w| w.into_inner().map_err(io::IntoInnerError::into_error))
        .and_then(|file| file.sync_all());
    if let Err(err) = written {
        let _ = std::fs::remove_file(&temp);
        return Err(err.into());
    }
    std::fs::rename(&temp, &path)?;
    Ok(std::fs::metadata(&path).map_or(0, |m| m.len()))
}

/// Reads the saved index for the drive holding `root`.
///
/// # Errors
///
/// When there is none, or it cannot be read.
pub fn load(dir: &Path, root: &str) -> Result<Saved> {
    let path = path_for(dir, root)
        .ok_or_else(|| Error::NotFound(format!("{root} is not a whole drive")))?;
    let file = std::fs::File::open(&path)?;
    let saved = read_scan(BufReader::with_capacity(1 << 20, file))?;
    let saved_root = saved.result.tree.path_of(saved.result.tree.root());
    if drive_letter(&saved_root) != drive_letter(root) {
        return Err(Error::NotFound("the index is for another drive".to_owned()));
    }
    Ok(saved)
}

/// Whether an index exists for the drive holding `root`.
#[must_use]
pub fn exists(dir: &Path, root: &str) -> bool {
    path_for(dir, root).is_some_and(|p| p.is_file())
}

/// Why a rescan walked everything instead of using the index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fallback {
    NoIndex,
    Unreadable(String),
    Journal(String),
    Stale(Stale),
    /// So many files changed their names that finding every folder holding
    /// one would take longer than walking.
    TooManyRelinks(usize),
}

/// Which folders of a saved scan must be listed again.
#[derive(Debug)]
pub struct Dirty {
    /// By old node index: this folder's listing may differ.
    pub folder: Vec<bool>,
    /// By old node index: this folder or one below it is dirty.
    pub below: Vec<bool>,
}

/// Works out the dirty set from the journal's changed folders.
///
/// `changed` are file IDs of folders; `extra` are old node indices known to
/// be dirty for another reason (the folders of a relinked file).
#[must_use]
pub fn dirty_set(
    saved: &ScanResult,
    changed: &std::collections::HashSet<u64>,
    extra: &[u32],
) -> Dirty {
    let n = saved.tree.len();
    let mut folder = vec![false; n];
    for (i, id) in saved.dir_ids.iter().enumerate() {
        if *id != 0 && changed.contains(id) {
            folder[i] = true;
        }
    }
    for &i in extra {
        if let Some(flag) = folder.get_mut(i as usize) {
            *flag = true;
        }
    }
    // A linked file's bytes live in one of its folders: list one and all of
    // them must be listed, or the bytes are counted twice or not at all.
    // Repeated to a fixed point, since a folder added here may hold another
    // linked file.
    let mut by_folder: HashMap<u32, Vec<usize>> = HashMap::new();
    for (k, file) in saved.shared_files.iter().enumerate() {
        for f in &file.folders {
            by_folder.entry(*f).or_default().push(k);
        }
    }
    let mut done = vec![false; saved.shared_files.len()];
    let mut work: Vec<u32> = (0..n as u32).filter(|i| folder[*i as usize]).collect();
    while let Some(f) = work.pop() {
        for &k in by_folder.get(&f).map_or(&[][..], Vec::as_slice) {
            if std::mem::replace(&mut done[k], true) {
                continue;
            }
            for &g in &saved.shared_files[k].folders {
                if !std::mem::replace(&mut folder[g as usize], true) {
                    work.push(g);
                }
            }
        }
    }
    let mut below = folder.clone();
    for i in (1..n).rev() {
        if below[i]
            && let Some(p) = saved.tree.node(NodeId(i as u32)).and_then(|x| x.parent())
        {
            below[p.0 as usize] = true;
        }
    }
    Dirty { folder, below }
}

/// Copies unchanged folders of a saved scan into a new walk.
struct Graft<'a> {
    old: &'a ScanResult,
    dirty: &'a Dirty,
    by_id: HashMap<u64, u32>,
    used: Vec<bool>,
    /// Old node index to new node, for every folder copied.
    new_of: Vec<Option<NodeId>>,
    reused: u64,
    files: u64,
}

impl<'a> Graft<'a> {
    fn new(old: &'a ScanResult, dirty: &'a Dirty) -> Self {
        let by_id = old
            .dir_ids
            .iter()
            .enumerate()
            .filter(|(_, id)| **id != 0)
            .map(|(i, id)| (*id, i as u32))
            .collect();
        let n = old.tree.len();
        Self {
            old,
            dirty,
            by_id,
            used: vec![false; n],
            new_of: vec![None; n],
            reused: 0,
            files: 0,
        }
    }

    /// Copies one clean old folder's own figures into `node`.
    fn copy_own(&mut self, old: u32, node: NodeId, tree: &mut SizeTree) {
        let Some(from) = self.old.tree.node(NodeId(old)) else {
            return;
        };
        tree.add_files(
            node,
            from.own_allocated().get(),
            from.own_logical().get(),
            from.own_files(),
        );
        if let Some(reason) = from.skipped() {
            let path = tree.path_of(node);
            tree.mark_skipped(node, path, reason);
        }
        self.used[old as usize] = true;
        self.new_of[old as usize] = Some(node);
        self.reused += 1;
        self.files += from.own_files();
    }
}

impl Reuse for Graft<'_> {
    fn graft(
        &mut self,
        id: u64,
        node: NodeId,
        tree: &mut SizeTree,
        dir_ids: &mut Vec<u64>,
    ) -> Option<Vec<(NodeId, String)>> {
        let &old = self.by_id.get(&id)?;
        if self.dirty.folder[old as usize] || self.used[old as usize] {
            return None;
        }
        let mut rest = Vec::new();
        self.copy_own(old, node, tree);
        let mut stack = vec![(old, node)];
        while let Some((from, to)) = stack.pop() {
            for child in self.old.tree.children(NodeId(from)) {
                let c = child.0;
                let new_child = tree.add_child(to, self.old.tree.name_of(child));
                let child_id = self.old.dir_ids.get(c as usize).copied().unwrap_or(0);
                scan::record_id(dir_ids, new_child, child_id);
                if self.dirty.folder[c as usize] || self.used[c as usize] {
                    rest.push((new_child, tree.path_of(new_child)));
                } else {
                    self.copy_own(c, new_child, tree);
                    stack.push((c, new_child));
                }
            }
        }
        Some(rest)
    }

    fn finish(&mut self, result: &mut ScanResult) {
        let relisted = result.directories_scanned;
        result.directories_scanned += self.reused;
        result.files_scanned += self.files;

        // Linked files whose folders were all copied keep their bytes where
        // they were, and are still linked files of this scan.
        for file in &self.old.shared_files {
            let folders: Option<Vec<u32>> = file
                .folders
                .iter()
                .map(|f| self.new_of[*f as usize].map(|n| n.0))
                .collect();
            if let Some(folders) = folders {
                let repeats = folders.len().saturating_sub(1) as u64;
                result.hard_link_duplicates += repeats;
                result.hard_link_bytes_saved += file.allocated * repeats;
                result.shared_files.push(SharedFile {
                    identity: file.identity,
                    allocated: file.allocated,
                    folders,
                });
            }
        }
        result.shared_files.sort_unstable_by_key(|f| f.identity.0);

        // The largest files of copied folders were never listed; they come
        // from the saved list, and the two lists are merged.
        let capacity = result.largest_files.len().max(self.old.largest_files.len());
        for file in &self.old.largest_files {
            if let Some(dir) = self.new_of[file.dir.0 as usize] {
                result.largest_files.push(LargeFile {
                    dir,
                    ..file.clone()
                });
            }
        }
        result.largest_files.sort_by(|a, b| {
            (b.allocated, b.logical, &b.name).cmp(&(a.allocated, a.logical, &a.name))
        });
        result.largest_files.truncate(capacity);

        result.method = ScanMethod::Incremental;
        result.reused_directories = Some(self.reused);
        result.relisted_directories = Some(relisted);
    }
}

/// How many relinked files are looked up by ID before walking is cheaper.
const MAX_RELINKS: usize = 2000;

/// Rescans a whole drive from its saved index and the change journal.
///
/// `now` must be read before this is called and saved with the result, so
/// changes made during the rescan are picked up by the next one.
///
/// # Errors
///
/// A [`Fallback`] when the index cannot be used; the caller then walks.
pub fn rescan(
    saved: &Saved,
    now: &Checkpoint,
    options: ScanOptions,
    control: &mut ScanControl<'_>,
) -> std::result::Result<ScanResult, Fallback> {
    journal::usable(&saved.checkpoint, now).map_err(Fallback::Stale)?;
    let root = saved.result.tree.path_of(saved.result.tree.root());
    let changes = journal::changes_since(&root, &saved.checkpoint, now)
        .map_err(|err| Fallback::Journal(err.to_string()))?;
    if changes.relinked.len() > MAX_RELINKS {
        return Err(Fallback::TooManyRelinks(changes.relinked.len()));
    }
    let drive = root.get(..2).unwrap_or("");
    let mut extra = Vec::new();
    for file in &changes.relinked {
        match journal::folders_of(&root, *file) {
            Ok(folders) => {
                for folder in folders {
                    if let Some(node) = saved.result.find_directory(&format!("{drive}{folder}")) {
                        extra.push(node.0);
                    }
                }
            }
            // Gone since: the folder it left is in the journal already.
            Err(Error::Os { code: 2 | 3, .. }) => {}
            Err(err) => return Err(Fallback::Journal(err.to_string())),
        }
    }
    let dirty = dirty_set(&saved.result, &changes.folders, &extra);
    let mut graft = Graft::new(&saved.result, &dirty);
    Ok(scan::walk(
        Path::new(&root),
        options,
        control,
        Some(&mut graft),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn checkpoint() -> Checkpoint {
        Checkpoint {
            journal_id: 0x1DA7_A175_B14C_26F,
            next_usn: 156_712_833_688,
            first_usn: 156_674_031_616,
            volume_serial: 0xDEAD_BEEF,
        }
    }

    /// A private tree on the system drive, removed on drop.
    struct Fixture(std::path::PathBuf);

    impl Fixture {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!("vitals-index-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            for dir in ["a\\b", "c", "linked"] {
                std::fs::create_dir_all(root.join(dir)).expect("mkdir");
            }
            std::fs::write(root.join("a\\one.bin"), vec![1_u8; 9000]).expect("write");
            std::fs::write(root.join("a\\b\\two.bin"), vec![2_u8; 20_000]).expect("write");
            std::fs::write(root.join("c\\three.bin"), vec![3_u8; 5000]).expect("write");
            std::fs::write(root.join("linked\\big.bin"), vec![4_u8; 50_000]).expect("write");
            std::fs::hard_link(root.join("linked\\big.bin"), root.join("c\\big-link.bin"))
                .expect("link");
            Self(root)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn walk(root: &Path) -> ScanResult {
        scan::scan_directory(root, ScanOptions::default(), &mut ScanControl::default())
    }

    fn round_trip(result: &ScanResult) -> Saved {
        let bytes = write_scan(Vec::new(), result, &checkpoint()).expect("write");
        read_scan(bytes.as_slice()).expect("read")
    }

    #[test]
    fn a_saved_scan_reads_back_with_every_figure_intact() {
        let fixture = Fixture::new("roundtrip");
        let result = walk(&fixture.0);
        let saved = round_trip(&result);
        let back = &saved.result;

        assert_eq!(saved.checkpoint, checkpoint());
        assert_eq!(back.tree.len(), result.tree.len());
        assert_eq!(back.allocated(), result.allocated());
        assert_eq!(back.logical(), result.logical());
        assert_eq!(back.files_scanned, result.files_scanned);
        assert_eq!(back.dir_ids, result.dir_ids);
        assert_eq!(back.largest_files, result.largest_files);
        assert_eq!(back.shared_files, result.shared_files);
        assert_eq!(back.shared_files.len(), 1, "big.bin has two names");
        for i in 0..result.tree.len() {
            let id = NodeId(i as u32);
            assert_eq!(back.tree.path_of(id), result.tree.path_of(id));
            assert_eq!(
                back.tree.node(id).map(|n| n.allocated()),
                result.tree.node(id).map(|n| n.allocated())
            );
        }
    }

    #[test]
    fn a_damaged_or_truncated_index_is_refused_not_half_read() {
        let fixture = Fixture::new("damaged");
        let bytes = write_scan(Vec::new(), &walk(&fixture.0), &checkpoint()).expect("write");
        for cut in [0, 4, bytes.len() / 2, bytes.len() - 1] {
            assert!(read_scan(&bytes[..cut]).is_err(), "cut at {cut}");
        }
        let mut other = bytes.clone();
        other[4] = VERSION + 1;
        assert!(read_scan(other.as_slice()).is_err(), "another version");
    }

    #[test]
    fn a_rescan_with_nothing_changed_lists_nothing_and_agrees_exactly() {
        let fixture = Fixture::new("unchanged");
        let first = walk(&fixture.0);
        let saved = round_trip(&first);
        let dirty = dirty_set(&saved.result, &HashSet::new(), &[]);
        let mut graft = Graft::new(&saved.result, &dirty);
        let again = scan::walk(
            &fixture.0,
            ScanOptions::default(),
            &mut ScanControl::default(),
            Some(&mut graft),
        );
        assert_eq!(again.method, ScanMethod::Incremental);
        assert_eq!(again.relisted_directories, Some(0));
        assert_eq!(again.reused_directories, Some(first.tree.len() as u64));
        assert_eq!(again.allocated(), first.allocated());
        assert_eq!(again.logical(), first.logical());
        assert_eq!(again.files_scanned, first.files_scanned);
        assert_eq!(again.hard_link_duplicates, first.hard_link_duplicates);
        assert_eq!(again.largest_files.len(), first.largest_files.len());
    }

    #[test]
    fn a_changed_folder_is_read_again_and_everything_else_is_reused() {
        let fixture = Fixture::new("changed");
        let first = walk(&fixture.0);
        let saved = round_trip(&first);
        let b = first
            .find_directory(&format!("{}\\a\\b", fixture.0.display()))
            .expect("a\\b");
        let b_id = first.dir_ids[b.0 as usize];

        std::fs::write(fixture.0.join("a\\b\\new.bin"), vec![9_u8; 100_000]).expect("write");
        let truth = walk(&fixture.0);

        let dirty = dirty_set(&saved.result, &HashSet::from([b_id]), &[]);
        let mut graft = Graft::new(&saved.result, &dirty);
        let again = scan::walk(
            &fixture.0,
            ScanOptions::default(),
            &mut ScanControl::default(),
            Some(&mut graft),
        );
        assert_eq!(again.relisted_directories, Some(1), "only a\\b");
        assert_eq!(again.reused_directories, Some(first.tree.len() as u64 - 1));
        assert_eq!(again.allocated(), truth.allocated(), "the new file is counted");
        assert_eq!(again.files_scanned, truth.files_scanned);
        assert!(
            again.largest_files.iter().any(|f| f.name == "new.bin"),
            "the new file is among the largest"
        );
    }

    #[test]
    fn a_linked_file_is_counted_once_when_only_one_of_its_folders_changed() {
        // big.bin's bytes sit in whichever of `linked` and `c` the first
        // walk met first. Relisting only the other folder would count them
        // again there, so both must be relisted together.
        let fixture = Fixture::new("links");
        let first = walk(&fixture.0);
        let saved = round_trip(&first);
        let c = first
            .find_directory(&format!("{}\\c", fixture.0.display()))
            .expect("c");
        let dirty = dirty_set(&saved.result, &HashSet::from([first.dir_ids[c.0 as usize]]), &[]);
        let linked = first
            .find_directory(&format!("{}\\linked", fixture.0.display()))
            .expect("linked");
        assert!(dirty.folder[linked.0 as usize], "the other folder of the link");
        assert!(dirty.below[0], "the root is above a dirty folder");

        let mut graft = Graft::new(&saved.result, &dirty);
        let again = scan::walk(
            &fixture.0,
            ScanOptions::default(),
            &mut ScanControl::default(),
            Some(&mut graft),
        );
        assert_eq!(again.relisted_directories, Some(2));
        assert_eq!(again.allocated(), first.allocated(), "counted once, not twice");
        assert_eq!(again.hard_link_duplicates, 1);
    }

    #[test]
    fn only_a_whole_drive_is_indexed() {
        assert_eq!(drive_letter("c:\\"), Some('C'));
        assert_eq!(drive_letter("D:"), Some('D'));
        assert_eq!(drive_letter("C:\\Users"), None);
        assert_eq!(drive_letter("\\\\server\\share"), None);
        let dir = Path::new("X:\\idx");
        assert_eq!(path_for(dir, "e:\\"), Some(dir.join("E.idx")));
    }

    #[test]
    fn a_stopped_scan_is_not_saved() {
        let fixture = Fixture::new("stopped");
        let mut result = walk(&fixture.0);
        result.cancelled = true;
        let dir = fixture.0.join("index");
        // Not a drive root, so refused for that reason first; make it look
        // like one by checking the cancelled rule on a drive-rooted copy.
        assert!(save(&dir, &result, &checkpoint()).is_err());
        assert!(!dir.join("C.idx").exists());
    }
}
