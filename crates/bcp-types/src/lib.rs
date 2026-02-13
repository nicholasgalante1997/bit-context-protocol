#![warn(clippy::pedantic)]

pub mod error;
pub mod fields;
pub mod block_type;
pub mod enums;
pub mod summary;
pub mod code;
pub mod conversation;
pub mod file_tree;
pub mod tool_result;
pub mod document;
pub mod structured_data;
pub mod diff;
pub mod annotation;
pub mod embedding_ref;
pub mod image;
pub mod extension;
pub mod end;
pub mod block;

pub use block::{Block, BlockContent};
pub use block_type::BlockType;
pub use enums::{
  AnnotationKind, DataFormat, FormatHint, Lang, MediaType, Priority, Role, Status,
};
pub use error::TypeError;
pub use fields::FieldWireType;
pub use summary::Summary;
