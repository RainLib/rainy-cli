use crate::error::{RainyError, RainyResult};
use crate::output::CommandOutput;
use chrono::Utc;
use fs2::FileExt;
use serde::Serialize;
use std::io::Write;
use std::path::Path;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AuditRecord {
    protocol_version: &'static str,
    timestamp: String,
    command: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    trace_id: Option<String>,
    output_type: String,
    summary: String,
}

pub fn record_success(
    workspace: &Path,
    command: &str,
    trace_id: Option<&str>,
    output: &CommandOutput,
) -> RainyResult<()> {
    if output.is_dry_run() || !workspace.join("rainy.yaml").exists() {
        return Ok(());
    }
    append(
        workspace,
        AuditRecord {
            protocol_version: "rainy.audit.v1",
            timestamp: Utc::now().to_rfc3339(),
            command: command.to_string(),
            status: output.status().to_string(),
            trace_id: trace_id.map(str::to_string),
            output_type: output.kind().to_string(),
            summary: output.audit_summary(),
        },
    )
}

pub fn record_error(
    workspace: &Path,
    command: &str,
    trace_id: Option<&str>,
    error: &RainyError,
) -> RainyResult<()> {
    if !workspace.join("rainy.yaml").exists() {
        return Ok(());
    }
    let body = error.body();
    append(
        workspace,
        AuditRecord {
            protocol_version: "rainy.audit.v1",
            timestamp: Utc::now().to_rfc3339(),
            command: command.to_string(),
            status: "error".to_string(),
            trace_id: trace_id.map(str::to_string),
            output_type: "error".to_string(),
            summary: format!("{}: {}", body.code, body.message),
        },
    )
}

pub fn preflight(workspace: &Path) -> RainyResult<()> {
    if !workspace.join("rainy.yaml").exists() {
        return Ok(());
    }
    let audit_dir = workspace.join(".rainy");
    std::fs::create_dir_all(&audit_dir).map_err(audit_error)?;
    let path = audit_dir.join("audit.log");
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(audit_error)?;
    file.try_lock_exclusive().map_err(audit_error)?;
    FileExt::unlock(&file).map_err(audit_error)
}

fn append(workspace: &Path, record: AuditRecord) -> RainyResult<()> {
    let audit_dir = workspace.join(".rainy");
    std::fs::create_dir_all(&audit_dir).map_err(audit_error)?;
    let path = audit_dir.join("audit.log");
    let line = serde_json::to_string(&record)?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(audit_error)?;
    file.lock_exclusive().map_err(audit_error)?;
    writeln!(file, "{line}").map_err(audit_error)?;
    file.sync_data().map_err(audit_error)?;
    FileExt::unlock(&file).map_err(audit_error)
}

fn audit_error(error: std::io::Error) -> RainyError {
    RainyError::config(
        "AUDIT_WRITE_FAILED",
        format!("audit log is not writable: {error}"),
    )
}
