#![warn(clippy::pedantic)]

pub mod block_frame;
pub mod error;
pub mod header;
pub mod varint;

pub use error::WireError;
