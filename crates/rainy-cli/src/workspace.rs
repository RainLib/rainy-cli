use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy)]
pub enum WorkspaceMarker {
    None,
    Project,
    Skills,
    Either,
}

pub fn resolve(explicit: Option<PathBuf>, marker: WorkspaceMarker) -> std::io::Result<PathBuf> {
    if let Some(path) = explicit {
        return absolutize(path);
    }
    let start = std::env::current_dir()?;
    if matches!(marker, WorkspaceMarker::None) {
        return Ok(start);
    }
    Ok(discover(&start, marker).unwrap_or(start))
}

fn discover(start: &Path, marker: WorkspaceMarker) -> Option<PathBuf> {
    let mut current = Some(start);
    while let Some(directory) = current {
        if has_marker(directory, marker) {
            return Some(directory.to_path_buf());
        }
        if directory.join(".git").exists() {
            break;
        }
        current = directory.parent();
    }
    None
}

fn has_marker(directory: &Path, marker: WorkspaceMarker) -> bool {
    let project = directory.join("rainy.yaml").is_file();
    let skills = directory.join("rainy-skills.yaml").is_file();
    match marker {
        WorkspaceMarker::None => false,
        WorkspaceMarker::Project => project,
        WorkspaceMarker::Skills => skills,
        WorkspaceMarker::Either => project || skills,
    }
}

fn absolutize(path: PathBuf) -> std::io::Result<PathBuf> {
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_nearest_marker_but_does_not_cross_git_root() {
        let temp = tempfile::tempdir().expect("tempdir");
        let project = temp.path().join("project");
        let nested = project.join("apps/backend");
        std::fs::create_dir_all(&nested).expect("nested");
        std::fs::write(project.join("rainy.yaml"), "kind: Project\n").expect("marker");
        assert_eq!(discover(&nested, WorkspaceMarker::Project), Some(project));

        let repository = temp.path().join("repository");
        let nested = repository.join("nested");
        std::fs::create_dir_all(repository.join(".git")).expect("git marker");
        std::fs::create_dir_all(&nested).expect("nested repository");
        std::fs::write(temp.path().join("rainy.yaml"), "kind: Project\n").expect("parent marker");
        assert_eq!(discover(&nested, WorkspaceMarker::Project), None);
    }
}
