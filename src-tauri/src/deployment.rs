use crate::{
    azd::{AzdClient, SystemAzdClient},
    catalog::{entry_by_id, template_path},
    command_runner::{capture, CommandSpec, Executable},
    error::{AppError, AppResult},
    models::{PrepareDeploymentRequest, ProjectEnvironment, ProjectSummary},
    persistence::Database,
};
use chrono::Utc;
use std::{
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
    time::Duration,
};
use uuid::Uuid;

pub(crate) fn validate_environment_name(value: &str) -> AppResult<()> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-()_.".contains(character))
    {
        return Err(AppError::Invalid(
            "Environment name may contain letters, numbers, hyphens, underscores, periods, and parentheses."
                .into(),
        ));
    }
    Ok(())
}

fn validate_source_trust(request: &PrepareDeploymentRequest) -> AppResult<()> {
    if matches!(
        request.source_mode.as_str(),
        "github" | "localTemplate" | "existingProject"
    ) && !request.source_trust_accepted
    {
        return Err(AppError::Invalid(
            "Unvetted sources require explicit trust before materialization.".into(),
        ));
    }
    Ok(())
}

fn environment_exists(environments: &[ProjectEnvironment], name: &str) -> bool {
    environments
        .iter()
        .any(|environment| environment.name == name)
}

pub(crate) fn validate_location(value: &str) -> AppResult<()> {
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

pub(crate) fn validate_optional_guid(label: &str, value: Option<&str>) -> AppResult<()> {
    let Some(value) = value.filter(|value| !value.is_empty()) else {
        return Ok(());
    };
    let parsed = Uuid::parse_str(value)
        .map_err(|_| AppError::Invalid(format!("{label} must be a valid UUID.")))?;
    if parsed.is_nil() {
        return Err(AppError::Invalid(format!(
            "{label} cannot be the all-zero UUID."
        )));
    }
    Ok(())
}

fn validate_resource_group(value: Option<&str>) -> AppResult<()> {
    let Some(value) = value.filter(|value| !value.is_empty()) else {
        return Ok(());
    };
    if value.len() > 90
        || value.ends_with('.')
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_.()".contains(character))
    {
        return Err(AppError::Invalid(
            "Invalid Azure resource group name.".into(),
        ));
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

fn valid_environment_variable_name(value: &str) -> bool {
    let mut characters = value.chars();
    !value.is_empty()
        && value.len() <= 128
        && characters
            .next()
            .is_some_and(|character| character.is_ascii_uppercase())
        && characters.all(|character| {
            character.is_ascii_uppercase() || character.is_ascii_digit() || character == '_'
        })
}

fn environment_value_as_string(
    environment_variable: &str,
    value: &serde_json::Value,
) -> AppResult<Option<String>> {
    let value = match value {
        serde_json::Value::Null => return Ok(None),
        serde_json::Value::String(value) => value.clone(),
        serde_json::Value::Bool(value) => value.to_string(),
        serde_json::Value::Array(values) => values
            .iter()
            .filter_map(serde_json::Value::as_str)
            .collect::<Vec<_>>()
            .join(","),
        _ => {
            return Err(AppError::Invalid(format!(
                "Unsupported environment value type: {environment_variable}"
            )))
        }
    };
    if value.len() > 32_768 {
        return Err(AppError::Invalid(format!(
            "Environment value is too large: {environment_variable}"
        )));
    }
    Ok(Some(value))
}

fn environment_assignments(
    request: &PrepareDeploymentRequest,
    entry: Option<&crate::models::CatalogEntry>,
) -> AppResult<Vec<(String, String)>> {
    let allowed = entry.map(|entry| {
        entry
            .manifest
            .iter()
            .flat_map(|manifest| &manifest.configuration.groups)
            .flat_map(|group| &group.fields)
            .map(|field| field.environment_variable.as_str())
            .collect::<HashSet<_>>()
    });
    let mut assignments = BTreeMap::new();
    assignments.insert("AZURE_LOCATION".into(), request.location.clone());
    if let Some(tenant) = request.tenant.as_deref().filter(|value| !value.is_empty()) {
        assignments.insert("AZURE_TENANT_ID".into(), tenant.into());
    }
    if let Some(subscription) = request
        .subscription
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        assignments.insert("AZURE_SUBSCRIPTION_ID".into(), subscription.into());
    }
    if let Some(resource_group) = request
        .resource_group
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        assignments.insert("AZURE_RESOURCE_GROUP".into(), resource_group.into());
    }

    for (environment_variable, value) in &request.environment_values {
        if !valid_environment_variable_name(environment_variable) {
            return Err(AppError::Invalid(format!(
                "Invalid environment variable name: {environment_variable}"
            )));
        }
        if environment_variable.starts_with("AZURE_") {
            return Err(AppError::Invalid(format!(
                "Standard azd environment values must use their dedicated fields: {environment_variable}"
            )));
        }
        if allowed
            .as_ref()
            .is_some_and(|allowed| !allowed.contains(environment_variable.as_str()))
        {
            return Err(AppError::Invalid(format!(
                "Catalog manifest does not declare environment variable: {environment_variable}"
            )));
        }
        if let Some(value) = environment_value_as_string(environment_variable, value)? {
            assignments.insert(environment_variable.clone(), value);
        }
    }
    Ok(assignments.into_iter().collect())
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
    validate_source_trust(&request)?;
    validate_environment_name(&request.environment)?;
    validate_location(&request.location)?;
    validate_optional_guid("Tenant ID", request.tenant.as_deref())?;
    validate_optional_guid("Subscription ID", request.subscription.as_deref())?;
    validate_resource_group(request.resource_group.as_deref())?;
    let existing_project = request.source_mode == "existingProject";

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
    let environment_assignments = environment_assignments(&request, entry.as_ref())?;
    let destination = ensure_destination(&request.destination, existing_project)?;

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

    let existing_environment_missing = if existing_project {
        let environments = SystemAzdClient.environment_list(&destination).await?;
        !environment_exists(&environments, &request.environment)
    } else {
        false
    };
    if existing_environment_missing {
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

    for (environment_variable, value) in environment_assignments {
        run_checked(
            CommandSpec::new(
                Executable::Azd,
                [
                    "env",
                    "set",
                    &environment_variable,
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

#[cfg(test)]
mod tests {
    use super::{
        environment_assignments, environment_exists, validate_optional_guid,
        validate_resource_group, validate_source_trust,
    };
    use crate::{
        catalog::entry_by_id,
        models::{PrepareDeploymentRequest, ProjectEnvironment},
    };
    use serde_json::json;
    use std::collections::HashMap;

    fn myworkid_request() -> PrepareDeploymentRequest {
        PrepareDeploymentRequest {
            template_id: "myworkid".into(),
            source_mode: "catalog".into(),
            source_trust_accepted: false,
            source_path: None,
            source_revision: None,
            destination: "/tmp/myworkid".into(),
            environment: "dev".into(),
            tenant: Some("11111111-1111-1111-1111-111111111111".into()),
            subscription: Some("22222222-2222-2222-2222-222222222222".into()),
            location: "westus2".into(),
            resource_group: Some("rg-myworkid-dev".into()),
            environment_values: HashMap::from([
                ("MYWORKID_DEPLOY_MODE".into(), json!("releaseZip")),
                ("MYWORKID_RELEASE_VERSION".into(), json!("latest")),
                (
                    "MYWORKID_ENABLE_APP_SERVICE_MANAGED_CERTIFICATE".into(),
                    json!(true),
                ),
            ]),
        }
    }

    #[test]
    fn materializes_standard_and_manifest_declared_values() {
        let entry = entry_by_id("myworkid").unwrap();
        let assignments = environment_assignments(&myworkid_request(), Some(&entry)).unwrap();
        let assignments = assignments.into_iter().collect::<HashMap<_, _>>();

        assert_eq!(assignments["AZURE_LOCATION"], "westus2");
        assert_eq!(
            assignments["AZURE_TENANT_ID"],
            "11111111-1111-1111-1111-111111111111"
        );
        assert_eq!(
            assignments["AZURE_SUBSCRIPTION_ID"],
            "22222222-2222-2222-2222-222222222222"
        );
        assert_eq!(assignments["AZURE_RESOURCE_GROUP"], "rg-myworkid-dev");
        assert_eq!(assignments["MYWORKID_DEPLOY_MODE"], "releaseZip");
        assert_eq!(
            assignments["MYWORKID_ENABLE_APP_SERVICE_MANAGED_CERTIFICATE"],
            "true"
        );
    }

    #[test]
    fn rejects_undeclared_catalog_environment_values() {
        let entry = entry_by_id("myworkid").unwrap();
        let mut request = myworkid_request();
        request
            .environment_values
            .insert("MYWORKID_ENABLE_GRAPH_EXPORT".into(), json!(true));

        let error = environment_assignments(&request, Some(&entry))
            .unwrap_err()
            .to_string();
        assert!(error.contains("does not declare"));
    }

    #[test]
    fn reserves_standard_azd_values_for_typed_fields() {
        let entry = entry_by_id("myworkid").unwrap();
        let mut request = myworkid_request();
        request.environment_values.insert(
            "AZURE_SUBSCRIPTION_ID".into(),
            json!("33333333-3333-3333-3333-333333333333"),
        );

        let error = environment_assignments(&request, Some(&entry))
            .unwrap_err()
            .to_string();
        assert!(error.contains("dedicated fields"));
    }

    #[test]
    fn validates_azure_identifiers_before_materialization() {
        assert!(validate_optional_guid("Tenant ID", Some("not-a-guid")).is_err());
        assert!(
            validate_optional_guid("Tenant ID", Some("00000000-0000-0000-0000-000000000000"))
                .is_err()
        );
        assert!(validate_resource_group(Some("invalid.")).is_err());
        assert!(validate_resource_group(Some("rg-valid_(dev)")).is_ok());
    }

    #[test]
    fn requires_explicit_trust_for_unvetted_sources() {
        let mut request = myworkid_request();
        request.source_mode = "github".into();
        assert!(validate_source_trust(&request).is_err());
        request.source_trust_accepted = true;
        assert!(validate_source_trust(&request).is_ok());
    }

    #[test]
    fn checks_the_requested_environment_instead_of_the_azure_folder() {
        let environments = vec![ProjectEnvironment {
            name: "dev".into(),
            dot_env_path: String::new(),
            has_local: true,
            has_remote: false,
            is_default: true,
        }];
        assert!(environment_exists(&environments, "dev"));
        assert!(!environment_exists(&environments, "staging"));
    }
}
