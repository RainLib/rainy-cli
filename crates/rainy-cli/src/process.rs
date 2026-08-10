use crate::error::{RainyError, RainyResult};
use command_group::CommandGroup;
#[cfg(unix)]
use command_group::{Signal, UnixChildExt};
use std::ffi::OsStr;
use std::io::Read;
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

pub const DEFAULT_OUTPUT_LIMIT: usize = 10 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Termination {
    Exited,
    TimedOut,
    Cancelled,
}

#[derive(Debug)]
pub struct ProcessOutput {
    pub status: Option<ExitStatus>,
    pub stdout: String,
    pub stderr: String,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub termination: Termination,
    pub duration: Duration,
}

impl ProcessOutput {
    pub fn success(&self) -> bool {
        self.termination == Termination::Exited
            && self.status.is_some_and(|status| status.success())
    }
}

pub fn run<I, S>(
    program: &str,
    args: I,
    cwd: &Path,
    timeout: Duration,
    output_limit: usize,
) -> RainyResult<ProcessOutput>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new(program);
    command.args(args).current_dir(cwd);
    run_command(command, program, timeout, output_limit)
}

pub fn run_command<P: AsRef<OsStr>>(
    command: Command,
    display_program: P,
    timeout: Duration,
    output_limit: usize,
) -> RainyResult<ProcessOutput> {
    run_command_until(
        command,
        display_program,
        timeout,
        output_limit,
        crate::progress::cancelled,
    )
}

fn run_command_until<P, F>(
    mut command: Command,
    display_program: P,
    timeout: Duration,
    output_limit: usize,
    cancelled: F,
) -> RainyResult<ProcessOutput>
where
    P: AsRef<OsStr>,
    F: Fn() -> bool,
{
    crate::progress::detail_current(format!(
        "Running external process: {}",
        display_program.as_ref().to_string_lossy()
    ));
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.group_spawn().map_err(|error| {
        RainyError::action(
            "PROCESS_SPAWN_FAILED",
            format!(
                "could not start {}: {error}",
                display_program.as_ref().to_string_lossy()
            ),
        )
    })?;
    let stdout = child.inner().stdout.take().ok_or_else(|| {
        RainyError::action("PROCESS_CAPTURE_FAILED", "child stdout was not captured")
    })?;
    let stderr = child.inner().stderr.take().ok_or_else(|| {
        RainyError::action("PROCESS_CAPTURE_FAILED", "child stderr was not captured")
    })?;
    let stdout_reader = thread::spawn(move || read_bounded(stdout, output_limit));
    let stderr_reader = thread::spawn(move || read_bounded(stderr, output_limit));
    let started = Instant::now();
    let mut termination = Termination::Exited;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        if cancelled() {
            termination = Termination::Cancelled;
            terminate_group(&mut child, Duration::from_secs(2));
            break child.wait().ok();
        }
        if started.elapsed() >= timeout {
            termination = Termination::TimedOut;
            let _ = child.kill();
            break child.wait().ok();
        }
        thread::sleep(Duration::from_millis(50));
    };
    let (stdout, stdout_truncated) = join_reader(stdout_reader)?;
    let (stderr, stderr_truncated) = join_reader(stderr_reader)?;
    Ok(ProcessOutput {
        status,
        stdout: crate::redaction::text(&String::from_utf8_lossy(&stdout)),
        stderr: crate::redaction::text(&String::from_utf8_lossy(&stderr)),
        stdout_truncated,
        stderr_truncated,
        termination,
        duration: started.elapsed(),
    })
}

fn read_bounded(mut reader: impl Read, limit: usize) -> std::io::Result<(Vec<u8>, bool)> {
    let mut output = Vec::with_capacity(limit.min(64 * 1024));
    let mut truncated = false;
    let mut buffer = [0_u8; 8192];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        let remaining = limit.saturating_sub(output.len());
        output.extend_from_slice(&buffer[..read.min(remaining)]);
        truncated |= read > remaining;
    }
    Ok((output, truncated))
}

fn join_reader(
    reader: thread::JoinHandle<std::io::Result<(Vec<u8>, bool)>>,
) -> RainyResult<(Vec<u8>, bool)> {
    reader
        .join()
        .map_err(|_| RainyError::action("PROCESS_CAPTURE_FAILED", "output reader panicked"))?
        .map_err(RainyError::from)
}

fn terminate_group(child: &mut command_group::GroupChild, grace: Duration) {
    #[cfg(unix)]
    let _ = child.signal(Signal::SIGTERM);

    let deadline = Instant::now() + grace;
    while Instant::now() < deadline {
        if child.try_wait().ok().flatten().is_some() {
            return;
        }
        thread::sleep(Duration::from_millis(50));
    }
    let _ = child.kill();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    #[cfg(unix)]
    fn limits_output_and_times_out_process_groups() {
        let output = run(
            "sh",
            ["-c", "printf 123456789; sleep 5"],
            Path::new("."),
            Duration::from_millis(100),
            4,
        )
        .expect("process output");
        assert_eq!(output.termination, Termination::TimedOut);
        assert_eq!(output.stdout, "1234");
        assert!(output.stdout_truncated);
    }

    #[test]
    #[cfg(unix)]
    fn timeout_terminates_descendant_processes() {
        let output = run(
            "sh",
            ["-c", "sleep 30 & child=$!; printf '%s' \"$child\"; wait"],
            Path::new("."),
            Duration::from_millis(150),
            DEFAULT_OUTPUT_LIMIT,
        )
        .expect("process output");
        assert_eq!(output.termination, Termination::TimedOut);
        let pid = output.stdout.trim().to_string();
        assert!(!pid.is_empty(), "child process ID was not captured");
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            let alive = Command::new("kill")
                .args(["-0", &pid])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|status| status.success());
            if !alive {
                return;
            }
            thread::sleep(Duration::from_millis(50));
        }
        panic!("timed-out descendant process {pid} was not terminated");
    }

    #[test]
    #[cfg(unix)]
    fn cancellation_terminates_descendant_processes() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let trigger = Arc::clone(&cancelled);
        let worker = thread::spawn(move || {
            thread::sleep(Duration::from_millis(150));
            trigger.store(true, Ordering::SeqCst);
        });
        let mut command = Command::new("sh");
        command.args(["-c", "sleep 30 & child=$!; printf '%s' \"$child\"; wait"]);
        let output = run_command_until(
            command,
            "sh",
            Duration::from_secs(10),
            DEFAULT_OUTPUT_LIMIT,
            || cancelled.load(Ordering::SeqCst),
        )
        .expect("process output");
        worker.join().expect("cancellation trigger");
        assert_eq!(output.termination, Termination::Cancelled);
        let pid = output.stdout.trim().to_string();
        assert!(!pid.is_empty(), "child process ID was not captured");
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            let alive = Command::new("kill")
                .args(["-0", &pid])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|status| status.success());
            if !alive {
                return;
            }
            thread::sleep(Duration::from_millis(50));
        }
        panic!("cancelled descendant process {pid} was not terminated");
    }
}
