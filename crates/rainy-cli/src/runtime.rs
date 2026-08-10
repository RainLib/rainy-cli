use crate::progress::ProgressReporter;
use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    Human,
    Json,
    Quiet,
}

#[derive(Debug, Clone)]
pub struct TerminalCapabilities {
    pub interactive: bool,
    pub color: bool,
    pub width: usize,
}

impl TerminalCapabilities {
    pub fn detect(no_color: bool, output_mode: OutputMode) -> Self {
        let terminal = io::stdin().is_terminal() && io::stderr().is_terminal();
        let dumb = std::env::var("TERM").is_ok_and(|term| term.eq_ignore_ascii_case("dumb"));
        let width = terminal_size::terminal_size()
            .map(|(terminal_size::Width(width), _)| usize::from(width))
            .unwrap_or(80);
        Self {
            interactive: terminal && !dumb && output_mode == OutputMode::Human,
            color: terminal && !dumb && !no_color && output_mode == OutputMode::Human,
            width,
        }
    }
}

pub struct RunContext<'a> {
    workspace: PathBuf,
    pub output_mode: OutputMode,
    pub terminal: TerminalCapabilities,
    pub trace_id: Option<String>,
    pub progress: &'a ProgressReporter,
}

impl<'a> RunContext<'a> {
    pub fn new(
        workspace: PathBuf,
        output_mode: OutputMode,
        terminal: TerminalCapabilities,
        trace_id: Option<String>,
        progress: &'a ProgressReporter,
    ) -> Self {
        Self {
            workspace,
            output_mode,
            terminal,
            trace_id,
            progress,
        }
    }

    pub fn workspace(&self) -> &Path {
        &self.workspace
    }

    pub fn cancelled(&self) -> bool {
        crate::progress::cancelled()
    }
}
