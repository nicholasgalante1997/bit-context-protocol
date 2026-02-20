#![no_main]

use arbitrary::{Arbitrary, Unstructured};
use libfuzzer_sys::fuzz_target;
use bcp_encoder::BcpEncoder;
use bcp_decoder::BcpDecoder;
use bcp_types::enums::{AnnotationKind, DataFormat, FormatHint, Lang, Role, Status};

#[derive(Debug, Arbitrary)]
enum FuzzBlock {
    Code {
        lang_id: u8,
        path: String,
        content: Vec<u8>,
    },
    Conversation {
        role_id: u8,
        content: Vec<u8>,
    },
    Document {
        title: String,
        content: Vec<u8>,
        format_id: u8,
    },
    ToolResult {
        name: String,
        status_id: u8,
        content: Vec<u8>,
    },
    StructuredData {
        format_id: u8,
        content: Vec<u8>,
    },
    Annotation {
        kind_id: u8,
        target_id: u32,
        content: Vec<u8>,
    },
    Extension {
        namespace: String,
        type_name: String,
        data: Vec<u8>,
    },
}

#[derive(Debug, Arbitrary)]
struct FuzzInput {
    blocks: Vec<FuzzBlock>,
    compress_payload: bool,
    compress_blocks: bool,
}

fn lang_from_id(id: u8) -> Lang {
    match id % 19 {
        0 => Lang::Rust,
        1 => Lang::TypeScript,
        2 => Lang::JavaScript,
        3 => Lang::Python,
        4 => Lang::Go,
        5 => Lang::Java,
        6 => Lang::C,
        7 => Lang::Cpp,
        8 => Lang::Ruby,
        9 => Lang::Shell,
        10 => Lang::Sql,
        11 => Lang::Html,
        12 => Lang::Css,
        13 => Lang::Json,
        14 => Lang::Yaml,
        15 => Lang::Toml,
        16 => Lang::Markdown,
        17 => Lang::Unknown,
        _ => Lang::Other(id),
    }
}

fn role_from_id(id: u8) -> Role {
    match id % 4 {
        0 => Role::System,
        1 => Role::User,
        2 => Role::Assistant,
        _ => Role::Tool,
    }
}

fn status_from_id(id: u8) -> Status {
    match id % 3 {
        0 => Status::Ok,
        1 => Status::Error,
        _ => Status::Timeout,
    }
}

fn data_format_from_id(id: u8) -> DataFormat {
    match id % 4 {
        0 => DataFormat::Json,
        1 => DataFormat::Yaml,
        2 => DataFormat::Toml,
        _ => DataFormat::Csv,
    }
}

fn format_hint_from_id(id: u8) -> FormatHint {
    match id % 3 {
        0 => FormatHint::Markdown,
        1 => FormatHint::Plain,
        _ => FormatHint::Html,
    }
}

fn annotation_kind_from_id(id: u8) -> AnnotationKind {
    match id % 3 {
        0 => AnnotationKind::Priority,
        1 => AnnotationKind::Summary,
        _ => AnnotationKind::Tag,
    }
}

// Fuzz target: BcpEncoder -> BcpDecoder roundtrip.
//
// Generates structured payloads via the encoder, then decodes them.
// The decoder must not panic on anything the encoder produces.
fuzz_target!(|data: &[u8]| {
    let mut u = Unstructured::new(data);
    let Ok(input) = FuzzInput::arbitrary(&mut u) else {
        return;
    };

    if input.blocks.is_empty() {
        return;
    }

    let block_count = input.blocks.len().min(32);

    let mut encoder = BcpEncoder::new();

    if input.compress_payload {
        encoder.compress_payload();
    } else if input.compress_blocks {
        encoder.compress_blocks();
    }

    for block in &input.blocks[..block_count] {
        match block {
            FuzzBlock::Code { lang_id, path, content } => {
                encoder.add_code(lang_from_id(*lang_id), path, content);
            }
            FuzzBlock::Conversation { role_id, content } => {
                encoder.add_conversation(role_from_id(*role_id), content);
            }
            FuzzBlock::Document { title, content, format_id } => {
                encoder.add_document(title, content, format_hint_from_id(*format_id));
            }
            FuzzBlock::ToolResult { name, status_id, content } => {
                encoder.add_tool_result(name, status_from_id(*status_id), content);
            }
            FuzzBlock::StructuredData { format_id, content } => {
                encoder.add_structured_data(data_format_from_id(*format_id), content);
            }
            FuzzBlock::Annotation { kind_id, target_id, content } => {
                encoder.add_annotation(
                    *target_id,
                    annotation_kind_from_id(*kind_id),
                    content,
                );
            }
            FuzzBlock::Extension { namespace, type_name, data } => {
                encoder.add_extension(namespace, type_name, data);
            }
        }
    }

    let Ok(payload) = encoder.encode() else {
        return;
    };

    let decoded = BcpDecoder::decode(&payload);
    assert!(decoded.is_ok(), "decoder failed on valid encoder output: {:?}", decoded.err());

    let decoded = decoded.unwrap();
    assert_eq!(decoded.blocks.len(), block_count);
});
