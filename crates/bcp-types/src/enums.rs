use crate::error::TypeError;

// ── Macro for wire-byte enum boilerplate ──────────────────────────────
//
// Every enum in this module follows the same pattern: a fixed set of
// named variants, each mapped to a single wire byte, plus a conversion
// pair (to_wire_byte / from_wire_byte). This macro eliminates the
// repetition while keeping each enum's doc comments and derive list
// explicit at the call site.

macro_rules! wire_enum {
  (
    $(#[$meta:meta])*
    pub enum $name:ident {
      $( $(#[$vmeta:meta])* $variant:ident = $wire:expr ),+ $(,)?
    }
  ) => {
    $(#[$meta])*
    pub enum $name {
      $( $(#[$vmeta])* $variant ),+
    }

    impl $name {
      /// Encode this variant as a single wire byte.
      pub fn to_wire_byte(self) -> u8 {
        match self {
          $( Self::$variant => $wire ),+
        }
      }

      /// Decode a wire byte into this enum.
      ///
      /// Returns `Err(TypeError::InvalidEnumValue)` if the byte
      /// doesn't match any known variant.
      pub fn from_wire_byte(value: u8) -> Result<Self, TypeError> {
        match value {
          $( $wire => Ok(Self::$variant), )+
          other => Err(TypeError::InvalidEnumValue {
            enum_name: stringify!($name),
            value: other,
          }),
        }
      }
    }
  };
}

// ── Lang ──────────────────────────────────────────────────────────────

/// Programming language identifiers for CODE blocks.
///
/// Each variant maps to a single wire byte. The `Unknown` variant (0xFF)
/// is used when the language is not in the predefined set. For truly
/// unrecognized bytes from a newer encoder, `Other(u8)` preserves the
/// raw value for forward compatibility.
///
/// ```text
/// ┌──────┬────────────┐
/// │ Wire │ Language    │
/// ├──────┼────────────┤
/// │ 0x01 │ Rust       │
/// │ 0x02 │ TypeScript │
/// │ 0x03 │ JavaScript │
/// │ 0x04 │ Python     │
/// │ 0x05 │ Go         │
/// │ 0x06 │ Java       │
/// │ 0x07 │ C          │
/// │ 0x08 │ Cpp        │
/// │ 0x09 │ Ruby       │
/// │ 0x0A │ Shell      │
/// │ 0x0B │ Sql        │
/// │ 0x0C │ Html       │
/// │ 0x0D │ Css        │
/// │ 0x0E │ Json       │
/// │ 0x0F │ Yaml       │
/// │ 0x10 │ Toml       │
/// │ 0x11 │ Markdown   │
/// │ 0xFF │ Unknown    │
/// └──────┴────────────┘
/// ```
///
/// `Lang` is special compared to other enums in this module: it has an
/// `Other(u8)` variant for forward compatibility, so it cannot use the
/// `wire_enum!` macro and is implemented manually.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
  Rust,
  TypeScript,
  JavaScript,
  Python,
  Go,
  Java,
  C,
  Cpp,
  Ruby,
  Shell,
  Sql,
  Html,
  Css,
  Json,
  Yaml,
  Toml,
  Markdown,
  Unknown,
  /// Forward-compatible catch-all. Preserves the raw wire byte for
  /// language IDs this version doesn't recognize.
  Other(u8),
}

impl Lang {
  /// Encode this variant as a single wire byte.
  pub fn to_wire_byte(self) -> u8 {
    match self {
      Self::Rust => 0x01,
      Self::TypeScript => 0x02,
      Self::JavaScript => 0x03,
      Self::Python => 0x04,
      Self::Go => 0x05,
      Self::Java => 0x06,
      Self::C => 0x07,
      Self::Cpp => 0x08,
      Self::Ruby => 0x09,
      Self::Shell => 0x0A,
      Self::Sql => 0x0B,
      Self::Html => 0x0C,
      Self::Css => 0x0D,
      Self::Json => 0x0E,
      Self::Yaml => 0x0F,
      Self::Toml => 0x10,
      Self::Markdown => 0x11,
      Self::Unknown => 0xFF,
      Self::Other(id) => id,
    }
  }

  /// Decode a wire byte into a [`Lang`].
  ///
  /// Known values map to named variants. Unrecognized values become
  /// `Other(id)` rather than an error, since new languages may be
  /// added without a spec revision.
  pub fn from_wire_byte(value: u8) -> Self {
    match value {
      0x01 => Self::Rust,
      0x02 => Self::TypeScript,
      0x03 => Self::JavaScript,
      0x04 => Self::Python,
      0x05 => Self::Go,
      0x06 => Self::Java,
      0x07 => Self::C,
      0x08 => Self::Cpp,
      0x09 => Self::Ruby,
      0x0A => Self::Shell,
      0x0B => Self::Sql,
      0x0C => Self::Html,
      0x0D => Self::Css,
      0x0E => Self::Json,
      0x0F => Self::Yaml,
      0x10 => Self::Toml,
      0x11 => Self::Markdown,
      0xFF => Self::Unknown,
      other => Self::Other(other),
    }
  }
}

// ── Role ──────────────────────────────────────────────────────────────

wire_enum! {
  /// Conversation role for CONVERSATION blocks.
  ///
  /// Maps the four standard chat roles to single-byte wire values.
  /// Unlike [`Lang`], this enum does not have an `Other` variant —
  /// unrecognized role bytes produce an error, since role semantics
  /// are fundamental to conversation structure.
  ///
  /// ```text
  /// ┌──────┬───────────┐
  /// │ Wire │ Role      │
  /// ├──────┼───────────┤
  /// │ 0x01 │ System    │
  /// │ 0x02 │ User      │
  /// │ 0x03 │ Assistant │
  /// │ 0x04 │ Tool      │
  /// └──────┴───────────┘
  /// ```
  #[derive(Clone, Copy, Debug, PartialEq, Eq)]
  pub enum Role {
    System = 0x01,
    User = 0x02,
    Assistant = 0x03,
    Tool = 0x04,
  }
}

// ── Status ────────────────────────────────────────────────────────────

wire_enum! {
  /// Tool execution status for TOOL_RESULT blocks.
  ///
  /// Indicates whether the tool invocation succeeded, failed, or timed out.
  ///
  /// ```text
  /// ┌──────┬─────────┐
  /// │ Wire │ Status  │
  /// ├──────┼─────────┤
  /// │ 0x01 │ Ok      │
  /// │ 0x02 │ Error   │
  /// │ 0x03 │ Timeout │
  /// └──────┴─────────┘
  /// ```
  #[derive(Clone, Copy, Debug, PartialEq, Eq)]
  pub enum Status {
    Ok = 0x01,
    Error = 0x02,
    Timeout = 0x03,
  }
}

// ── Priority ──────────────────────────────────────────────────────────

wire_enum! {
  /// Content priority for ANNOTATION blocks.
  ///
  /// Used by the token budget engine to decide which blocks to include,
  /// summarize, or drop when context space is limited. Ordered from
  /// highest to lowest urgency.
  ///
  /// ```text
  /// ┌──────┬────────────┐
  /// │ Wire │ Priority   │
  /// ├──────┼────────────┤
  /// │ 0x01 │ Critical   │
  /// │ 0x02 │ High       │
  /// │ 0x03 │ Normal     │
  /// │ 0x04 │ Low        │
  /// │ 0x05 │ Background │
  /// └──────┴────────────┘
  /// ```
  #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
  pub enum Priority {
    Critical = 0x01,
    High = 0x02,
    Normal = 0x03,
    Low = 0x04,
    Background = 0x05,
  }
}

// ── FormatHint ────────────────────────────────────────────────────────

wire_enum! {
  /// Document format hint for DOCUMENT blocks.
  ///
  /// Tells the renderer how to interpret the document body. This is a
  /// hint, not a guarantee — the body may contain mixed content.
  ///
  /// ```text
  /// ┌──────┬──────────┐
  /// │ Wire │ Format   │
  /// ├──────┼──────────┤
  /// │ 0x01 │ Markdown │
  /// │ 0x02 │ Plain    │
  /// │ 0x03 │ Html     │
  /// └──────┴──────────┘
  /// ```
  #[derive(Clone, Copy, Debug, PartialEq, Eq)]
  pub enum FormatHint {
    Markdown = 0x01,
    Plain = 0x02,
    Html = 0x03,
  }
}

// ── DataFormat ────────────────────────────────────────────────────────

wire_enum! {
  /// Data format for STRUCTURED_DATA blocks.
  ///
  /// Identifies the serialization format of the block's content field,
  /// so the renderer can syntax-highlight or parse it appropriately.
  ///
  /// ```text
  /// ┌──────┬──────┐
  /// │ Wire │ Fmt  │
  /// ├──────┼──────┤
  /// │ 0x01 │ Json │
  /// │ 0x02 │ Yaml │
  /// │ 0x03 │ Toml │
  /// │ 0x04 │ Csv  │
  /// └──────┴──────┘
  /// ```
  #[derive(Clone, Copy, Debug, PartialEq, Eq)]
  pub enum DataFormat {
    Json = 0x01,
    Yaml = 0x02,
    Toml = 0x03,
    Csv = 0x04,
  }
}

// ── AnnotationKind ────────────────────────────────────────────────────

wire_enum! {
  /// Annotation kind for ANNOTATION blocks.
  ///
  /// Determines how the annotation's `value` field should be interpreted.
  ///
  /// ```text
  /// ┌──────┬──────────┐
  /// │ Wire │ Kind     │
  /// ├──────┼──────────┤
  /// │ 0x01 │ Priority │
  /// │ 0x02 │ Summary  │
  /// │ 0x03 │ Tag      │
  /// └──────┴──────────┘
  /// ```
  #[derive(Clone, Copy, Debug, PartialEq, Eq)]
  pub enum AnnotationKind {
    Priority = 0x01,
    Summary = 0x02,
    Tag = 0x03,
  }
}

// ── MediaType ─────────────────────────────────────────────────────────

wire_enum! {
  /// Image media type for IMAGE blocks.
  ///
  /// Identifies the image encoding so the decoder knows how to handle
  /// the raw bytes in the `data` field.
  ///
  /// ```text
  /// ┌──────┬──────┐
  /// │ Wire │ Type │
  /// ├──────┼──────┤
  /// │ 0x01 │ Png  │
  /// │ 0x02 │ Jpeg │
  /// │ 0x03 │ Gif  │
  /// │ 0x04 │ Svg  │
  /// │ 0x05 │ Webp │
  /// └──────┴──────┘
  /// ```
  #[derive(Clone, Copy, Debug, PartialEq, Eq)]
  pub enum MediaType {
    Png = 0x01,
    Jpeg = 0x02,
    Gif = 0x03,
    Svg = 0x04,
    Webp = 0x05,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  // ── Lang tests ────────────────────────────────────────────────────

  #[test]
  fn lang_all_known_roundtrip() {
    let cases = [
      (Lang::Rust, 0x01),
      (Lang::TypeScript, 0x02),
      (Lang::JavaScript, 0x03),
      (Lang::Python, 0x04),
      (Lang::Go, 0x05),
      (Lang::Java, 0x06),
      (Lang::C, 0x07),
      (Lang::Cpp, 0x08),
      (Lang::Ruby, 0x09),
      (Lang::Shell, 0x0A),
      (Lang::Sql, 0x0B),
      (Lang::Html, 0x0C),
      (Lang::Css, 0x0D),
      (Lang::Json, 0x0E),
      (Lang::Yaml, 0x0F),
      (Lang::Toml, 0x10),
      (Lang::Markdown, 0x11),
      (Lang::Unknown, 0xFF),
    ];
    for (variant, wire) in cases {
      assert_eq!(variant.to_wire_byte(), wire);
      assert_eq!(Lang::from_wire_byte(wire), variant);
    }
  }

  #[test]
  fn lang_other_preserved() {
    let lang = Lang::from_wire_byte(0x42);
    assert_eq!(lang, Lang::Other(0x42));
    assert_eq!(lang.to_wire_byte(), 0x42);
  }

  // ── Role tests ────────────────────────────────────────────────────

  #[test]
  fn role_roundtrip() {
    let cases = [
      (Role::System, 0x01),
      (Role::User, 0x02),
      (Role::Assistant, 0x03),
      (Role::Tool, 0x04),
    ];
    for (variant, wire) in cases {
      assert_eq!(variant.to_wire_byte(), wire);
      assert_eq!(Role::from_wire_byte(wire).unwrap(), variant);
    }
  }

  #[test]
  fn role_invalid_rejected() {
    let result = Role::from_wire_byte(0x09);
    assert!(matches!(
      result,
      Err(TypeError::InvalidEnumValue {
        enum_name: "Role",
        value: 0x09
      })
    ));
  }

  // ── Status tests ──────────────────────────────────────────────────

  #[test]
  fn status_roundtrip() {
    let cases = [
      (Status::Ok, 0x01),
      (Status::Error, 0x02),
      (Status::Timeout, 0x03),
    ];
    for (variant, wire) in cases {
      assert_eq!(variant.to_wire_byte(), wire);
      assert_eq!(Status::from_wire_byte(wire).unwrap(), variant);
    }
  }

  // ── Priority tests ────────────────────────────────────────────────

  #[test]
  fn priority_roundtrip() {
    let cases = [
      (Priority::Critical, 0x01),
      (Priority::High, 0x02),
      (Priority::Normal, 0x03),
      (Priority::Low, 0x04),
      (Priority::Background, 0x05),
    ];
    for (variant, wire) in cases {
      assert_eq!(variant.to_wire_byte(), wire);
      assert_eq!(Priority::from_wire_byte(wire).unwrap(), variant);
    }
  }

  #[test]
  fn priority_ordering() {
    assert!(Priority::Critical < Priority::High);
    assert!(Priority::High < Priority::Normal);
    assert!(Priority::Normal < Priority::Low);
    assert!(Priority::Low < Priority::Background);
  }

  // ── FormatHint tests ──────────────────────────────────────────────

  #[test]
  fn format_hint_roundtrip() {
    let cases = [
      (FormatHint::Markdown, 0x01),
      (FormatHint::Plain, 0x02),
      (FormatHint::Html, 0x03),
    ];
    for (variant, wire) in cases {
      assert_eq!(variant.to_wire_byte(), wire);
      assert_eq!(FormatHint::from_wire_byte(wire).unwrap(), variant);
    }
  }

  // ── DataFormat tests ──────────────────────────────────────────────

  #[test]
  fn data_format_roundtrip() {
    let cases = [
      (DataFormat::Json, 0x01),
      (DataFormat::Yaml, 0x02),
      (DataFormat::Toml, 0x03),
      (DataFormat::Csv, 0x04),
    ];
    for (variant, wire) in cases {
      assert_eq!(variant.to_wire_byte(), wire);
      assert_eq!(DataFormat::from_wire_byte(wire).unwrap(), variant);
    }
  }

  // ── AnnotationKind tests ──────────────────────────────────────────

  #[test]
  fn annotation_kind_roundtrip() {
    let cases = [
      (AnnotationKind::Priority, 0x01),
      (AnnotationKind::Summary, 0x02),
      (AnnotationKind::Tag, 0x03),
    ];
    for (variant, wire) in cases {
      assert_eq!(variant.to_wire_byte(), wire);
      assert_eq!(AnnotationKind::from_wire_byte(wire).unwrap(), variant);
    }
  }

  // ── MediaType tests ───────────────────────────────────────────────

  #[test]
  fn media_type_roundtrip() {
    let cases = [
      (MediaType::Png, 0x01),
      (MediaType::Jpeg, 0x02),
      (MediaType::Gif, 0x03),
      (MediaType::Svg, 0x04),
      (MediaType::Webp, 0x05),
    ];
    for (variant, wire) in cases {
      assert_eq!(variant.to_wire_byte(), wire);
      assert_eq!(MediaType::from_wire_byte(wire).unwrap(), variant);
    }
  }
}
