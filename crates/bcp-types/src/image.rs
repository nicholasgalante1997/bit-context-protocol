use crate::enums::MediaType;
use crate::error::TypeError;
use crate::fields::{
    decode_bytes_value, decode_field_header, decode_varint_value, encode_bytes_field,
    encode_varint_field, skip_field,
};

/// IMAGE block — image content or reference.
///
/// Can carry either inline image bytes or a URI pointing to an external
/// image. The `media_type` field tells the decoder how to interpret the
/// `data` payload (PNG, JPEG, SVG, etc.).
///
/// Field layout within body:
///
/// ```text
/// ┌──────────┬───────────┬────────────┬──────────────────────────┐
/// │ Field ID │ Wire Type │ Name       │ Description              │
/// ├──────────┼───────────┼────────────┼──────────────────────────┤
/// │ 1        │ Varint    │ media_type │ MediaType enum byte      │
/// │ 2        │ Bytes     │ alt_text   │ Alt text description     │
/// │ 3        │ Bytes     │ data       │ Image bytes or URI       │
/// └──────────┴───────────┴────────────┴──────────────────────────┘
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageBlock {
    pub media_type: MediaType,
    pub alt_text: String,
    /// Raw image bytes (inline) or a UTF-8 URI string (reference).
    /// The block's `IS_REFERENCE` flag in `BlockFlags` distinguishes
    /// between inline data and a URI reference.
    pub data: Vec<u8>,
}

impl ImageBlock {
    /// Serialize this block's fields into a TLV-encoded body.
    pub fn encode_body(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        encode_varint_field(&mut buf, 1, u64::from(self.media_type.to_wire_byte()));
        encode_bytes_field(&mut buf, 2, self.alt_text.as_bytes());
        encode_bytes_field(&mut buf, 3, &self.data);
        buf
    }

    /// Deserialize an IMAGE block from a TLV-encoded body.
    pub fn decode_body(mut buf: &[u8]) -> Result<Self, TypeError> {
        let mut media_type: Option<MediaType> = None;
        let mut alt_text: Option<String> = None;
        let mut data: Option<Vec<u8>> = None;

        while !buf.is_empty() {
            let (header, n) = decode_field_header(buf)?;
            buf = &buf[n..];

            match header.field_id {
                1 => {
                    let (v, n) = decode_varint_value(buf)?;
                    buf = &buf[n..];
                    media_type = Some(MediaType::from_wire_byte(v as u8)?);
                }
                2 => {
                    let (d, n) = decode_bytes_value(buf)?;
                    buf = &buf[n..];
                    alt_text = Some(String::from_utf8_lossy(d).into_owned());
                }
                3 => {
                    let (d, n) = decode_bytes_value(buf)?;
                    buf = &buf[n..];
                    data = Some(d.to_vec());
                }
                _ => {
                    let n = skip_field(buf, header.wire_type)?;
                    buf = &buf[n..];
                }
            }
        }

        Ok(Self {
            media_type: media_type.ok_or(TypeError::MissingRequiredField {
                field: "media_type",
            })?,
            alt_text: alt_text.ok_or(TypeError::MissingRequiredField { field: "alt_text" })?,
            data: data.ok_or(TypeError::MissingRequiredField { field: "data" })?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_inline_png() {
        let block = ImageBlock {
            media_type: MediaType::Png,
            alt_text: "A screenshot of the app".to_string(),
            data: vec![0x89, 0x50, 0x4E, 0x47], // PNG magic bytes (truncated)
        };
        let body = block.encode_body();
        let decoded = ImageBlock::decode_body(&body).unwrap();
        assert_eq!(decoded, block);
    }

    #[test]
    fn roundtrip_svg_reference() {
        let block = ImageBlock {
            media_type: MediaType::Svg,
            alt_text: "Architecture diagram".to_string(),
            data: b"https://example.com/diagram.svg".to_vec(),
        };
        let body = block.encode_body();
        let decoded = ImageBlock::decode_body(&body).unwrap();
        assert_eq!(decoded, block);
    }
}
