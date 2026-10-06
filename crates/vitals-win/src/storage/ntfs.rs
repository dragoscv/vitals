//! Reading the NTFS Master File Table straight off the volume.
//!
//! # Why not `FSCTL_ENUM_USN_DATA` or `FSCTL_QUERY_FILE_LAYOUT`
//!
//! Both were measured on this machine (2026-09-29, elevated, `C:` with 13.3
//! million in-use file records): USN enumeration takes 37.8 s and returns no
//! sizes at all; the file-layout query returns sizes but took 364 s, slower
//! than the directory walk it was meant to replace. Reading the table's own
//! records and decoding them here took 19.5 s for the whole volume *with*
//! sizes, which is the only one of the three that is both fast and complete.
//!
//! # What is decoded, and what is deliberately not
//!
//! Only what the walker also reports, so the two agree folder by folder:
//! every `$FILE_NAME` that is not a DOS 8.3 alias (the parent and the name),
//! the unnamed `$DATA` stream's sizes, and the directory flag and reparse
//! tag. Named streams are not counted, and neither are a directory's own
//! index blocks: the walker cannot see them either, and counting them here
//! alone would make "the same totals" impossible to check.
//!
//! Everything here is a pure function over a byte slice, so every offset is
//! tested against synthetic records without an elevated handle. A wrong
//! offset in this module does not crash; it silently reads sizes from the
//! wrong bytes, which is why the layout is pinned by tests.

/// `FILE` in little-endian: the signature of an in-use or free file record.
const FILE_MAGIC: u32 = 0x454C_4946;
/// Record header flags.
const RECORD_IN_USE: u16 = 0x0001;
const RECORD_DIRECTORY: u16 = 0x0002;

/// Attribute type codes.
pub const ATTR_ATTRIBUTE_LIST: u32 = 0x20;
pub const ATTR_FILE_NAME: u32 = 0x30;
pub const ATTR_DATA: u32 = 0x80;
pub const ATTR_REPARSE_POINT: u32 = 0xC0;
const ATTR_END: u32 = 0xFFFF_FFFF;

/// Attribute header flags that mean the on-disk figure is the "total
/// allocated" field rather than the allocation of the whole stream.
const ATTR_COMPRESSED: u16 = 0x0001;
const ATTR_SPARSE: u16 = 0x8000;

/// `$FILE_NAME` namespace for a DOS 8.3 alias. It names a file that already
/// has a long name, so counting it would count the file twice.
const NAMESPACE_DOS: u8 = 2;

/// The low 48 bits of a file reference: the record number.
pub const RECORD_MASK: u64 = 0x0000_FFFF_FFFF_FFFF;

/// The fields of `NTFS_VOLUME_DATA_BUFFER` this reader needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VolumeData {
    pub bytes_per_cluster: u32,
    pub bytes_per_record: u32,
    pub mft_valid_length: u64,
    pub mft_start_lcn: u64,
}

impl VolumeData {
    /// Decodes the buffer `FSCTL_GET_NTFS_VOLUME_DATA` fills.
    #[must_use]
    pub fn parse(buffer: &[u8]) -> Option<Self> {
        let data = Self {
            bytes_per_cluster: u32_at(buffer, 44)?,
            bytes_per_record: u32_at(buffer, 48)?,
            mft_valid_length: u64_at(buffer, 56)?,
            mft_start_lcn: u64_at(buffer, 64)?,
        };
        // A record smaller than its header or a cluster that is not a power
        // of two means the buffer is not what the driver promised.
        let sane = data.bytes_per_cluster.is_power_of_two()
            && (256..=65_536).contains(&data.bytes_per_record)
            && data.bytes_per_record.is_power_of_two();
        sane.then_some(data)
    }
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn u64_at(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(at..at + 8)?.try_into().ok()?))
}

/// Applies the update-sequence fixup to one record in place.
///
/// NTFS writes the last two bytes of every 512-byte stride of a record into
/// an array in the header and replaces them with a check value, so a record
/// torn by a crash mid-write is detectable. Until the bytes are put back,
/// any field that happens to straddle a stride end reads as the check value.
///
/// Returns `false` for a torn or malformed record, which must be skipped: its
/// contents are a mix of two writes.
pub fn apply_fixup(record: &mut [u8]) -> bool {
    let (Some(offset), Some(count)) = (u16_at(record, 4), u16_at(record, 6)) else {
        return false;
    };
    let (offset, count) = (offset as usize, count as usize);
    if count < 2 || offset + count * 2 > record.len() {
        return false;
    }
    let stride = record.len() / (count - 1);
    if stride < 2 || stride * (count - 1) != record.len() {
        return false;
    }
    let check = [record[offset], record[offset + 1]];
    for i in 1..count {
        let end = i * stride - 2;
        if record[end..end + 2] != check {
            return false;
        }
        record[end] = record[offset + i * 2];
        record[end + 1] = record[offset + i * 2 + 1];
    }
    true
}

/// One attribute of a record, as a view into the record's bytes.
#[derive(Debug, Clone, Copy)]
pub struct Attribute<'a> {
    pub kind: u32,
    bytes: &'a [u8],
}

impl<'a> Attribute<'a> {
    #[must_use]
    pub fn is_resident(&self) -> bool {
        self.bytes.get(8) == Some(&0)
    }

    fn flags(&self) -> u16 {
        u16_at(self.bytes, 12).unwrap_or(0)
    }

    /// The attribute's own name — a stream name for `$DATA`.
    #[must_use]
    pub fn name(&self) -> String {
        let len = self.bytes.get(9).copied().unwrap_or(0) as usize;
        let offset = u16_at(self.bytes, 10).unwrap_or(0) as usize;
        utf16_at(self.bytes, offset, len).unwrap_or_default()
    }

    fn has_name(&self) -> bool {
        self.bytes.get(9).is_some_and(|len| *len != 0)
    }

    /// A resident attribute's value.
    #[must_use]
    pub fn value(&self) -> Option<&'a [u8]> {
        if !self.is_resident() {
            return None;
        }
        let len = u32_at(self.bytes, 16)? as usize;
        let offset = u16_at(self.bytes, 20)? as usize;
        self.bytes.get(offset..offset.checked_add(len)?)
    }

    /// The first virtual cluster a non-resident segment covers. Only the
    /// segment starting at zero carries the stream's sizes.
    #[must_use]
    pub fn lowest_vcn(&self) -> Option<u64> {
        if self.is_resident() {
            return None;
        }
        u64_at(self.bytes, 16)
    }

    /// A non-resident attribute's cluster runs.
    #[must_use]
    pub fn runs(&self) -> Option<Vec<Run>> {
        if self.is_resident() {
            return None;
        }
        let offset = u16_at(self.bytes, 32)? as usize;
        decode_runs(self.bytes.get(offset..)?)
    }

    /// The stream's sizes, `(allocated, logical)`, or `None` for a
    /// non-resident segment that does not start the stream.
    #[must_use]
    pub fn sizes(&self, cluster: u64) -> Option<(u64, u64)> {
        if self.is_resident() {
            let logical = u64::from(u32_at(self.bytes, 16)?);
            // What the directory walk reports for the same file: NTFS gives
            // a resident stream an allocation of its length rounded to eight
            // bytes, and the walker then places it on the cluster grid. Both
            // scanners must agree, so both do the same.
            let allocated =
                super::sizing::reconcile_reported(logical.next_multiple_of(8), cluster);
            return Some((allocated, logical));
        }
        if self.lowest_vcn()? != 0 {
            return None;
        }
        let logical = u64_at(self.bytes, 48)?;
        let allocated = if self.flags() & (ATTR_COMPRESSED | ATTR_SPARSE) != 0 {
            // Compressed and sparse streams occupy only the clusters that
            // hold data, which NTFS records separately from the stream's
            // nominal allocation.
            u64_at(self.bytes, 64)?
        } else {
            u64_at(self.bytes, 40)?
        };
        Some((allocated, logical))
    }
}

fn utf16_at(bytes: &[u8], offset: usize, units: usize) -> Option<String> {
    let raw = bytes.get(offset..offset.checked_add(units * 2)?)?;
    let wide: Vec<u16> = raw
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes(*pair))
        .collect();
    Some(String::from_utf16_lossy(&wide))
}

/// Iterates the attributes of a fixed-up record.
///
/// Stops at the end marker, at a zero or out-of-bounds length, or at the end
/// of the bytes the header says are in use — whichever comes first, so a
/// damaged record yields a prefix rather than a loop or an overrun.
pub fn attributes(record: &[u8]) -> impl Iterator<Item = Attribute<'_>> {
    let first = u16_at(record, 20).unwrap_or(0) as usize;
    let used = (u32_at(record, 24).unwrap_or(0) as usize).min(record.len());
    let mut offset = first;
    std::iter::from_fn(move || {
        let kind = u32_at(record, offset)?;
        if kind == ATTR_END || offset + 16 > used {
            return None;
        }
        let len = u32_at(record, offset + 4)? as usize;
        if len < 16 || offset + len > used {
            return None;
        }
        let bytes = &record[offset..offset + len];
        offset += len;
        Some(Attribute { kind, bytes })
    })
}

/// A contiguous stretch of clusters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Run {
    /// First logical cluster, or `None` for a sparse run that has none.
    pub lcn: Option<u64>,
    pub clusters: u64,
}

/// Decodes an NTFS run list.
///
/// Each run is a header byte whose low nibble is the width of the length and
/// high nibble the width of the offset, then the two little-endian fields.
/// The offset is **signed and relative to the previous run's start**, which
/// is the step that silently produces nonsense when it is read as absolute.
/// `None` for a malformed list.
#[must_use]
pub fn decode_runs(bytes: &[u8]) -> Option<Vec<Run>> {
    let mut runs = Vec::new();
    let mut at = 0_usize;
    let mut lcn: i64 = 0;
    loop {
        let header = *bytes.get(at)?;
        if header == 0 {
            return Some(runs);
        }
        let len_bytes = (header & 0x0F) as usize;
        let off_bytes = (header >> 4) as usize;
        if len_bytes == 0 || len_bytes > 8 || off_bytes > 8 {
            return None;
        }
        at += 1;
        let mut clusters = 0_u64;
        for i in 0..len_bytes {
            clusters |= u64::from(*bytes.get(at + i)?) << (8 * i);
        }
        at += len_bytes;
        if off_bytes == 0 {
            runs.push(Run {
                lcn: None,
                clusters,
            });
            continue;
        }
        let mut delta = 0_i64;
        for i in 0..off_bytes {
            delta |= i64::from(*bytes.get(at + i)?) << (8 * i);
        }
        // Sign-extend from the field's own width.
        let shift = 64 - 8 * off_bytes as u32;
        delta = (delta << shift) >> shift;
        at += off_bytes;
        lcn = lcn.checked_add(delta)?;
        runs.push(Run {
            lcn: Some(u64::try_from(lcn).ok()?),
            clusters,
        });
    }
}

/// One `$FILE_NAME` that names the file (not a DOS alias).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Name {
    /// The parent directory's file reference, sequence number included.
    pub parent: u64,
    pub name: String,
}

/// What one record contributes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Record {
    /// The record number, from its position in the table.
    pub number: u64,
    pub sequence: u16,
    pub directory: bool,
    /// The base record's number when this is an extension record, whose
    /// attributes belong to that file.
    pub base: Option<u64>,
    pub names: Vec<Name>,
    /// Unnamed `$DATA` sizes, when the segment that starts the stream is in
    /// this record.
    pub data: Option<(u64, u64)>,
    /// Attributes live in other records too; the file is only complete
    /// once they have been read.
    pub has_attribute_list: bool,
    /// The reparse tag, when the file is a reparse point.
    pub reparse_tag: Option<u32>,
}

impl Record {
    /// The file reference the change journal and a directory listing use.
    #[must_use]
    pub fn reference(&self) -> u64 {
        u64::from(self.sequence) << 48 | (self.number & RECORD_MASK)
    }
}

/// Decodes one raw record. `None` for a free, torn or foreign record.
///
/// `raw` is fixed up in place.
pub fn parse_record(raw: &mut [u8], number: u64, cluster: u64) -> Option<Record> {
    if u32_at(raw, 0)? != FILE_MAGIC {
        return None;
    }
    let flags = u16_at(raw, 22)?;
    if flags & RECORD_IN_USE == 0 || !apply_fixup(raw) {
        return None;
    }
    let base = u64_at(raw, 32)? & RECORD_MASK;
    let mut record = Record {
        number,
        sequence: u16_at(raw, 16)?,
        directory: flags & RECORD_DIRECTORY != 0,
        base: (base != 0).then_some(base),
        ..Record::default()
    };
    for attribute in attributes(raw) {
        match attribute.kind {
            ATTR_FILE_NAME => {
                let Some(value) = attribute.value() else {
                    continue;
                };
                let (Some(parent), Some(&len), Some(&namespace)) =
                    (u64_at(value, 0), value.get(64), value.get(65))
                else {
                    continue;
                };
                if namespace == NAMESPACE_DOS {
                    continue;
                }
                if let Some(name) = utf16_at(value, 66, len as usize) {
                    record.names.push(Name { parent, name });
                }
            }
            ATTR_DATA if !attribute.has_name() => {
                if let Some(sizes) = attribute.sizes(cluster) {
                    record.data = Some(sizes);
                }
            }
            ATTR_ATTRIBUTE_LIST => record.has_attribute_list = true,
            ATTR_REPARSE_POINT => {
                // The tag is the first four bytes of the value. A reparse
                // point too large to be resident still has one; zero stands
                // for "a tag this reader could not see", which no real tag is.
                record.reparse_tag = Some(attribute.value().and_then(|v| u32_at(v, 0)).unwrap_or(0));
            }
            _ => {}
        }
    }
    Some(record)
}

/// Where the rest of a stream lives, from an attribute list.
///
/// Returns the record numbers holding `$DATA` segments that do not start at
/// virtual cluster zero, which is what `$MFT` needs when it is fragmented
/// beyond what its base record can describe.
#[must_use]
pub fn data_extensions(list: &[u8], own: u64) -> Vec<(u64, u64)> {
    let mut out = Vec::new();
    let mut at = 0_usize;
    while let (Some(kind), Some(len)) = (u32_at(list, at), u16_at(list, at + 4)) {
        let len = len as usize;
        if len < 26 {
            break;
        }
        let name_len = list.get(at + 6).copied().unwrap_or(0);
        if kind == ATTR_DATA && name_len == 0 {
            let vcn = u64_at(list, at + 8).unwrap_or(0);
            let segment = u64_at(list, at + 16).unwrap_or(0) & RECORD_MASK;
            if segment != own {
                out.push((vcn, segment));
            }
        }
        at += len;
    }
    out.sort_unstable();
    out
}

#[cfg(test)]
pub(crate) mod synth {
    //! Builds records byte by byte, the way NTFS lays them out, so the parser
    //! is tested against the format rather than against itself.

    pub const RECORD: usize = 1024;

    pub struct Builder {
        pub bytes: Vec<u8>,
        at: usize,
    }

    impl Builder {
        pub fn new(sequence: u16, flags: u16, base: u64) -> Self {
            let mut bytes = vec![0_u8; RECORD];
            bytes[0..4].copy_from_slice(b"FILE");
            // Update sequence array at 48: check value + one entry per stride.
            bytes[4..6].copy_from_slice(&48_u16.to_le_bytes());
            bytes[6..8].copy_from_slice(&3_u16.to_le_bytes());
            bytes[16..18].copy_from_slice(&sequence.to_le_bytes());
            bytes[20..22].copy_from_slice(&56_u16.to_le_bytes());
            bytes[22..24].copy_from_slice(&flags.to_le_bytes());
            bytes[28..32].copy_from_slice(&(RECORD as u32).to_le_bytes());
            bytes[32..40].copy_from_slice(&base.to_le_bytes());
            Self { bytes, at: 56 }
        }

        fn push(&mut self, attr: &[u8]) {
            let len = attr.len().next_multiple_of(8);
            self.bytes[self.at..self.at + attr.len()].copy_from_slice(attr);
            self.bytes[self.at + 4..self.at + 8].copy_from_slice(&(len as u32).to_le_bytes());
            self.at += len;
        }

        fn header(kind: u32, resident: bool, name: &str) -> (Vec<u8>, usize) {
            let name16: Vec<u8> = name.encode_utf16().flat_map(u16::to_le_bytes).collect();
            let fixed = if resident { 24 } else { 72 };
            let mut a = vec![0_u8; fixed + name16.len()];
            a[0..4].copy_from_slice(&kind.to_le_bytes());
            a[8] = u8::from(!resident);
            a[9] = name.encode_utf16().count() as u8;
            a[10..12].copy_from_slice(&(fixed as u16).to_le_bytes());
            a[fixed..].copy_from_slice(&name16);
            let end = a.len().next_multiple_of(8);
            a.resize(end, 0);
            (a, end)
        }

        pub fn file_name(mut self, parent: u64, name: &str, namespace: u8) -> Self {
            let (mut a, start) = Self::header(super::ATTR_FILE_NAME, true, "");
            let name16: Vec<u8> = name.encode_utf16().flat_map(u16::to_le_bytes).collect();
            let mut value = vec![0_u8; 66 + name16.len()];
            value[0..8].copy_from_slice(&parent.to_le_bytes());
            value[64] = name.encode_utf16().count() as u8;
            value[65] = namespace;
            value[66..].copy_from_slice(&name16);
            a[16..20].copy_from_slice(&(value.len() as u32).to_le_bytes());
            a[20..22].copy_from_slice(&(start as u16).to_le_bytes());
            a.extend(value);
            self.push(&a);
            self
        }

        pub fn resident_data(mut self, name: &str, len: u32) -> Self {
            let (mut a, start) = Self::header(super::ATTR_DATA, true, name);
            a[16..20].copy_from_slice(&len.to_le_bytes());
            a[20..22].copy_from_slice(&(start as u16).to_le_bytes());
            a.extend(vec![0xAB_u8; len as usize]);
            self.push(&a);
            self
        }

        pub fn nonresident_data(
            mut self,
            name: &str,
            vcn: u64,
            flags: u16,
            allocated: u64,
            logical: u64,
            total: u64,
            runs: &[u8],
        ) -> Self {
            let (mut a, start) = Self::header(super::ATTR_DATA, false, name);
            a[12..14].copy_from_slice(&flags.to_le_bytes());
            a[16..24].copy_from_slice(&vcn.to_le_bytes());
            a[32..34].copy_from_slice(&(start as u16).to_le_bytes());
            a[40..48].copy_from_slice(&allocated.to_le_bytes());
            a[48..56].copy_from_slice(&logical.to_le_bytes());
            a[64..72].copy_from_slice(&total.to_le_bytes());
            a.extend_from_slice(runs);
            a.push(0);
            self.push(&a);
            self
        }

        pub fn plain(mut self, kind: u32, value: &[u8]) -> Self {
            let (mut a, start) = Self::header(kind, true, "");
            a[16..20].copy_from_slice(&(value.len() as u32).to_le_bytes());
            a[20..22].copy_from_slice(&(start as u16).to_le_bytes());
            a.extend_from_slice(value);
            self.push(&a);
            self
        }

        /// Ends the record and applies the update sequence the way NTFS
        /// does on write, so the parser has to undo it.
        pub fn finish(mut self) -> Vec<u8> {
            self.bytes[self.at..self.at + 4].copy_from_slice(&0xFFFF_FFFF_u32.to_le_bytes());
            let used = (self.at + 8) as u32;
            self.bytes[24..28].copy_from_slice(&used.to_le_bytes());
            let check = [0x5A_u8, 0xA5];
            self.bytes[48..50].copy_from_slice(&check);
            for i in 1..3 {
                let end = i * 512 - 2;
                let saved = [self.bytes[end], self.bytes[end + 1]];
                self.bytes[48 + i * 2..50 + i * 2].copy_from_slice(&saved);
                self.bytes[end..end + 2].copy_from_slice(&check);
            }
            self.bytes
        }
    }
}

#[cfg(test)]
mod tests {
    use super::synth::Builder;
    use super::*;

    #[test]
    fn the_volume_data_offsets_are_the_documented_ones() {
        let mut b = vec![0_u8; 96];
        b[44..48].copy_from_slice(&4096_u32.to_le_bytes());
        b[48..52].copy_from_slice(&1024_u32.to_le_bytes());
        b[56..64].copy_from_slice(&13_943_701_504_u64.to_le_bytes());
        b[64..72].copy_from_slice(&786_432_u64.to_le_bytes());
        assert_eq!(
            VolumeData::parse(&b),
            Some(VolumeData {
                bytes_per_cluster: 4096,
                bytes_per_record: 1024,
                mft_valid_length: 13_943_701_504,
                mft_start_lcn: 786_432,
            })
        );
        b[48..52].copy_from_slice(&7_u32.to_le_bytes());
        assert_eq!(VolumeData::parse(&b), None, "a nonsense record size");
    }

    #[test]
    fn run_offsets_are_signed_and_relative_to_the_previous_run() {
        // 0x21: one length byte, two offset bytes. Second run goes BACK by
        // 0x10 clusters, which an unsigned or absolute reading gets wrong.
        let runs = decode_runs(&[
            0x21, 0x08, 0x00, 0x01, // 8 clusters at 0x100
            0x21, 0x04, 0xF0, 0xFF, // 4 clusters at 0x100 - 0x10
            0x01, 0x02, // sparse, 2 clusters
            0x11, 0x01, 0x20, // 1 cluster at 0xF0 + 0x20
            0x00,
        ])
        .expect("valid");
        assert_eq!(
            runs,
            [
                Run { lcn: Some(0x100), clusters: 8 },
                Run { lcn: Some(0xF0), clusters: 4 },
                Run { lcn: None, clusters: 2 },
                Run { lcn: Some(0x110), clusters: 1 },
            ]
        );
    }

    #[test]
    fn a_run_list_that_runs_off_its_bytes_is_refused() {
        assert_eq!(decode_runs(&[0x21, 0x08]), None);
        assert_eq!(decode_runs(&[0x09, 0, 0, 0, 0, 0, 0, 0, 0, 0]), None);
        assert_eq!(decode_runs(&[0x11, 0x01, 0xFF, 0x00]), None, "before cluster 0");
    }

    #[test]
    fn a_file_with_a_long_name_and_a_dos_alias_is_one_name() {
        let mut raw = Builder::new(3, 0x1, 0)
            .file_name(5 | 5 << 48, "PROGRA~1", 2)
            .file_name(5 | 5 << 48, "Program Files", 1)
            .finish();
        let record = parse_record(&mut raw, 40, 4096).expect("in use");
        assert_eq!(
            record.names,
            [Name { parent: 5 | 5 << 48, name: "Program Files".into() }]
        );
        assert_eq!(record.reference(), 3 << 48 | 40);
    }

    #[test]
    fn a_resident_stream_is_sized_as_the_walker_sizes_it() {
        let mut raw = Builder::new(1, 0x1, 0)
            .file_name(5, "tiny.txt", 1)
            .resident_data("", 10)
            .finish();
        let record = parse_record(&mut raw, 100, 4096).expect("in use");
        // Walker: AllocationSize 16 (10 rounded to 8), then the cluster grid.
        assert_eq!(record.data, Some((4096, 10)));

        let mut empty = Builder::new(1, 0x1, 0)
            .file_name(5, "empty.txt", 1)
            .resident_data("", 0)
            .finish();
        assert_eq!(parse_record(&mut empty, 101, 4096).expect("in use").data, Some((0, 0)));
    }

    #[test]
    fn compressed_and_sparse_streams_count_only_their_occupied_clusters() {
        let mut plain = Builder::new(1, 0x1, 0)
            .file_name(5, "a.bin", 1)
            .nonresident_data("", 0, 0, 8192, 5000, 0, &[0x11, 0x02, 0x10])
            .finish();
        assert_eq!(
            parse_record(&mut plain, 1, 4096).expect("in use").data,
            Some((8192, 5000))
        );
        let mut sparse = Builder::new(1, 0x1, 0)
            .file_name(5, "b.vhdx", 1)
            .nonresident_data("", 0, ATTR_SPARSE, 1 << 40, 1 << 40, 65_536, &[0x01, 0x10])
            .finish();
        assert_eq!(
            parse_record(&mut sparse, 2, 4096).expect("in use").data,
            Some((65_536, 1 << 40)),
            "a 1 TB sparse disk holding 64 KB occupies 64 KB"
        );
    }

    #[test]
    fn a_reparse_tag_and_a_named_stream_are_told_apart_from_the_file_data() {
        let mut raw = Builder::new(1, 0x1, 0)
            .file_name(5, "notepad.exe", 1)
            .nonresident_data("", 0, 0, 360_448, 360_000, 0, &[0x11, 0x58, 0x10])
            .nonresident_data("Zone.Identifier", 0, 0, 4096, 26, 0, &[0x11, 0x01, 0x40])
            .plain(ATTR_REPARSE_POINT, &0x8000_0017_u32.to_le_bytes())
            .finish();
        let record = parse_record(&mut raw, 3, 4096).expect("in use");
        assert_eq!(
            record.data,
            Some((360_448, 360_000)),
            "a named stream is not the file's data"
        );
        assert_eq!(record.reparse_tag, Some(0x8000_0017));
    }

    #[test]
    fn only_the_segment_that_starts_a_stream_carries_its_sizes() {
        let mut ext = Builder::new(1, 0x1, 77)
            .nonresident_data("", 900, 0, 0, 0, 0, &[0x11, 0x01, 0x05])
            .finish();
        let record = parse_record(&mut ext, 4, 4096).expect("in use");
        assert_eq!(record.base, Some(77));
        assert_eq!(record.data, None);
    }

    #[test]
    fn a_torn_record_is_skipped_rather_than_half_read() {
        let mut raw = Builder::new(1, 0x1, 0).file_name(5, "x", 1).finish();
        raw[1022] ^= 0xFF;
        assert!(parse_record(&mut raw, 5, 4096).is_none());
    }

    #[test]
    fn a_free_record_and_a_foreign_block_are_nothing() {
        let mut free = Builder::new(1, 0x0, 0).file_name(5, "gone", 1).finish();
        assert!(parse_record(&mut free, 6, 4096).is_none());
        let mut junk = vec![0x41_u8; 1024];
        assert!(parse_record(&mut junk, 7, 4096).is_none());
    }

    #[test]
    fn a_directory_and_its_flags_are_read_from_the_header() {
        let mut raw = Builder::new(9, 0x3, 0)
            .file_name(5, "Users", 1)
            .plain(ATTR_ATTRIBUTE_LIST, &[])
            .finish();
        let record = parse_record(&mut raw, 8, 4096).expect("in use");
        assert!(record.directory);
        assert!(record.has_attribute_list);
        assert_eq!(record.data, None);
    }

    #[test]
    fn a_damaged_attribute_length_ends_the_walk_instead_of_overrunning() {
        let mut raw = Builder::new(1, 0x1, 0)
            .file_name(5, "ok", 1)
            .resident_data("", 3)
            .finish();
        // Corrupt the second attribute's length to point past the record.
        let first = u16_at(&raw, 20).expect("offset") as usize;
        let len = u32_at(&raw, first + 4).expect("len") as usize;
        raw[first + len + 4..first + len + 8].copy_from_slice(&50_000_u32.to_le_bytes());
        let record = parse_record(&mut raw, 9, 4096).expect("header is fine");
        assert_eq!(record.names.len(), 1);
        assert_eq!(record.data, None, "the damaged attribute is not read");
    }

    #[test]
    fn attribute_list_entries_name_the_records_holding_later_segments() {
        let mut list = Vec::new();
        for (kind, vcn, segment) in [(0x10_u32, 0_u64, 0_u64), (0x80, 0, 0), (0x80, 5000, 42)] {
            let mut e = vec![0_u8; 32];
            e[0..4].copy_from_slice(&kind.to_le_bytes());
            e[4..6].copy_from_slice(&32_u16.to_le_bytes());
            e[8..16].copy_from_slice(&vcn.to_le_bytes());
            e[16..24].copy_from_slice(&(segment | 3 << 48).to_le_bytes());
            list.extend(e);
        }
        assert_eq!(data_extensions(&list, 0), [(5000, 42)]);
    }
}
