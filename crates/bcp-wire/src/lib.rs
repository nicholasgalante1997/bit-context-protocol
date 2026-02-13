#![warn(clippy::pedantic)]

pub mod error;
pub mod varint;
pub mod header;
pub mod block_frame;

pub use error::WireError;