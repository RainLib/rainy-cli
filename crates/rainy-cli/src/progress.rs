use clap::ValueEnum;
use std::io::{self, IsTerminal, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const COMMAND_STEPS: u64 = 4;
const BAR_WIDTH: usize = 24;
const SPINNER: &[char] = &['-', '\\', '|', '/'];

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ProgressMode {
    Auto,
    Always,
    Never,
}

struct ProgressState {
    current: u64,
    message: String,
    started_at: Instant,
}

struct SharedProgress {
    state: Mutex<ProgressState>,
    running: AtomicBool,
    no_color: bool,
}

pub struct ProgressReporter {
    shared: Option<Arc<SharedProgress>>,
    worker: Option<JoinHandle<()>>,
    plain: bool,
}

impl ProgressReporter {
    pub fn new(mode: ProgressMode, json: bool, quiet: bool, no_color: bool) -> Self {
        if json || quiet || matches!(mode, ProgressMode::Never) {
            return Self::hidden();
        }

        let terminal = io::stderr().is_terminal();
        if matches!(mode, ProgressMode::Auto) && !terminal {
            return Self::hidden();
        }

        let shared = Arc::new(SharedProgress {
            state: Mutex::new(ProgressState {
                current: 0,
                message: String::new(),
                started_at: Instant::now(),
            }),
            running: AtomicBool::new(terminal),
            no_color,
        });
        let worker = terminal.then(|| {
            let shared = Arc::clone(&shared);
            thread::spawn(move || {
                let mut tick = 0;
                while shared.running.load(Ordering::Relaxed) {
                    draw_terminal(&shared, tick);
                    tick = tick.wrapping_add(1);
                    thread::sleep(Duration::from_millis(120));
                }
            })
        });

        Self {
            shared: Some(shared),
            worker,
            plain: !terminal,
        }
    }

    pub fn stage(&mut self, message: impl Into<String>) {
        let Some(shared) = &self.shared else {
            return;
        };
        let mut state = shared.state.lock().expect("progress state");
        state.current = (state.current + 1).min(COMMAND_STEPS);
        state.message = message.into();
        if self.plain {
            eprintln!("[{}/{}] {}", state.current, COMMAND_STEPS, state.message);
        }
    }

    pub fn detail(&self, message: impl Into<String>) {
        let Some(shared) = &self.shared else {
            return;
        };
        let mut state = shared.state.lock().expect("progress state");
        state.message = message.into();
        if self.plain {
            eprintln!("      {}", state.message);
        }
    }

    pub fn finish_success(&mut self) {
        let Some(shared) = &self.shared else {
            return;
        };
        let elapsed = {
            let mut state = shared.state.lock().expect("progress state");
            state.current = COMMAND_STEPS;
            state.message = "Completed".to_string();
            state.started_at.elapsed()
        };
        self.stop_worker();
        if self.plain {
            eprintln!(
                "[{COMMAND_STEPS}/{COMMAND_STEPS}] Completed in {}",
                format_duration(elapsed)
            );
        } else {
            clear_terminal_line();
            eprintln!("Completed in {}", format_duration(elapsed));
        }
    }

    pub fn finish_error(&mut self) {
        let Some(shared) = &self.shared else {
            return;
        };
        let elapsed = {
            let mut state = shared.state.lock().expect("progress state");
            state.message = "Failed".to_string();
            state.started_at.elapsed()
        };
        self.stop_worker();
        if self.plain {
            eprintln!(
                "[{COMMAND_STEPS}/{COMMAND_STEPS}] Failed in {}",
                format_duration(elapsed)
            );
        } else {
            clear_terminal_line();
            eprintln!("Failed in {}", format_duration(elapsed));
        }
    }

    fn stop_worker(&mut self) {
        if let Some(shared) = &self.shared {
            shared.running.store(false, Ordering::Relaxed);
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }

    fn hidden() -> Self {
        Self {
            shared: None,
            worker: None,
            plain: false,
        }
    }
}

impl Drop for ProgressReporter {
    fn drop(&mut self) {
        self.stop_worker();
    }
}

fn draw_terminal(shared: &SharedProgress, tick: usize) {
    let state = shared.state.lock().expect("progress state");
    if state.current == 0 {
        return;
    }
    let filled = BAR_WIDTH * state.current as usize / COMMAND_STEPS as usize;
    let bar = format!("{}{}", "=".repeat(filled), "-".repeat(BAR_WIDTH - filled));
    let elapsed = state.started_at.elapsed().as_secs();
    let message = truncate(&state.message, 64);
    let spinner = SPINNER[tick % SPINNER.len()];
    let line = format!(
        "{spinner} [{bar}] {}/{} {message} ({elapsed}s)",
        state.current, COMMAND_STEPS
    );
    let mut stderr = io::stderr().lock();
    if shared.no_color {
        let _ = write!(stderr, "\r\x1b[2K{line}");
    } else {
        let _ = write!(stderr, "\r\x1b[2K\x1b[36m{line}\x1b[0m");
    }
    let _ = stderr.flush();
}

fn clear_terminal_line() {
    let mut stderr = io::stderr().lock();
    let _ = write!(stderr, "\r\x1b[2K");
    let _ = stderr.flush();
}

fn truncate(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    value.chars().take(max_chars - 3).collect::<String>() + "..."
}

fn format_duration(duration: Duration) -> String {
    if duration.as_secs() == 0 {
        "<1s".to_string()
    } else if duration.as_secs() < 10 {
        format!("{:.1}s", duration.as_secs_f64())
    } else {
        format!("{}s", duration.as_secs())
    }
}
