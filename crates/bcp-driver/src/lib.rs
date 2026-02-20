#![warn(clippy::pedantic)]

pub mod budget;
pub mod config;
pub mod driver;
pub mod error;
pub mod render_markdown;
pub mod render_minimal;
pub mod render_xml;

mod placeholder;

pub use budget::{CodeAwareEstimator, HeuristicEstimator, RenderDecision, TokenEstimator};
pub use config::{DriverConfig, ModelFamily, OutputMode, Verbosity};
pub use driver::{DefaultDriver, BcpDriver};
pub use error::DriverError;
