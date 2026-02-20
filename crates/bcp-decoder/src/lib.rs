#![warn(clippy::pedantic)]

//! BCP decoder — parses binary payloads into typed blocks.
//!
//! Two APIs are provided:
//!
//! - [`BcpDecoder`] — synchronous, operates on a complete `&[u8]` slice.
//! - [`StreamingDecoder`] — asynchronous, reads from any `AsyncRead` source
//!   and yields blocks incrementally.
//!
//! **Compression and streaming**: The streaming decoder provides true
//! incremental parsing for uncompressed and per-block-compressed payloads.
//! However, whole-payload compression (`HeaderFlags::COMPRESSED`) forces
//! the streaming decoder to buffer the entire payload before yielding
//! blocks — the streaming API surface is preserved but the memory and
//! latency benefits are lost. Prefer per-block compression when streaming
//! matters.

pub mod block_reader;
pub mod decoder;
pub mod error;
pub mod streaming;

mod decompression;

pub use decoder::{DecodedPayload, BcpDecoder};
pub use error::DecodeError;
pub use streaming::{DecoderEvent, StreamingDecoder};
