#![warn(clippy::pedantic)]

pub mod annotation;
pub mod block;
pub mod block_type;
pub mod code;
pub mod content_store;
pub mod conversation;
pub mod diff;
pub mod document;
pub mod embedding_ref;
pub mod end;
pub mod enums;
pub mod error;
pub mod extension;
pub mod fields;
pub mod file_tree;
pub mod image;
pub mod structured_data;
pub mod summary;
pub mod tool_result;

pub use block::{Block, BlockContent};
pub use block_type::BlockType;
pub use enums::{AnnotationKind, DataFormat, FormatHint, Lang, MediaType, Priority, Role, Status};
pub use content_store::{ContentStore, REFERENCE_BODY_SIZE};
pub use error::TypeError;
pub use fields::FieldWireType;
pub use summary::Summary;
