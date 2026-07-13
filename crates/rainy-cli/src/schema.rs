use crate::cli::{SchemaCommand, SchemaSubcommand};
use crate::error::{RainyError, RainyResult};
use crate::output::CommandOutput;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
pub struct SchemaInfo {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SchemaValidationReport {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    pub status: String,
    pub schema: String,
    pub file: String,
    pub issues: Vec<SchemaIssue>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SchemaIssue {
    pub path: String,
    pub message: String,
}

pub fn handle_schema_command(command: SchemaCommand) -> RainyResult<CommandOutput> {
    match command.command {
        SchemaSubcommand::List => Ok(CommandOutput::Schemas {
            schemas: list_schemas()?,
        }),
        SchemaSubcommand::Validate(args) => {
            let report = validate_file(&args.schema, &args.file)?;
            if report.status == "failed" {
                return Err(RainyError::config(
                    "SCHEMA_VALIDATION_FAILED",
                    serde_json::to_string(&report)?,
                ));
            }
            Ok(CommandOutput::SchemaValidation { report })
        }
    }
}

fn list_schemas() -> RainyResult<Vec<SchemaInfo>> {
    let mut schemas = Vec::new();
    for entry in std::fs::read_dir(schema_root()?)? {
        let entry = entry?;
        if entry.file_type()?.is_file()
            && entry
                .path()
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext == "json")
        {
            let name = schema_name(&entry.path());
            schemas.push(SchemaInfo {
                name,
                path: entry.path().to_string_lossy().to_string(),
            });
        }
    }
    schemas.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(schemas)
}

fn validate_file(schema_name: &str, file: &Path) -> RainyResult<SchemaValidationReport> {
    let schema_path = schema_path(schema_name)?;
    let mut schema: Value = serde_json::from_str(&std::fs::read_to_string(&schema_path)?)?;
    let instance = read_value(file)?;
    let mut schemas = load_schemas()?;
    for schema in schemas.values_mut() {
        rewrite_external_refs(schema);
    }
    rewrite_external_refs(&mut schema);
    schema
        .as_object_mut()
        .ok_or_else(|| RainyError::config("SCHEMA_INVALID", "schema root must be an object"))?
        .insert(
            "$defs".to_string(),
            Value::Object(schemas.into_iter().collect()),
        );
    let validator = jsonschema::validator_for(&schema).map_err(|err| {
        RainyError::config(
            "SCHEMA_INVALID",
            format!("schema compilation failed: {err}"),
        )
    })?;
    let issues = validator
        .iter_errors(&instance)
        .map(|error| SchemaIssue {
            path: if error.instance_path.as_str().is_empty() {
                "$".to_string()
            } else {
                format!("${}", error.instance_path)
            },
            message: error.to_string(),
        })
        .collect::<Vec<_>>();
    Ok(SchemaValidationReport {
        protocol_version: "rainy.schema-validation.v1".to_string(),
        status: if issues.is_empty() {
            "passed"
        } else {
            "failed"
        }
        .to_string(),
        schema: schema_name.to_string(),
        file: file.to_string_lossy().to_string(),
        issues,
    })
}

fn rewrite_external_refs(value: &mut Value) {
    match value {
        Value::Object(object) => {
            if let Some(reference) = object.get_mut("$ref")
                && let Some(raw) = reference.as_str()
                && !raw.starts_with('#')
                && !raw.contains("://")
            {
                *reference = Value::String(format!("#/$defs/{raw}"));
            }
            for nested in object.values_mut() {
                rewrite_external_refs(nested);
            }
        }
        Value::Array(values) => {
            for nested in values {
                rewrite_external_refs(nested);
            }
        }
        _ => {}
    }
}

fn read_value(file: &Path) -> RainyResult<Value> {
    let content = std::fs::read_to_string(file)?;
    if file
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext == "json")
    {
        Ok(serde_json::from_str(&content)?)
    } else {
        let yaml: serde_yaml::Value = serde_yaml::from_str(&content)?;
        Ok(serde_json::to_value(yaml)?)
    }
}

fn load_schemas() -> RainyResult<BTreeMap<String, Value>> {
    let mut schemas = BTreeMap::new();
    for entry in std::fs::read_dir(schema_root()?)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let path = entry.path();
        if path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_none_or(|ext| ext != "json")
        {
            continue;
        }
        schemas.insert(
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_string(),
            serde_json::from_str(&std::fs::read_to_string(&path)?)?,
        );
    }
    Ok(schemas)
}

fn schema_path(name: &str) -> RainyResult<PathBuf> {
    let filename = if name.ends_with(".schema.json") || name.ends_with(".json") {
        name.to_string()
    } else {
        format!("{name}.schema.json")
    };
    let path = schema_root()?.join(&filename);
    if !path.exists() {
        return Err(RainyError::config(
            "SCHEMA_NOT_FOUND",
            format!("schema not found: {name}"),
        ));
    }
    Ok(path)
}

fn schema_name(path: &Path) -> String {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    file_name
        .strip_suffix(".schema.json")
        .or_else(|| file_name.strip_suffix(".json"))
        .unwrap_or(file_name)
        .to_string()
}

fn schema_root() -> RainyResult<PathBuf> {
    crate::bundled_assets::schema_path()
}
