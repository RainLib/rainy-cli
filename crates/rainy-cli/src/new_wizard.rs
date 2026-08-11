use crate::error::{RainyError, RainyResult};
use crate::progress::ProgressReporter;
use inquire::{InquireError, Select};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub enum NewProjectSelection {
    GoldenPath { id: String },
    Source { name: String },
    ProjectTemplate { id: String, catalog_path: PathBuf },
}

enum ProjectKind {
    GoldenPath,
    Source,
    ProjectTemplate,
}

struct WizardTemplate {
    id: String,
    description: Option<String>,
    catalog_path: PathBuf,
    origin: String,
}

pub fn select_new_project(
    base_dir: &Path,
    no_color: bool,
    progress: &ProgressReporter,
) -> RainyResult<NewProjectSelection> {
    let sources = crate::source::available_project_sources()?;
    let mut template_catalogs = Vec::new();
    if let Some(catalog) = crate::project_template::discover_template_catalog(base_dir)? {
        template_catalogs.push(("local catalog".to_string(), catalog));
    }
    let mut catalog_paths = template_catalogs
        .iter()
        .map(|(_, catalog)| catalog.path.clone())
        .collect::<BTreeSet<_>>();
    for cached in crate::source::available_project_template_catalogs()? {
        if catalog_paths.insert(cached.path.clone()) {
            template_catalogs.push((
                format!("{}@{}", cached.source_name, cached.source_version),
                crate::project_template::inspect_template_catalog(cached.path)?,
            ));
        }
    }
    let templates = template_catalogs
        .iter()
        .flat_map(|(origin, catalog)| {
            catalog.templates.iter().map(|template| WizardTemplate {
                id: template.id.clone(),
                description: template.description.clone(),
                catalog_path: catalog.path.clone(),
                origin: origin.clone(),
            })
        })
        .collect::<Vec<_>>();
    let _suspension = progress.suspend();

    loop {
        let mut labels =
            vec!["Built-in Golden Path  Spring Boot + Next.js SaaS workspace".to_string()];
        let mut kinds = vec![ProjectKind::GoldenPath];
        if !sources.is_empty() {
            labels.push(format!(
                "Managed Rainy Source  {} verified cached source(s)",
                sources.len()
            ));
            kinds.push(ProjectKind::Source);
        }
        if !templates.is_empty() {
            labels.push(format!(
                "Enterprise Git templates  {} available across {} catalog(s)",
                templates.len(),
                template_catalogs.len()
            ));
            kinds.push(ProjectKind::ProjectTemplate);
        }

        let selected = prompt_select(
            "Select the project creation workflow",
            labels.clone(),
            0,
            no_color,
            "Type to search; Up/Down move; Enter confirms; Esc cancels",
        )?;
        let index = labels
            .iter()
            .position(|label| label == &selected)
            .unwrap_or(0);
        match kinds[index] {
            ProjectKind::GoldenPath => {
                return Ok(NewProjectSelection::GoldenPath {
                    id: "spring-nextjs-saas".to_string(),
                });
            }
            ProjectKind::Source => {
                if sources.len() == 1 {
                    return Ok(NewProjectSelection::Source {
                        name: sources[0].name.clone(),
                    });
                }
                let mut source_labels = vec!["Back to creation workflows".to_string()];
                source_labels.extend(sources.iter().map(|source| {
                    format!(
                        "{}  v{}{}",
                        source.name,
                        source.version,
                        source
                            .description
                            .as_deref()
                            .filter(|description| !description.trim().is_empty())
                            .map(|description| format!("  {description}"))
                            .unwrap_or_default()
                    )
                }));
                let selected = prompt_select(
                    "Select a managed Rainy Source",
                    source_labels.clone(),
                    1,
                    no_color,
                    "Type to search; Up/Down move; Enter confirms; Esc cancels",
                )?;
                let index = source_labels
                    .iter()
                    .position(|label| label == &selected)
                    .unwrap_or(0);
                if index == 0 {
                    continue;
                }
                return Ok(NewProjectSelection::Source {
                    name: sources[index - 1].name.clone(),
                });
            }
            ProjectKind::ProjectTemplate => {
                if templates.len() == 1 {
                    return Ok(NewProjectSelection::ProjectTemplate {
                        id: templates[0].id.clone(),
                        catalog_path: templates[0].catalog_path.clone(),
                    });
                }
                let mut template_labels = vec!["Back to creation workflows".to_string()];
                template_labels.extend(templates.iter().map(|template| {
                    format!(
                        "{}  [{}]{}",
                        template.id,
                        template.origin,
                        template
                            .description
                            .as_deref()
                            .filter(|description| !description.trim().is_empty())
                            .map(|description| format!("  {description}"))
                            .unwrap_or_default()
                    )
                }));
                let selected = prompt_select(
                    "Select an enterprise project template",
                    template_labels.clone(),
                    1,
                    no_color,
                    "Type to search; Up/Down move; Enter confirms; Esc cancels",
                )?;
                let index = template_labels
                    .iter()
                    .position(|label| label == &selected)
                    .unwrap_or(0);
                if index == 0 {
                    continue;
                }
                return Ok(NewProjectSelection::ProjectTemplate {
                    id: templates[index - 1].id.clone(),
                    catalog_path: templates[index - 1].catalog_path.clone(),
                });
            }
        }
    }
}

fn prompt_select(
    message: &str,
    labels: Vec<String>,
    starting_cursor: usize,
    no_color: bool,
    help: &str,
) -> RainyResult<String> {
    let prompt = Select::new(message, labels)
        .with_starting_cursor(starting_cursor)
        .with_help_message(help);
    if no_color {
        prompt
            .with_render_config(inquire::ui::RenderConfig::empty())
            .prompt()
    } else {
        prompt.prompt()
    }
    .map_err(prompt_error)
}

fn prompt_error(error: InquireError) -> RainyError {
    RainyError::action(
        "CANCELLED",
        match error {
            InquireError::OperationCanceled | InquireError::OperationInterrupted => {
                "Project creation selection cancelled".to_string()
            }
            InquireError::IO(io) if io.kind() == std::io::ErrorKind::UnexpectedEof => {
                "Project creation selection cancelled because input ended".to_string()
            }
            other => format!("Project creation selection failed: {other}"),
        },
    )
}
