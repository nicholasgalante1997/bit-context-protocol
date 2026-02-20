use crate::error::WireError;

// Quick note on the magic bytes: 0x42 is B, 0x43 is C, 0x50 is P, 0x00 is null.
// You can verify this in any ASCII table.
// We store it as raw bytes rather than a u32
// so we don't have to think about endianness — it's always these 4 bytes in this order.

/// Magic number: ASCII "BCP\0".
/// Written as raw bytes, not as a u32, so byte order doesn't matter.
/// each u8 (unsigned 8bit integer) can be represented as a byte
/// A single hex digit represents exactly 4 bits (a "nibble").
/// So a byte (8 bits) is always exactly 2 hex digits
pub const BCP_MAGIC: [u8; 4] = [0x42, 0x43, 0x50, 0x00];

/// Total header size in bytes (fixed).
pub const HEADER_SIZE: usize = 8;

/// Current format version major.
pub const VERSION_MAJOR: u8 = 1;

/// Current format version minor.
pub const VERSION_MINOR: u8 = 0;

// Now HeaderFlags. This is a newtype pattern — a single-field struct wrapping a primitive.
// You'll see this a lot in Rust where you want type safety around a raw value:

/// Header flags bitfield.
///
/// Bit layout:
///   bit 0 = compressed (whole-payload zstd compression)
///   bit 1 = has_index  (index trailer appended after END block)
///   bits 2-7 = reserved (MUST be 0)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HeaderFlags(u8);

impl HeaderFlags {
    /// Whole-payload compression is enabled.
    pub const COMPRESSED: Self = Self(0b0000_0001);

    /// An index trailer is appended after the END block.
    pub const HAS_INDEX: Self = Self(0b0000_0010);

    /// No flags set.
    pub const NONE: Self = Self(0);

    /// Create flags from a raw byte.
    pub fn from_raw(raw: u8) -> Self {
        Self(raw)
    }

    /// Get the underlying byte value.
    pub fn raw(self) -> u8 {
        self.0
    }

    pub fn is_compressed(self) -> bool {
        self.0 & Self::COMPRESSED.0 != 0
    }

    pub fn has_index(self) -> bool {
        self.0 & Self::HAS_INDEX.0 != 0
    }
}

// The key thing here: HeaderFlags(u8) is a tuple struct.
// The self.0 accesses the inner u8.
// The constants like COMPRESSED are const values of Self,
// so HeaderFlags::COMPRESSED is HeaderFlags(0b0000_0001).
// The methods use the same & masking pattern from varint:
//      self.0 & Self::COMPRESSED.0 != 0 checks if bit 0 is set.
// This is the & 0x80 pattern you just learned, but checking bit 0 instead of bit 7.
// Also notice #[derive(Clone, Copy)] —
// this tells Rust that HeaderFlags can be copied implicitly, like a primitive.
// Without Copy, passing a HeaderFlags to a function would move it (transferring ownership).
// Since it's just a u8 wrapper, copying is trivially cheap and we want value semantics.

/// BCP file header — the first 8 bytes of every payload.
///
/// ```text
/// ┌────────┬─────────┬──────────────────────────────────┐
/// │ Offset │ Size    │ Description                      │
/// ├────────┼─────────┼──────────────────────────────────┤
/// │ 0x00   │ 4 bytes │ Magic: "BCP\0" (0x42435000)      │
/// │ 0x04   │ 1 byte  │ Version major                    │
/// │ 0x05   │ 1 byte  │ Version minor                    │
/// │ 0x06   │ 1 byte  │ Flags                            │
/// │ 0x07   │ 1 byte  │ Reserved (0x00)                  │
/// └────────┴─────────┴──────────────────────────────────┘
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BcpHeader {
    pub version_major: u8,
    pub version_minor: u8,
    pub flags: HeaderFlags,
}

impl BcpHeader {
    /// Create a new header with the current version and the given flags.
    pub fn new(flags: HeaderFlags) -> Self {
        Self {
            version_major: VERSION_MAJOR,
            version_minor: VERSION_MINOR,
            flags,
        }
    }

    /// Write the 8-byte header into the provided buffer.
    ///
    /// # Errors
    ///
    /// Returns [`WireError::UnexpectedEof`] if `buf` is shorter than
    /// [`HEADER_SIZE`] (8 bytes).
    pub fn write_to(&self, buf: &mut [u8]) -> Result<(), WireError> {
        if buf.len() < HEADER_SIZE {
            return Err(WireError::UnexpectedEof { offset: buf.len() });
        }

        buf[0..4].copy_from_slice(&BCP_MAGIC);
        buf[4] = self.version_major;
        buf[5] = self.version_minor;
        buf[6] = self.flags.raw();
        buf[7] = 0x00; // reserved

        Ok(())
    }

    /// Parse a header from the first 8 bytes of the provided buffer.
    ///
    /// # Errors
    ///
    /// - [`WireError::UnexpectedEof`] if buffer is too short.
    /// - [`WireError::InvalidMagic`] if the magic number doesn't match.
    /// - [`WireError::UnsupportedVersion`] if the major version is unknown.
    /// - [`WireError::ReservedNonZero`] if the reserved byte is not 0x00.
    pub fn read_from(buf: &[u8]) -> Result<Self, WireError> {
        if buf.len() < HEADER_SIZE {
            return Err(WireError::UnexpectedEof { offset: buf.len() });
        }

        // Validate magic
        if buf[0..4] != BCP_MAGIC {
            let found = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
            return Err(WireError::InvalidMagic { found });
        }

        let version_major = buf[4];
        let version_minor = buf[5];

        // We only support version 1.x
        if version_major != VERSION_MAJOR {
            return Err(WireError::UnsupportedVersion {
                major: version_major,
                minor: version_minor,
            });
        }

        let flags = HeaderFlags::from_raw(buf[6]);

        // Reserved byte must be zero
        if buf[7] != 0x00 {
            return Err(WireError::ReservedNonZero {
                offset: 7,
                value: buf[7],
            });
        }

        Ok(Self {
            version_major,
            version_minor,
            flags,
        })
    }
}

// A few things worth noting:
// ```rs buf[0..4].copy_from_slice(&BCP_MAGIC)```
// — this copies the 4 magic bytes into the buffer.
// copy_from_slice is a slice method that copies from one slice into another.
// It panics if the lengths don't match,
// but we already checked buf.len() >= HEADER_SIZE
// so the subslice buf[0..4] is guaranteed to be 4 bytes.
// ```rs u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]])```
// — this constructs a u32 from 4 bytes in little-endian order.
// We only use this for the error message so the developer sees a readable hex value.
// We don't use it for the comparison itself — comparing byte slices directly (buf[0..4] != BCP_MAGIC) is cleaner
// and sidesteps endianness entirely.
// The validation order matters — we check magic first (is this even a BCP file?),
// then version (is it a version we understand?), then reserved fields.
// This gives the most useful error message for each failure case.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_default_header() {
        let header = BcpHeader::new(HeaderFlags::NONE);
        let mut buf = [0u8; HEADER_SIZE];
        header.write_to(&mut buf).unwrap();
        let parsed = BcpHeader::read_from(&buf).unwrap();
        assert_eq!(header, parsed);
    }

    #[test]
    fn roundtrip_with_flags() {
        let flags =
            HeaderFlags::from_raw(HeaderFlags::COMPRESSED.raw() | HeaderFlags::HAS_INDEX.raw());
        let header = BcpHeader::new(flags);
        let mut buf = [0u8; HEADER_SIZE];
        header.write_to(&mut buf).unwrap();
        let parsed = BcpHeader::read_from(&buf).unwrap();
        assert!(parsed.flags.is_compressed());
        assert!(parsed.flags.has_index());
    }

    #[test]
    fn magic_bytes_are_correct() {
        let header = BcpHeader::new(HeaderFlags::NONE);
        let mut buf = [0u8; HEADER_SIZE];
        header.write_to(&mut buf).unwrap();
        assert_eq!(&buf[0..4], b"BCP\0");
    }

    #[test]
    fn reject_bad_magic() {
        let mut buf = [0u8; HEADER_SIZE];
        buf[0..4].copy_from_slice(b"NOPE");
        buf[4] = VERSION_MAJOR;
        let result = BcpHeader::read_from(&buf);
        assert!(matches!(result, Err(WireError::InvalidMagic { .. })));
    }

    #[test]
    fn reject_unsupported_version() {
        let mut buf = [0u8; HEADER_SIZE];
        buf[0..4].copy_from_slice(&BCP_MAGIC);
        buf[4] = 2; // unsupported major version
        let result = BcpHeader::read_from(&buf);
        assert!(matches!(
            result,
            Err(WireError::UnsupportedVersion { major: 2, .. })
        ));
    }

    #[test]
    fn reject_nonzero_reserved() {
        let mut buf = [0u8; HEADER_SIZE];
        buf[0..4].copy_from_slice(&BCP_MAGIC);
        buf[4] = VERSION_MAJOR;
        buf[7] = 0xFF; // reserved byte must be 0
        let result = BcpHeader::read_from(&buf);
        assert!(matches!(
            result,
            Err(WireError::ReservedNonZero {
                offset: 7,
                value: 0xFF
            })
        ));
    }

    #[test]
    fn reject_buffer_too_short() {
        let buf = [0u8; 4]; // only 4 bytes, need 8
        let result = BcpHeader::read_from(&buf);
        assert!(matches!(result, Err(WireError::UnexpectedEof { .. })));
    }

    #[test]
    fn flags_default_is_none() {
        let flags = HeaderFlags::default();
        assert!(!flags.is_compressed());
        assert!(!flags.has_index());
        assert_eq!(flags.raw(), 0);
    }
}
