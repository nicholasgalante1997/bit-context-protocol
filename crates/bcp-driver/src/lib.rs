#![warn(clippy::pedantic)]

pub mod config;
pub mod driver;
pub mod error;
pub mod render_markdown;
pub mod render_minimal;
pub mod render_xml;

mod budget;

pub use config::{DriverConfig, ModelFamily, OutputMode};
pub use driver::{DefaultDriver, LcpDriver};
pub use error::DriverError;
