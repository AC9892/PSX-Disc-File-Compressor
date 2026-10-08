pub mod cue;
pub mod metadata;
pub mod compression;
pub mod container;
pub use container::{compress, extract, inspect, verify, DiscInfo, Manifest, Progress, Report};
#[cfg(test)] mod tests;
