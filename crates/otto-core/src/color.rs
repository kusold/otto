//! Colorized output utilities for stderr
//!
//! Provides semantic colorization for error, warning, and info messages
//! to improve readability and user experience.

use std::io::Write;
use termcolor::{Color, ColorSpec, StandardStream, WriteColor};

/// Colorizes and prints an error message to stderr
///
/// Uses red color with a ✗ symbol to indicate critical errors.
///
/// # Example
/// ```rust
/// use otto_core::color::print_error;
/// print_error("Failed to connect to server");
/// // Outputs: ✗ Error: Failed to connect to server (in red)
/// ```
pub fn print_error(message: &str) {
    let mut stderr = StandardStream::stderr(termcolor::ColorChoice::Auto);
    let _ = stderr.set_color(ColorSpec::new().set_fg(Some(Color::Red)));
    let _ = write!(&mut stderr, "✗ Error: {}", message);
    let _ = stderr.reset();
    let _ = writeln!(&mut stderr);
}

/// Colorizes and prints a warning message to stderr
///
/// Uses yellow color with a ⚠ symbol to indicate warnings.
///
/// # Example
/// ```rust
/// use otto_core::color::print_warning;
/// print_warning("Agent timed out");
/// // Outputs: ⚠ Warning: Agent timed out (in yellow)
/// ```
pub fn print_warning(message: &str) {
    let mut stderr = StandardStream::stderr(termcolor::ColorChoice::Auto);
    let _ = stderr.set_color(ColorSpec::new().set_fg(Some(Color::Yellow)));
    let _ = write!(&mut stderr, "⚠ Warning: {}", message);
    let _ = stderr.reset();
    let _ = writeln!(&mut stderr);
}

/// Colorizes and prints an info message to stderr
///
/// Uses blue color with a ℹ symbol for informational messages.
///
/// # Example
/// ```rust
/// use otto_core::color::print_info;
/// print_info("Agent working...");
/// // Outputs: ℹ Info: Agent working... (in blue)
/// ```
pub fn print_info(message: &str) {
    let mut stderr = StandardStream::stderr(termcolor::ColorChoice::Auto);
    let _ = stderr.set_color(ColorSpec::new().set_fg(Some(Color::Blue)));
    let _ = write!(&mut stderr, "ℹ Info: {}", message);
    let _ = stderr.reset();
    let _ = writeln!(&mut stderr);
}

/// Colorizes and prints a progress message to stderr without newline
///
/// Uses cyan color with a → symbol for progress messages.
/// This function does not add a newline, making it suitable for
/// progress indicators that will be overwritten.
///
/// # Example
/// ```rust
/// use otto_core::color::print_progress;
/// print_progress("Agent working... (1m 23s)");
/// // Outputs: → Agent working... (1m 23s) (in cyan, no newline)
/// ```
pub fn print_progress(message: &str) {
    let mut stderr = StandardStream::stderr(termcolor::ColorChoice::Auto);
    let _ = stderr.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)));
    let _ = write!(&mut stderr, "→ {}", message);
    let _ = stderr.reset();
    let _ = stderr.flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_print_error_doesnt_crash() {
        print_error("Test error message");
    }

    #[test]
    fn test_print_warning_doesnt_crash() {
        print_warning("Test warning message");
    }

    #[test]
    fn test_print_info_doesnt_crash() {
        print_info("Test info message");
    }

    #[test]
    fn test_print_progress_doesnt_crash() {
        print_progress("Test progress message");
    }
}
