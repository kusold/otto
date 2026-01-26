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
/// use otto_log::color::print_error;
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
/// use otto_log::color::print_warning;
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
/// use otto_log::color::print_info;
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
/// use otto_log::color::print_progress;
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

    #[test]
    fn test_print_error_with_empty_message() {
        print_error("");
    }

    #[test]
    fn test_print_error_with_unicode() {
        print_error("Error with unicode: 🚨 💥 ⚠️");
    }

    #[test]
    fn test_print_error_with_long_message() {
        let long_msg = "This is a very long error message that spans multiple lines and contains lots of information about what went wrong. ".repeat(10);
        print_error(&long_msg);
    }

    #[test]
    fn test_print_warning_with_empty_message() {
        print_warning("");
    }

    #[test]
    fn test_print_warning_with_unicode() {
        print_warning("Warning with unicode: ⚠️ 🚸 🔔");
    }

    #[test]
    fn test_print_warning_with_long_message() {
        let long_msg = "This is a very long warning message that spans multiple lines and contains lots of information about what might go wrong. ".repeat(10);
        print_warning(&long_msg);
    }

    #[test]
    fn test_print_info_with_empty_message() {
        print_info("");
    }

    #[test]
    fn test_print_info_with_unicode() {
        print_info("Info with unicode: ℹ️ 💡 📝");
    }

    #[test]
    fn test_print_info_with_long_message() {
        let long_msg = "This is a very long info message that spans multiple lines and contains lots of information about what is happening. ".repeat(10);
        print_info(&long_msg);
    }

    #[test]
    fn test_print_progress_with_empty_message() {
        print_progress("");
    }

    #[test]
    fn test_print_progress_with_unicode() {
        print_progress("Progress with unicode: ⏳ 🚀 📊");
    }

    #[test]
    fn test_print_progress_with_long_message() {
        let long_msg = "This is a very long progress message that shows the current status of the operation. ".repeat(10);
        print_progress(&long_msg);
    }

    #[test]
    fn test_print_error_with_special_chars() {
        print_error("Error with special chars: \t\n\r\"'\\");
    }

    #[test]
    fn test_print_warning_with_special_chars() {
        print_warning("Warning with special chars: \t\n\r\"'\\");
    }

    #[test]
    fn test_print_info_with_special_chars() {
        print_info("Info with special chars: \t\n\r\"'\\");
    }

    #[test]
    fn test_print_progress_with_special_chars() {
        print_progress("Progress with special chars: \t\n\r\"'\\");
    }

    #[test]
    fn test_print_error_function_exists() {
        // Verify function signature
        let _ = print_error as fn(&str);
    }

    #[test]
    fn test_print_warning_function_exists() {
        // Verify function signature
        let _ = print_warning as fn(&str);
    }

    #[test]
    fn test_print_info_function_exists() {
        // Verify function signature
        let _ = print_info as fn(&str);
    }

    #[test]
    fn test_print_progress_function_exists() {
        // Verify function signature
        let _ = print_progress as fn(&str);
    }

    #[test]
    fn test_multiple_prints_in_sequence() {
        // Test that multiple prints work without interfering
        print_error("First error");
        print_warning("Second warning");
        print_info("Third info");
        print_progress("Fourth progress");
    }

    #[test]
    fn test_print_functions_with_newlines() {
        print_error("Error\nwith\nnewlines");
        print_warning("Warning\nwith\nnewlines");
        print_info("Info\nwith\nnewlines");
        // print_progress doesn't add newline, so we skip it
    }

    #[test]
    fn test_all_print_functions_are_callable() {
        // Ensure all public functions are accessible
        print_error("error");
        print_warning("warning");
        print_info("info");
        print_progress("progress");
    }
}
