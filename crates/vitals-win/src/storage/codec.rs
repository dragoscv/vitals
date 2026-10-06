//! The byte format shared by the Turbo pipe and the saved index.
//!
//! LEB128 varints rather than fixed-width fields: almost every figure in a
//! scan is small (a folder's own file count, a parent index, a byte count
//! under a gigabyte), and a fixed eight bytes for each made the index for
//! `C:` twice the size for nothing. Hand-rolled because the whole format is
//! four functions, and a serialisation dependency for four functions is a
//! supply-chain cost with no benefit.

use std::io::{self, Read, Write};

/// Longest string either format writes: a path component is at most 255
/// UTF-16 units, which is at most 765 bytes of UTF-8. Anything longer on the
/// read side means the stream is not what it claims to be.
const MAX_STR_BYTES: usize = 4096;

pub struct Writer<W: Write> {
    inner: W,
}

impl<W: Write> Writer<W> {
    pub const fn new(inner: W) -> Self {
        Self { inner }
    }

    pub fn u8(&mut self, value: u8) -> io::Result<()> {
        self.inner.write_all(&[value])
    }

    pub fn bytes(&mut self, value: &[u8]) -> io::Result<()> {
        self.inner.write_all(value)
    }

    pub fn varint(&mut self, mut value: u64) -> io::Result<()> {
        let mut buf = [0_u8; 10];
        let mut len = 0;
        loop {
            // Masked to seven bits first, so the narrowing cannot lose data.
            let byte = (value & 0x7F) as u8;
            value >>= 7;
            if value == 0 {
                buf[len] = byte;
                len += 1;
                break;
            }
            buf[len] = byte | 0x80;
            len += 1;
        }
        self.inner.write_all(&buf[..len])
    }

    pub fn str(&mut self, value: &str) -> io::Result<()> {
        let bytes = value.as_bytes();
        let bytes = &bytes[..bytes.len().min(MAX_STR_BYTES)];
        self.varint(bytes.len() as u64)?;
        self.inner.write_all(bytes)
    }

    pub fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }

    pub fn into_inner(self) -> W {
        self.inner
    }
}

pub struct Reader<R: Read> {
    inner: R,
}

fn corrupt(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what.to_owned())
}

impl<R: Read> Reader<R> {
    pub const fn new(inner: R) -> Self {
        Self { inner }
    }

    pub fn u8(&mut self) -> io::Result<u8> {
        let mut byte = [0_u8; 1];
        self.inner.read_exact(&mut byte)?;
        Ok(byte[0])
    }

    pub fn bytes<const N: usize>(&mut self) -> io::Result<[u8; N]> {
        let mut out = [0_u8; N];
        self.inner.read_exact(&mut out)?;
        Ok(out)
    }

    pub fn varint(&mut self) -> io::Result<u64> {
        let mut value = 0_u64;
        for shift in (0..64).step_by(7) {
            let byte = self.u8()?;
            value |= u64::from(byte & 0x7F) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(corrupt("a number in the stream is longer than 64 bits"))
    }

    /// A varint that must fit a `u32`, for indices and counts.
    pub fn varint_u32(&mut self) -> io::Result<u32> {
        u32::try_from(self.varint()?).map_err(|_| corrupt("an index in the stream is too large"))
    }

    pub fn string(&mut self) -> io::Result<String> {
        let len = usize::try_from(self.varint()?).unwrap_or(usize::MAX);
        if len > MAX_STR_BYTES {
            return Err(corrupt("a name in the stream is longer than any path component"));
        }
        let mut bytes = vec![0_u8; len];
        self.inner.read_exact(&mut bytes)?;
        String::from_utf8(bytes).map_err(|_| corrupt("a name in the stream is not UTF-8"))
    }

    pub fn into_inner(self) -> R {
        self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_width_of_number_survives_a_round_trip() {
        let values = [0, 1, 127, 128, 16_383, 16_384, u64::from(u32::MAX), u64::MAX];
        let mut writer = Writer::new(Vec::new());
        for value in values {
            writer.varint(value).expect("write");
        }
        writer.str("Program Files (x86)").expect("write");
        writer.str("").expect("write");
        let bytes = writer.into_inner();
        assert_eq!(bytes[0], 0, "zero is one byte");

        let mut reader = Reader::new(bytes.as_slice());
        for value in values {
            assert_eq!(reader.varint().expect("read"), value);
        }
        assert_eq!(reader.string().expect("read"), "Program Files (x86)");
        assert_eq!(reader.string().expect("read"), "");
        assert!(reader.u8().is_err(), "nothing is left over");
    }

    #[test]
    fn a_number_with_no_end_is_refused_rather_than_read_forever() {
        let endless = [0xFF_u8; 16];
        assert!(Reader::new(endless.as_slice()).varint().is_err());
    }

    #[test]
    fn a_claimed_name_longer_than_any_path_component_is_refused() {
        let mut writer = Writer::new(Vec::new());
        writer.varint(1 << 30).expect("write");
        let bytes = writer.into_inner();
        assert!(Reader::new(bytes.as_slice()).string().is_err());
    }

    #[test]
    fn a_truncated_stream_is_an_error_not_a_short_read() {
        let mut writer = Writer::new(Vec::new());
        writer.str("abcdef").expect("write");
        let mut bytes = writer.into_inner();
        bytes.truncate(4);
        assert!(Reader::new(bytes.as_slice()).string().is_err());
    }
}
