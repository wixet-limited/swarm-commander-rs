#![warn(rustdoc::invalid_rust_codeblocks)]
#![deny(rustdoc::missing_doc_code_examples)]
#![warn(missing_docs)]
#![warn(rustdoc::missing_crate_level_docs)]

//! This crate is meant to handle tons of commands (processes) in a simple and powerful way. The idea is that you only
//! have to send the command and process the stderr and stdout in an asynchronous way. But always controlling the status
//! of the commands (start, stop, if exit know the reason...)
//! 
//! Check the function [`run_hive`] to see an example of use
mod event;
mod runner;

pub use runner::{Hive, run_hive};
pub use event::{RunnerEvent, RunnerLogEvent, RunnerStartEvent, RunnerStopEvent, StatusEvent, StdType};