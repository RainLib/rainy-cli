use crate::error::RainyResult;
use serde::{Deserialize, Serialize};
use similar::{ChangeTag, TextDiff};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeSet {
    pub changes: Vec<Change>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Change {
    pub kind: ChangeKind,
    pub path: String,
    pub before: Option<String>,
    pub after: Option<String>,
    pub summary: String,
    #[serde(default)]
    pub noop: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChangeKind {
    #[serde(rename = "create-file")]
    Create,
    #[serde(rename = "modify-file")]
    Modify,
    #[serde(rename = "delete-file")]
    Delete,
}

impl ChangeSet {
    pub fn new() -> Self {
        Self {
            changes: Vec::new(),
        }
    }

    pub fn push(&mut self, change: Change) {
        self.changes.push(change);
    }

    pub fn extend(&mut self, other: ChangeSet) {
        self.changes.extend(other.changes);
    }

    pub fn changed_files(&self) -> Vec<String> {
        self.changes
            .iter()
            .filter(|change| !change.noop)
            .map(|change| change.path.clone())
            .collect()
    }
}

pub fn change_for_file(
    workspace: &Path,
    rel_path: impl Into<String>,
    after: String,
    summary: impl Into<String>,
) -> RainyResult<Change> {
    let rel_path = rel_path.into();
    let abs = workspace.join(&rel_path);
    let before = if abs.exists() {
        Some(std::fs::read_to_string(&abs)?)
    } else {
        None
    };
    let noop = before.as_deref() == Some(after.as_str());
    let kind = if before.is_some() {
        ChangeKind::Modify
    } else {
        ChangeKind::Create
    };
    Ok(Change {
        kind,
        path: rel_path,
        before,
        after: Some(after),
        summary: summary.into(),
        noop,
    })
}

pub fn delete_file(workspace: &Path, rel_path: impl Into<String>) -> RainyResult<Change> {
    let rel_path = rel_path.into();
    let abs = workspace.join(&rel_path);
    let before = if abs.exists() && abs.is_file() {
        Some(std::fs::read_to_string(&abs)?)
    } else {
        None
    };
    Ok(Change {
        kind: ChangeKind::Delete,
        path: rel_path,
        before,
        after: None,
        summary: "delete file".to_string(),
        noop: !abs.exists(),
    })
}

pub fn apply_changes(workspace: &Path, changes: &ChangeSet) -> RainyResult<()> {
    let mut applied = Vec::new();
    for change in &changes.changes {
        if change.noop {
            continue;
        }
        applied.push(AppliedChange {
            path: change.path.clone(),
            before: change.before.clone(),
        });
        if let Err(err) = apply_change(workspace, change) {
            let _ = rollback_changes(workspace, &applied);
            return Err(err);
        }
    }
    Ok(())
}

struct AppliedChange {
    path: String,
    before: Option<String>,
}

fn apply_change(workspace: &Path, change: &Change) -> RainyResult<()> {
    let target = workspace.join(&change.path);
    match change.kind {
        ChangeKind::Create | ChangeKind::Modify => {
            write_file_atomic(&target, change.after.as_deref().unwrap_or_default())
        }
        ChangeKind::Delete => {
            if target.exists() {
                fs::remove_file(target)?;
            }
            Ok(())
        }
    }
}

fn rollback_changes(workspace: &Path, applied: &[AppliedChange]) -> RainyResult<()> {
    for change in applied.iter().rev() {
        let target = workspace.join(&change.path);
        if let Some(before) = &change.before {
            write_file_atomic(&target, before)?;
        } else if target.exists() {
            fs::remove_file(target)?;
        }
    }
    Ok(())
}

fn write_file_atomic(target: &Path, content: &str) -> RainyResult<()> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp = target.with_extension("rainy-tmp");
    if let Err(err) = fs::write(&temp, content).and_then(|_| fs::rename(&temp, target)) {
        let _ = fs::remove_file(&temp);
        return Err(err.into());
    }
    Ok(())
}

pub fn render_diff(changes: &ChangeSet) -> String {
    let mut rendered = String::new();
    for change in &changes.changes {
        if change.noop {
            continue;
        }
        rendered.push_str(&format!("diff --rainy {}\n", change.path));
        let before = change.before.as_deref().unwrap_or("");
        let after = change.after.as_deref().unwrap_or("");
        let diff = TextDiff::from_lines(before, after);
        for op in diff.ops() {
            for line in diff.iter_changes(op) {
                let marker = match line.tag() {
                    ChangeTag::Delete => "-",
                    ChangeTag::Insert => "+",
                    ChangeTag::Equal => " ",
                };
                rendered.push_str(marker);
                rendered.push_str(line.value());
            }
        }
        if !rendered.ends_with('\n') {
            rendered.push('\n');
        }
    }
    rendered
}
