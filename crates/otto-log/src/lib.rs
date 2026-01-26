//! Colorized output utilities for stderr
//!
//! The `otto-log` crate provides semantic colorization for error, warning,
//! info, and progress messages to improve readability and user experience.
//!
//! # Example
//!
//! ```rust
//! use otto_log::color::{print_error, print_warning, print_info, print_progress};
//!
//! print_error("Failed to connect to server");
//! print_warning("Agent timed out");
//! print_info("Agent working...");
//! print_progress("Agent working... (1m 23s)");
//! ```

pub mod color;
