use crate::cli::EvidenceFormat;
use crate::config;
use crate::doctor;
use crate::error::RainyResult;
use crate::output::CommandOutput;
use crate::verify;
use serde::Serialize;
use serde_json::Value;
use std::path::Path;

const REDACTED: &str = "[REDACTED]";
const SENSITIVE_FIELDS: &[&str] = &[
    "password",
    "secret",
    "token",
    "accesskey",
    "secretkey",
    "privatekey",
    "authorization",
    "cookie",
];

#[derive(Debug, Serialize)]
struct EvidenceReport {
    #[serde(rename = "protocolVersion")]
    protocol_version: String,
    summary: String,
    status: String,
    project: String,
    capabilities: Vec<String>,
    changes: Vec<EvidenceChange>,
    doctor: doctor::DoctorReport,
    verify: verify::VerifyReport,
    risks: Vec<String>,
}

#[derive(Debug, Serialize)]
struct EvidenceChange {
    capability: String,
    description: String,
    artifacts: Vec<String>,
}

pub fn generate_command(workspace: &Path, format: EvidenceFormat) -> RainyResult<CommandOutput> {
    let config = config::load_config(workspace)?;
    let lock = config::load_lock(workspace)?;
    let evidence_path = config.paths.evidence.clone();
    let evidence_dir = workspace.join(&evidence_path);
    std::fs::create_dir_all(&evidence_dir)?;
    let doctor = doctor::run_doctor(workspace, None)?;
    let verify = verify::run_verify(workspace, "local", None)?;
    let capabilities = lock.capabilities.keys().cloned().collect::<Vec<_>>();
    let changes = lock
        .capabilities
        .iter()
        .map(|(id, capability)| EvidenceChange {
            capability: id.clone(),
            description: format!(
                "Installed {} {} from {}",
                id, capability.version, capability.pack
            ),
            artifacts: capability.artifacts.clone(),
        })
        .collect::<Vec<_>>();
    let status = aggregate_status(&doctor.status, &verify.status);
    let risks = risks(&config, &capabilities, &doctor, &verify);
    let report = EvidenceReport {
        protocol_version: "rainy.evidence.v1".to_string(),
        summary: format!("Evidence for Rainy project {}", config.project.name),
        status,
        project: config.project.name,
        capabilities,
        changes,
        doctor,
        verify,
        risks,
    };
    let mut files = Vec::new();

    if matches!(format, EvidenceFormat::Markdown | EvidenceFormat::All) {
        let path = evidence_dir.join("report.md");
        std::fs::write(&path, markdown(&report))?;
        files.push(format!("{evidence_path}/report.md"));
    }
    if matches!(format, EvidenceFormat::Json | EvidenceFormat::All) {
        let path = evidence_dir.join("report.json");
        let mut value = serde_json::to_value(&report)?;
        redact_json_value(&mut value);
        std::fs::write(
            &path,
            format!("{}\n", serde_json::to_string_pretty(&value)?),
        )?;
        files.push(format!("{evidence_path}/report.json"));
    }

    Ok(CommandOutput::Evidence {
        status: "generated",
        files,
    })
}

fn markdown(report: &EvidenceReport) -> String {
    let mut out = String::new();
    out.push_str("# Rainy Evidence Report\n\n");
    out.push_str("## Summary\n\n");
    out.push_str(&format!("- Status: {}\n", report.status));
    out.push_str(&format!("- Project: {}\n", report.project));
    out.push_str(&format!(
        "- Capabilities: {}\n\n",
        report.capabilities.join(", ")
    ));
    out.push_str("## Changes\n\n");
    for change in &report.changes {
        out.push_str(&format!("- {}\n", change.description));
        if !change.artifacts.is_empty() {
            out.push_str(&format!(
                "  Artifacts: {}\n",
                redact_text(&change.artifacts.join(", "))
            ));
        }
    }
    out.push_str("## Doctor\n\n");
    out.push_str(&format!("Status: {}\n\n", report.doctor.status));
    for check in &report.doctor.checks {
        out.push_str(&format!(
            "- {} `{}`: {}\n",
            check.status,
            check.id,
            redact_text(&check.message)
        ));
    }
    out.push_str("\n## Verify\n\n");
    out.push_str(&format!("Status: {}\n\n", report.verify.status));
    for check in &report.verify.checks {
        out.push_str(&format!(
            "- {} `{}`: {}\n",
            check.status,
            check.id,
            redact_text(&check.message)
        ));
    }
    out.push_str("\n## Risks\n\n");
    for risk in &report.risks {
        out.push_str(&format!("- {}\n", redact_text(risk)));
    }
    out
}

fn aggregate_status(doctor: &str, verify: &str) -> String {
    if doctor == "failed" || verify == "failed" {
        "failed"
    } else if doctor == "warning" || verify == "warning" {
        "warning"
    } else {
        "passed"
    }
    .to_string()
}

fn risks(
    config: &config::ProjectConfig,
    capabilities: &[String],
    doctor: &doctor::DoctorReport,
    verify: &verify::VerifyReport,
) -> Vec<String> {
    let mut risks = Vec::new();
    if capabilities
        .iter()
        .any(|capability| capability == "minio-file-storage")
    {
        risks.push(
            "MinIO credentials are development defaults; production secrets must be configured externally."
                .to_string(),
        );
    }
    if doctor.checks.iter().any(|check| check.status == "warning")
        || verify.checks.iter().any(|check| check.status == "warning")
    {
        risks.push("Some checks produced warnings; review skipped tools or environment gaps before merging.".to_string());
    }
    if !config.policy.require_approval.is_empty() {
        risks.push(format!(
            "Operations requiring approval remain gated: {}.",
            config.policy.require_approval.join(", ")
        ));
    }
    if risks.is_empty() {
        risks.push("No known risks detected by Rainy checks.".to_string());
    }
    risks
}

fn redact_json_value(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                if is_sensitive_key(key) {
                    *child = Value::String(REDACTED.to_string());
                } else {
                    redact_json_value(child);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                redact_json_value(item);
            }
        }
        Value::String(text) => {
            *text = redact_text(text);
        }
        _ => {}
    }
}

fn redact_text(text: &str) -> String {
    text.lines().map(redact_line).collect::<Vec<_>>().join("\n")
}

fn redact_line(line: &str) -> String {
    let Some((index, separator)) = first_secret_separator(line) else {
        return line.to_string();
    };
    let key = &line[..index];
    if !is_sensitive_key(key) {
        return line.to_string();
    }
    format!("{}{} {}", key.trim_end(), separator, REDACTED)
}

fn first_secret_separator(line: &str) -> Option<(usize, char)> {
    let equals = line.find('=');
    let colon = line.find(':');
    match (equals, colon) {
        (Some(left), Some(right)) if left < right => Some((left, '=')),
        (Some(_), Some(right)) => Some((right, ':')),
        (Some(index), None) => Some((index, '=')),
        (None, Some(index)) => Some((index, ':')),
        (None, None) => None,
    }
}

fn is_sensitive_key(key: &str) -> bool {
    let normalized = key
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(|ch| ch.to_lowercase())
        .collect::<String>();
    SENSITIVE_FIELDS
        .iter()
        .any(|field| normalized.contains(field))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_sensitive_json_keys_and_string_values() {
        let mut value = serde_json::json!({
            "authorization": "Bearer abc",
            "nested": {
                "stdout": "token=abc123\nstatus=ok",
                "cookie": "session=secret"
            },
            "safe": "production secrets must be configured externally"
        });

        redact_json_value(&mut value);

        assert_eq!(value["authorization"], REDACTED);
        assert_eq!(value["nested"]["cookie"], REDACTED);
        assert_eq!(value["nested"]["stdout"], "token= [REDACTED]\nstatus=ok");
        assert_eq!(
            value["safe"],
            "production secrets must be configured externally"
        );
    }
}
