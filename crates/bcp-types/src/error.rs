use bcp_wire::WireError;

/// Errors that can occur when encoding or decoding typed block bodies.
///
/// These are higher-level than [`WireError`] — they deal with semantic
/// validation of block fields rather than raw byte framing. A `TypeError`
/// can wrap an underlying `WireError` when the problem originates in
/// varint or length-prefix parsing within a block body.
///
/// # Error hierarchy
///
/// ```text
/// ┌─────────────────────────────────────────────────────┐
/// │ TypeError (this crate)                              │
/// │   ├── wraps WireError for low-level parse failures  │
/// │   ├── UnknownFieldWireType for bad TLV wire types   │
/// │   ├── MissingRequiredField for incomplete blocks     │
/// │   └── InvalidEnumValue for out-of-range enum bytes  │
/// └─────────────────────────────────────────────────────┘
/// ```
#[derive(Debug, thiserror::Error)]
pub enum TypeError {
  /// A required field was not present in the block body.
  ///
  /// Each block type defines which fields are mandatory. If the decoder
  /// reaches the end of the body without encountering a required field ID,
  /// this error is returned with the field name for diagnostics.
  #[error("missing required field: {field}")]
  MissingRequiredField { field: &'static str },

  /// A field's wire type byte did not match any known [`FieldWireType`].
  ///
  /// The TLV encoding uses wire types 0 (varint), 1 (bytes), and 2 (nested).
  /// Any other value indicates data corruption or a version mismatch.
  #[error("unknown field wire type: {value}")]
  UnknownFieldWireType { value: u64 },

  /// An enum field contained a byte value outside its defined range.
  ///
  /// For example, a `Role` field with value `0x09` when the max defined
  /// variant is `0x04`. The enum name and raw value are captured for
  /// diagnostics.
  #[error("invalid {enum_name} value: {value:#04X}")]
  InvalidEnumValue { enum_name: &'static str, value: u8 },

  /// An underlying wire-level error occurred while parsing within a body.
  ///
  /// This typically surfaces when a varint inside the block body is
  /// malformed or the body bytes are truncated mid-field.
  #[error(transparent)]
  Wire(#[from] WireError),
}
