use clap::ValueEnum;
use std::io::{self, IsTerminal, Write};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const BAR_WIDTH: usize = 20;
const SPINNER: &[char] = &['-', '\\', '|', '/'];

static DYNAMIC_PROGRESS_ACTIVE: AtomicBool = AtomicBool::new(false);
static CANCELLED: AtomicBool = AtomicBool::new(false);
static ACTIVE_PROGRESS: Mutex<Option<Weak<SharedProgress>>> = Mutex::new(None);

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ProgressMode {
    Auto,
    Always,
    Never,
}

struct ProgressState {
    current: u64,
    total: Option<u64>,
    message: String,
    started_at: Instant,
}

struct SharedProgress {
    state: Mutex<ProgressState>,
    running: AtomicBool,
    suspended: AtomicUsize,
    no_color: bool,
    plain: bool,
}

pub struct ProgressReporter {
    shared: Option<Arc<SharedProgress>>,
    worker: Mutex<Option<JoinHandle<()>>>,
    plain: bool,
}

pub struct ProgressSuspension<'a> {
    reporter: &'a ProgressReporter,
    active: bool,
}

impl ProgressReporter {
    pub fn new(mode: ProgressMode, json: bool, quiet: bool, no_color: bool) -> Self {
        if json || quiet || matches!(mode, ProgressMode::Never) {
            return Self::hidden();
        }

        let terminal = io::stderr().is_terminal()
            && !std::env::var("TERM").is_ok_and(|term| term.eq_ignore_ascii_case("dumb"));
        let dynamic_terminal = terminal;
        let shared = Arc::new(SharedProgress {
            state: Mutex::new(ProgressState {
                current: 0,
                total: None,
                message: String::new(),
                started_at: Instant::now(),
            }),
            running: AtomicBool::new(dynamic_terminal),
            suspended: AtomicUsize::new(0),
            no_color,
            plain: !dynamic_terminal,
        });
        *ACTIVE_PROGRESS.lock().expect("active progress") = Some(Arc::downgrade(&shared));
        let worker = dynamic_terminal.then(|| {
            DYNAMIC_PROGRESS_ACTIVE.store(true, Ordering::SeqCst);
            let shared = Arc::clone(&shared);
            thread::spawn(move || {
                let mut tick = 0;
                while shared.running.load(Ordering::SeqCst) {
                    if shared.suspended.load(Ordering::SeqCst) == 0 {
                        draw_terminal(&shared, tick);
                    }
                    tick = tick.wrapping_add(1);
                    thread::sleep(Duration::from_millis(120));
                }
            })
        });

        Self {
            shared: Some(shared),
            worker: Mutex::new(worker),
            plain: !dynamic_terminal,
        }
    }

    pub fn stage(&self, message: impl Into<String>) {
        let Some(shared) = &self.shared else {
            return;
        };
        let mut state = shared.state.lock().expect("progress state");
        state.current += 1;
        state.message = message.into();
        if self.plain {
            eprintln!("{}", event_line(&state));
        }
    }

    pub fn detail(&self, message: impl Into<String>) {
        let Some(shared) = &self.shared else {
            return;
        };
        let mut state = shared.state.lock().expect("progress state");
        state.message = message.into();
        if self.plain {
            eprintln!("  {}", state.message);
        }
    }

    pub fn suspend(&self) -> ProgressSuspension<'_> {
        if let Some(shared) = &self.shared
            && shared.suspended.fetch_add(1, Ordering::SeqCst) == 0
            && !self.plain
        {
            clear_terminal_line();
        }
        ProgressSuspension {
            reporter: self,
            active: true,
        }
    }

    pub fn finish_success(&self) {
        self.finish("Completed", false);
    }

    pub fn finish_error(&self) {
        self.finish("Failed", true);
    }

    pub fn finish_cancelled(&self) {
        self.finish("Cancelled", true);
    }

    fn finish(&self, label: &str, failed: bool) {
        let Some(shared) = &self.shared else {
            return;
        };
        let (elapsed, current, total) = {
            let mut state = shared.state.lock().expect("progress state");
            state.message = label.to_string();
            (state.started_at.elapsed(), state.current, state.total)
        };
        self.stop_worker();
        if !self.plain {
            clear_terminal_line();
        }
        let prefix = match total {
            Some(total) => format!("[{current}/{total}] "),
            None if current > 0 => format!("[{current}] "),
            None => String::new(),
        };
        let verb = if failed { label } else { "Completed" };
        eprintln!("{prefix}{verb} in {}", format_duration(elapsed));
    }

    fn stop_worker(&self) {
        if let Some(shared) = &self.shared {
            shared.running.store(false, Ordering::SeqCst);
        }
        if let Some(worker) = self.worker.lock().expect("progress worker").take() {
            let _ = worker.join();
        }
        DYNAMIC_PROGRESS_ACTIVE.store(false, Ordering::SeqCst);
    }

    fn hidden() -> Self {
        *ACTIVE_PROGRESS.lock().expect("active progress") = None;
        Self {
            shared: None,
            worker: Mutex::new(None),
            plain: false,
        }
    }
}

impl Drop for ProgressSuspension<'_> {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        if let Some(shared) = &self.reporter.shared {
            shared.suspended.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

pub fn install_interrupt_handler() -> Result<(), ctrlc::Error> {
    ctrlc::set_handler(|| {
        CANCELLED.store(true, Ordering::SeqCst);
        if DYNAMIC_PROGRESS_ACTIVE.load(Ordering::SeqCst) {
            clear_terminal_line();
        }
    })
}

pub fn cancelled() -> bool {
    CANCELLED.load(Ordering::SeqCst)
}

pub fn detail_current(message: impl Into<String>) {
    let Some(shared) = ACTIVE_PROGRESS
        .lock()
        .expect("active progress")
        .as_ref()
        .and_then(Weak::upgrade)
    else {
        return;
    };
    let mut state = shared.state.lock().expect("progress state");
    state.message = message.into();
    if shared.plain {
        eprintln!("  {}", state.message);
    }
}

impl Drop for ProgressReporter {
    fn drop(&mut self) {
        self.stop_worker();
        *ACTIVE_PROGRESS.lock().expect("active progress") = None;
    }
}

fn event_line(state: &ProgressState) -> String {
    match state.total {
        Some(total) => format!("[{}/{}] {}", state.current, total, state.message),
        None => format!("[{}] {}", state.current, state.message),
    }
}

fn draw_terminal(shared: &SharedProgress, tick: usize) {
    let state = shared.state.lock().expect("progress state");
    if state.current == 0 {
        return;
    }
    let elapsed = state.started_at.elapsed().as_secs();
    let message = truncate(&state.message, terminal_message_width());
    let spinner = SPINNER[tick % SPINNER.len()];
    let line = if let Some(total) = state.total {
        let filled = BAR_WIDTH * state.current.min(total) as usize / total.max(1) as usize;
        let bar = format!("{}{}", "=".repeat(filled), "-".repeat(BAR_WIDTH - filled));
        format!(
            "{spinner} [{bar}] {}/{} {message} ({elapsed}s)",
            state.current, total
        )
    } else {
        format!("{spinner} [{}] {message} ({elapsed}s)", state.current)
    };
    let mut stderr = io::stderr().lock();
    if shared.no_color {
        let _ = write!(stderr, "\r\x1b[2K{line}");
    } else {
        let _ = write!(stderr, "\r\x1b[2K\x1b[36m{line}\x1b[0m");
    }
    let _ = stderr.flush();
}

fn terminal_message_width() -> usize {
    terminal_size::terminal_size()
        .map(|(terminal_size::Width(width), _)| usize::from(width).saturating_sub(34).max(12))
        .unwrap_or(64)
}

fn clear_terminal_line() {
    let mut stderr = io::stderr().lock();
    let _ = write!(stderr, "\r\x1b[2K\x1b[?25h");
    let _ = stderr.flush();
}

fn truncate(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    value
        .chars()
        .take(max_chars.saturating_sub(3))
        .collect::<String>()
        + "..."
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
