use crate::error::RainyError;
use crate::output::CommandOutput;
use chrono::Utc;
use serde::Serialize;
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
) -> std::io::Result<()> {
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
) -> std::io::Result<()> {
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

fn append(workspace: &Path, record: AuditRecord) -> std::io::Result<()> {
    let audit_dir = workspace.join(".rainy");
    std::fs::create_dir_all(&audit_dir)?;
    let path = audit_dir.join("audit.log");
    let line = serde_json::to_string(&record).unwrap_or_else(|_| {
        "{\"protocolVersion\":\"rainy.audit.v1\",\"status\":\"error\",\"summary\":\"audit serialization failed\"}".to_string()
    });
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(file, "{line}")
}
