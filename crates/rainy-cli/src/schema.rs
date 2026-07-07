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
    for entry in std::fs::read_dir(schema_root())? {
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
    let schema: Value = serde_json::from_str(&std::fs::read_to_string(&schema_path)?)?;
    let instance = read_value(file)?;
    let schemas = load_schemas()?;
    let mut issues = Vec::new();
    validate_value(&schema, &instance, "$", &schemas, &mut issues);
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

fn validate_value(
    schema: &Value,
    instance: &Value,
    path: &str,
    schemas: &BTreeMap<String, Value>,
    issues: &mut Vec<SchemaIssue>,
) {
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        if let Some(ref_schema) = schemas.get(reference) {
            validate_value(ref_schema, instance, path, schemas, issues);
        } else {
            issues.push(SchemaIssue {
                path: path.to_string(),
                message: format!("schema reference not found: {reference}"),
            });
        }
        return;
    }

    if let Some(const_value) = schema.get("const")
        && instance != const_value
    {
        issues.push(SchemaIssue {
            path: path.to_string(),
            message: format!("expected constant {}", display_value(const_value)),
        });
    }

    if let Some(enum_values) = schema.get("enum").and_then(Value::as_array)
        && !enum_values.iter().any(|value| value == instance)
    {
        issues.push(SchemaIssue {
            path: path.to_string(),
            message: format!("value {} is not in enum", display_value(instance)),
        });
    }

    if let Some(schema_type) = schema.get("type")
        && !type_matches(schema_type, instance)
    {
        issues.push(SchemaIssue {
            path: path.to_string(),
            message: format!("type mismatch, expected {}", display_value(schema_type)),
        });
        return;
    }

    if let Some(pattern) = schema.get("pattern").and_then(Value::as_str)
        && let Some(text) = instance.as_str()
        && !matches_pattern(pattern, text)
    {
        issues.push(SchemaIssue {
            path: path.to_string(),
            message: format!("value does not match pattern {pattern}"),
        });
    }

    if let Some(min_length) = schema.get("minLength").and_then(Value::as_u64)
        && let Some(text) = instance.as_str()
        && text.chars().count() < min_length as usize
    {
        issues.push(SchemaIssue {
            path: path.to_string(),
            message: format!("string length is less than minLength {min_length}"),
        });
    }

    if let Some(required) = schema.get("required").and_then(Value::as_array)
        && let Some(object) = instance.as_object()
    {
        for key in required.iter().filter_map(Value::as_str) {
            if !object.contains_key(key) {
                issues.push(SchemaIssue {
                    path: path.to_string(),
                    message: format!("missing required property {key}"),
                });
            }
        }
    }

    if let (Some(properties), Some(object)) = (
        schema.get("properties").and_then(Value::as_object),
        instance.as_object(),
    ) {
        for (key, property_schema) in properties {
            if let Some(value) = object.get(key) {
                validate_value(
                    property_schema,
                    value,
                    &format!("{path}.{key}"),
                    schemas,
                    issues,
                );
            }
        }
        if schema
            .get("additionalProperties")
            .and_then(Value::as_bool)
            .is_some_and(|value| !value)
        {
            for key in object.keys() {
                if !properties.contains_key(key) {
                    issues.push(SchemaIssue {
                        path: format!("{path}.{key}"),
                        message: "additional property is not allowed".to_string(),
                    });
                }
            }
        }
    }

    if let (Some(item_schema), Some(items)) = (schema.get("items"), instance.as_array()) {
        for (index, item) in items.iter().enumerate() {
            validate_value(
                item_schema,
                item,
                &format!("{path}[{index}]"),
                schemas,
                issues,
            );
        }
    }
}

fn type_matches(schema_type: &Value, instance: &Value) -> bool {
    match schema_type {
        Value::String(kind) => single_type_matches(kind, instance),
        Value::Array(kinds) => kinds
            .iter()
            .filter_map(Value::as_str)
            .any(|kind| single_type_matches(kind, instance)),
        _ => true,
    }
}

fn single_type_matches(kind: &str, instance: &Value) -> bool {
    match kind {
        "object" => instance.is_object(),
        "array" => instance.is_array(),
        "string" => instance.is_string(),
        "number" => instance.is_number(),
        "integer" => instance.as_i64().is_some() || instance.as_u64().is_some(),
        "boolean" => instance.is_boolean(),
        "null" => instance.is_null(),
        _ => true,
    }
}

fn matches_pattern(pattern: &str, text: &str) -> bool {
    if pattern == "^[a-f0-9]{64}$" {
        return text.len() == 64
            && text
                .chars()
                .all(|ch| ch.is_ascii_digit() || ('a'..='f').contains(&ch));
    }
    true
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
    for entry in std::fs::read_dir(schema_root())? {
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
    let path = schema_root().join(&filename);
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

fn schema_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crate is inside workspace")
        .join("schemas")
}

fn display_value(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "<value>".to_string())
}
