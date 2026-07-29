use crate::{
    catalog::{entry_by_id, template_path},
    command_runner::{capture, CommandSpec, Executable},
    error::{AppError, AppResult},
    models::{PrepareDeploymentRequest, ProjectSummary},
    persistence::Database,
};
use chrono::Utc;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use uuid::Uuid;

fn validate_environment_name(value: &str) -> AppResult<()> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_".contains(character))
    {
        return Err(AppError::Invalid(
            "Environment name may contain letters, numbers, hyphens, and underscores.".into(),
        ));
    }
    Ok(())
}

fn validate_location(value: &str) -> AppResult<()> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
    {
        return Err(AppError::Invalid("Invalid Azure location.".into()));
    }
    Ok(())
}

fn validate_template_id(value: &str) -> AppResult<()> {
    if value.is_empty()
        || value.len() > 128
        || value.contains("..")
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "._-".contains(character))
    {
        return Err(AppError::Invalid("Invalid template ID.".into()));
    }
    Ok(())
}

async fn run_checked(spec: CommandSpec, label: &str, timeout: Duration) -> AppResult<String> {
    let output = capture(spec, timeout).await?;
    if !output.success {
        let detail = if label == "Applying environment value" {
            "azd rejected an environment value; the value was withheld from diagnostics."
                .to_string()
        } else {
            output.stderr.trim().to_string()
        };
        return Err(AppError::Command(format!("{label} failed: {detail}")));
    }
    Ok(output.stdout)
}

async fn cache_pinned_template(
    cache_root: &Path,
    repository: &str,
    commit: &str,
    template_id: &str,
) -> AppResult<PathBuf> {
    let template_root = cache_root.join(template_id).join(commit);
    if template_root.join(".git").is_dir() {
        let actual = run_checked(
            CommandSpec::new(Executable::Git, ["rev-parse", "HEAD"]).cwd(&template_root),
            "Verifying cached template",
            Duration::from_secs(15),
        )
        .await?;
        if actual.trim().eq_ignore_ascii_case(commit) {
            return Ok(template_root);
        }
        return Err(AppError::Invalid(
            "Cached template revision does not match its catalog pin.".into(),
        ));
    }

    let parent = template_root
        .parent()
        .ok_or_else(|| AppError::Invalid("Invalid template cache path.".into()))?;
    std::fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(".download-{}", Uuid::new_v4()));
    std::fs::create_dir(&temporary)?;
    let result: AppResult<()> = async {
        run_checked(
            CommandSpec::new(Executable::Git, ["init", "--quiet"]).cwd(&temporary),
            "Initializing template cache",
            Duration::from_secs(20),
        )
        .await?;
        run_checked(
            CommandSpec::new(Executable::Git, ["remote", "add", "origin", repository])
                .cwd(&temporary),
            "Configuring template source",
            Duration::from_secs(20),
        )
        .await?;
        run_checked(
            CommandSpec::new(
                Executable::Git,
                ["fetch", "--quiet", "--depth", "1", "origin", commit],
            )
            .cwd(&temporary),
            "Fetching pinned template",
            Duration::from_secs(180),
        )
        .await?;
        run_checked(
            CommandSpec::new(
                Executable::Git,
                ["checkout", "--quiet", "--detach", "FETCH_HEAD"],
            )
            .cwd(&temporary),
            "Checking out pinned template",
            Duration::from_secs(60),
        )
        .await?;
        let actual = run_checked(
            CommandSpec::new(Executable::Git, ["rev-parse", "HEAD"]).cwd(&temporary),
            "Verifying pinned template",
            Duration::from_secs(15),
        )
        .await?;
        if !actual.trim().eq_ignore_ascii_case(commit) {
            return Err(AppError::Invalid(format!(
                "Repository returned {} instead of pinned commit {commit}.",
                actual.trim()
            )));
        }
        Ok(())
    }
    .await;
    if let Err(error) = result {
        let _ = std::fs::remove_dir_all(&temporary);
        return Err(error);
    }
    std::fs::rename(&temporary, &template_root)?;
    Ok(template_root)
}

fn ensure_destination(path: &str, allow_existing_project: bool) -> AppResult<PathBuf> {
    let destination = PathBuf::from(path);
    if !destination.is_absolute() {
        return Err(AppError::Invalid(
            "Project destination must be an absolute path.".into(),
        ));
    }
    if destination.exists() {
        if !destination.is_dir() {
            return Err(AppError::Invalid(
                "Project destination must be a folder.".into(),
            ));
        }
        if allow_existing_project && destination.join("azure.yaml").is_file() {
            return Ok(destination);
        }
        let mut entries = std::fs::read_dir(&destination)?;
        if entries.next().is_some() {
            return Err(AppError::Invalid(
                "New project destination must be empty.".into(),
            ));
        }
    } else {
        std::fs::create_dir_all(&destination)?;
    }
    Ok(destination)
}

pub async fn prepare(
    request: PrepareDeploymentRequest,
    cache_root: &Path,
    database: &Database,
) -> AppResult<ProjectSummary> {
    validate_template_id(&request.template_id)?;
    validate_environment_name(&request.environment)?;
    validate_location(&request.location)?;
    let existing_project = request.source_mode == "existingProject";
    let destination = ensure_destination(&request.destination, existing_project)?;

    let entry = match request.source_mode.as_str() {
        "catalog" => Some(entry_by_id(&request.template_id)?),
        "externalCatalog" => Some(
            database
                .accepted_catalog_entry(&request.template_id)?
                .ok_or_else(|| {
                    AppError::Invalid(
                        "External template is not present in an accepted catalog snapshot.".into(),
                    )
                })?,
        ),
        _ => None,
    };

    if !existing_project {
        let source = match request.source_mode.as_str() {
            "catalog" | "externalCatalog" => {
                let entry = entry
                    .as_ref()
                    .ok_or_else(|| AppError::Invalid("Missing catalog entry.".into()))?;
                let cached = cache_pinned_template(
                    cache_root,
                    &entry.repository,
                    &entry.commit,
                    &entry.id,
                )
                .await?;
                template_path(&cached, entry)?
            }
            "localTemplate" => {
                let source = request
                    .source_path
                    .as_deref()
                    .ok_or_else(|| AppError::Invalid("Local template path is required.".into()))?;
                let source = std::fs::canonicalize(source)?;
                if !source.join("azure.yaml").is_file() {
                    return Err(AppError::Invalid(
                        "Local template does not contain azure.yaml.".into(),
                    ));
                }
                source
            }
            "github" => {
                let repository = request
                    .source_path
                    .as_deref()
                    .ok_or_else(|| AppError::Invalid("GitHub repository URL is required.".into()))?;
                let commit = request
                    .source_revision
                    .as_deref()
                    .ok_or_else(|| AppError::Invalid("Pinned GitHub revision is required.".into()))?;
                let parsed = url::Url::parse(repository)
                    .map_err(|error| AppError::Url(error.to_string()))?;
                if parsed.scheme() != "https" || parsed.host_str() != Some("github.com") {
                    return Err(AppError::Url(
                        "Direct repositories must use an HTTPS GitHub URL.".into(),
                    ));
                }
                if commit.len() != 40
                    || !commit
                        .chars()
                        .all(|character| character.is_ascii_hexdigit())
                {
                    return Err(AppError::Invalid("Invalid pinned GitHub revision.".into()));
                }
                let cached =
                    cache_pinned_template(cache_root, repository, commit, &request.template_id)
                        .await?;
                template_path(
                    &cached,
                    &crate::models::CatalogEntry {
                        id: request.template_id.clone(),
                        title: request.template_id.clone(),
                        summary: String::new(),
                        description: String::new(),
                        repository: repository.into(),
                        commit: commit.into(),
                        subdirectory: None,
                        tags: vec![],
                        platforms: vec![],
                        manifest: None,
                        source: Some("external".into()),
                        trusted: false,
                    },
                )?
            }
            _ => {
                return Err(AppError::Invalid(
                    "Only catalog, local template, and existing project sources are materialized in V1."
                        .into(),
                ))
            }
        };
        let mut args = vec![
            "init".into(),
            "--template".into(),
            source.display().to_string(),
            "--environment".into(),
            request.environment.clone(),
            "--location".into(),
            request.location.clone(),
            "--no-prompt".into(),
        ];
        if let Some(subscription) = request
            .subscription
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            args.extend(["--subscription".into(), subscription.into()]);
        }
        run_checked(
            CommandSpec::new(Executable::Azd, args).cwd(&destination),
            "Initializing azd project",
            Duration::from_secs(180),
        )
        .await?;
    } else if !destination.join("azure.yaml").is_file() {
        return Err(AppError::Invalid(
            "Existing project does not contain azure.yaml.".into(),
        ));
    }

    if existing_project && !destination.join(".azure").is_dir() {
        let mut args = vec![
            "env".into(),
            "new".into(),
            request.environment.clone(),
            "--location".into(),
            request.location.clone(),
            "--no-prompt".into(),
        ];
        if let Some(subscription) = request
            .subscription
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            args.extend(["--subscription".into(), subscription.into()]);
        }
        run_checked(
            CommandSpec::new(Executable::Azd, args).cwd(&destination),
            "Creating azd environment",
            Duration::from_secs(60),
        )
        .await?;
    }

    for (environment_variable, value) in &request.environment_values {
        let mut characters = environment_variable.chars();
        if environment_variable.is_empty()
            || environment_variable.len() > 128
            || !characters
                .next()
                .is_some_and(|character| character.is_ascii_uppercase())
            || !characters.all(|character| {
                character.is_ascii_uppercase() || character.is_ascii_digit() || character == '_'
            })
        {
            return Err(AppError::Invalid(format!(
                "Invalid environment variable name: {environment_variable}"
            )));
        }
        if value.is_null() {
            continue;
        }
        let value = match value {
            serde_json::Value::String(value) => value.clone(),
            serde_json::Value::Bool(value) => value.to_string(),
            serde_json::Value::Array(values) => values
                .iter()
                .filter_map(serde_json::Value::as_str)
                .collect::<Vec<_>>()
                .join(","),
            _ => continue,
        };
        if value.len() > 32_768 {
            return Err(AppError::Invalid(format!(
                "Environment value is too large: {environment_variable}"
            )));
        }
        run_checked(
            CommandSpec::new(
                Executable::Azd,
                [
                    "env",
                    "set",
                    environment_variable,
                    &value,
                    "--environment",
                    &request.environment,
                    "--no-prompt",
                ],
            )
            .cwd(&destination),
            "Applying environment value",
            Duration::from_secs(30),
        )
        .await?;
    }

    let name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("azd-project")
        .to_string();
    let project = ProjectSummary {
        id: format!("{}-{}", name, request.environment),
        name,
        path: destination.display().to_string(),
        environment: Some(request.environment),
        template_id: Some(request.template_id),
        revision: entry.as_ref().map(|entry| entry.commit.clone()),
        last_operation: Some(Utc::now().to_rfc3339()),
        state: "ready".into(),
    };
    database.upsert_project(&project)?;
    Ok(project)
}
