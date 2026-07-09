//! ANSI styling for terminal output.

use std::io::IsTerminal;

use crate::finding::Severity;

/// When to color output, from `--color`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum ColorChoice {
    /// Color when stdout is a terminal and `NO_COLOR` is not set.
    Auto,
    Always,
    Never,
}

#[derive(Debug, Clone, Copy)]
pub struct Style {
    on: bool,
}

impl Style {
    pub const PLAIN: Style = Style { on: false };

    pub fn new(choice: ColorChoice) -> Self {
        let on = match choice {
            ColorChoice::Always => true,
            ColorChoice::Never => false,
            ColorChoice::Auto => {
                std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty())
                    && std::io::stdout().is_terminal()
            }
        };
        Self { on }
    }

    fn paint(self, code: &str, text: &str) -> String {
        if self.on {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }

    pub fn bold(self, text: &str) -> String {
        self.paint("1", text)
    }

    pub fn dim(self, text: &str) -> String {
        self.paint("2", text)
    }

    pub fn red(self, text: &str) -> String {
        self.paint("1;31", text)
    }

    pub fn yellow(self, text: &str) -> String {
        self.paint("33", text)
    }

    pub fn green(self, text: &str) -> String {
        self.paint("32", text)
    }

    pub fn severity(self, severity: Severity, text: &str) -> String {
        match severity {
            Severity::High => self.red(text),
            Severity::Medium => self.yellow(text),
            Severity::Low => self.dim(text),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_adds_no_codes() {
        assert_eq!(Style::PLAIN.red("high"), "high");
        assert_eq!(
            Style::new(ColorChoice::Always).red("high"),
            "\x1b[1;31mhigh\x1b[0m"
        );
    }
}
