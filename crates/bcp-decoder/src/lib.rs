#![warn(clippy::pedantic)]

pub mod block_reader;
pub mod decoder;
pub mod error;
pub mod streaming;

mod decompression;

pub use decoder::{DecodedPayload, LcpDecoder};
pub use error::DecodeError;
pub use streaming::{DecoderEvent, StreamingDecoder};
