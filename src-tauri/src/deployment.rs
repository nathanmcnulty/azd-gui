use crate::{
    azd,
    catalog::{entry_by_id, template_path},
    command_runner::{capture, CommandSpec, Executable},
    error::{AppError, AppResult},
    models::{
        CatalogEntry, PrepareDeploymentRequest, ProjectEnvironment, ProjectSummary,
        ProjectTemplateSnapshot,
    },
    persistence::Database,
};
use chrono::Utc;
use std::{
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
    time::Duration,
};

const MAX_TEMPLATE_FILE_COUNT: usize = 20_000;
const MAX_TEMPLATE_FILE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_TEMPLATE_TOTAL_BYTES: u64 = 512 * 1024 * 1024;
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
        if crate::manifest::is_standard_environment_variable(environment_variable) {
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

fn validate_template_tree(output: &str) -> AppResult<()> {
    if output.contains("[output truncated]") {
        return Err(AppError::Invalid(
            "Template tree listing exceeded the inspection limit.".into(),
        ));
    }
    let mut file_count = 0_usize;
    let mut total_bytes = 0_u64;
    for line in output.lines().filter(|line| !line.trim().is_empty()) {
        let (metadata, _) = line.split_once('\t').ok_or_else(|| {
            AppError::Invalid("Git returned an invalid template tree entry.".into())
        })?;
        let fields = metadata.split_whitespace().collect::<Vec<_>>();
        if fields.get(1) != Some(&"blob") {
            continue;
        }
        let size = fields
            .get(3)
            .and_then(|value| value.parse::<u64>().ok())
            .ok_or_else(|| AppError::Invalid("Git omitted a template file size.".into()))?;
        file_count = file_count.saturating_add(1);
        total_bytes = total_bytes.saturating_add(size);
        if file_count > MAX_TEMPLATE_FILE_COUNT {
            return Err(AppError::Invalid(format!(
                "Template contains more than {MAX_TEMPLATE_FILE_COUNT} files."
            )));
        }
        if size > MAX_TEMPLATE_FILE_BYTES {
            return Err(AppError::Invalid(format!(
                "Template contains a file larger than {} MB.",
                MAX_TEMPLATE_FILE_BYTES / 1024 / 1024
            )));
        }
        if total_bytes > MAX_TEMPLATE_TOTAL_BYTES {
            return Err(AppError::Invalid(format!(
                "Template content exceeds {} MB.",
                MAX_TEMPLATE_TOTAL_BYTES / 1024 / 1024
            )));
        }
    }
    Ok(())
}

async fn checkout_pinned_template(
    cache_root: &Path,
    repository: &str,
    commit: &str,
    template_id: &str,
) -> AppResult<tempfile::TempDir> {
    let pinned_options = crate::catalog::pinned_git_options(repository).await?;
    std::fs::create_dir_all(cache_root)?;
    let checkout = tempfile::Builder::new()
        .prefix(&format!("{template_id}-"))
        .tempdir_in(cache_root)?;
    let git_command = |checkout: &Path, args: &[&str]| {
        let mut options = pinned_options.clone();
        options.extend(args.iter().map(|arg| (*arg).to_string()));
        crate::command_runner::git_command(checkout, options)
    };
    run_checked(
        git_command(checkout.path(), &["init", "--quiet", "--template="]),
        "Initializing template checkout",
        Duration::from_secs(20),
    )
    .await?;
    run_checked(
        git_command(checkout.path(), &["remote", "add", "origin", repository]),
        "Configuring template source",
        Duration::from_secs(20),
    )
    .await?;
    run_checked(
        git_command(
            checkout.path(),
            &[
                "fetch",
                "--quiet",
                "--no-tags",
                "--depth",
                "1",
                "origin",
                commit,
            ],
        ),
        "Fetching pinned template",
        Duration::from_secs(180),
    )
    .await?;
    let tree = run_checked(
        git_command(checkout.path(), &["ls-tree", "-r", "-l", "FETCH_HEAD"]),
        "Inspecting pinned template size",
        Duration::from_secs(30),
    )
    .await?;
    validate_template_tree(&tree)?;
    run_checked(
        git_command(
            checkout.path(),
            &["checkout", "--quiet", "--detach", "FETCH_HEAD"],
        ),
        "Checking out pinned template",
        Duration::from_secs(60),
    )
    .await?;
    let actual = run_checked(
        git_command(checkout.path(), &["rev-parse", "HEAD"]),
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
    Ok(checkout)
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
            return Ok(std::fs::canonicalize(destination)?);
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
    Ok(std::fs::canonicalize(destination)?)
}

async fn resolve_entry_with_local_source(
    request: &PrepareDeploymentRequest,
    database: &Database,
    inspected_local_source: Option<&Path>,
) -> AppResult<CatalogEntry> {
    let entry = match request.source_mode.as_str() {
        "catalog" => entry_by_id(&request.template_id)?,
        "externalCatalog" => database
            .accepted_catalog_entry(&request.template_id)?
            .ok_or_else(|| {
                AppError::Invalid(
                    "External template is not present in an accepted catalog snapshot.".into(),
                )
            })?,
        "localTemplate" | "existingProject" => {
            let source = request
                .source_path
                .as_deref()
                .ok_or_else(|| AppError::Invalid("Local source path is required.".into()))?;
            if request.source_mode == "existingProject"
                && std::fs::canonicalize(source)? != std::fs::canonicalize(&request.destination)?
            {
                return Err(AppError::Invalid(
                    "The inspected source and existing project must match.".into(),
                ));
            }
            let source = inspected_local_source
                .unwrap_or_else(|| Path::new(source))
                .to_str()
                .ok_or_else(|| {
                    AppError::Invalid("Local source path is not valid Unicode.".into())
                })?;
            let mut entry = crate::catalog::inspect_local(source)?.entry;
            if let Some(binding) = &request.source_binding {
                entry.repository = binding.source_path.clone();
                entry.description = format!("Local project loaded from {}", binding.source_path);
            }
            entry
        }
        "github" => {
            crate::catalog::inspect_github_at_revision(
                request
                    .source_path
                    .as_deref()
                    .ok_or_else(|| AppError::Invalid("GitHub source is required.".into()))?,
                request.source_revision.as_deref().ok_or_else(|| {
                    AppError::Invalid("Pinned GitHub revision is required.".into())
                })?,
            )
            .await?
            .entry
        }
        _ => return Err(AppError::Invalid("Unsupported template source.".into())),
    };
    let expected_revision = (entry.commit != "local").then_some(entry.commit.as_str());
    if entry.id != request.template_id || request.source_revision.as_deref() != expected_revision {
        return Err(AppError::Invalid(
            "Template identity changed. Inspect and approve the source again.".into(),
        ));
    }
    if let Some(manifest) = &entry.manifest {
        crate::manifest::validate(manifest)?;
    }
    Ok(entry)
}

fn stage_local_source(
    request: &PrepareDeploymentRequest,
    cache_root: &Path,
) -> AppResult<Option<tempfile::TempDir>> {
    if !matches!(
        request.source_mode.as_str(),
        "localTemplate" | "existingProject"
    ) {
        return Ok(None);
    }
    let source = request
        .source_path
        .as_deref()
        .ok_or_else(|| AppError::Invalid("Local source path is required.".into()))?;
    let binding = request.source_binding.as_ref().ok_or_else(|| {
        AppError::Invalid(
            "Local templates require a fresh native source inspection before approval.".into(),
        )
    })?;
    Ok(Some(crate::local_source::stage_for_path(
        binding,
        Path::new(source),
        &cache_root.join("local-source-staging"),
    )?))
}

pub async fn prepare(
    request: PrepareDeploymentRequest,
    cache_root: &Path,
    database: &Database,
) -> AppResult<ProjectSummary> {
    validate_template_id(&request.template_id)?;
    validate_environment_name(&request.environment)?;
    validate_location(&request.location)?;
    validate_optional_guid("Tenant ID", request.tenant.as_deref())?;
    validate_optional_guid("Subscription ID", request.subscription.as_deref())?;
    validate_resource_group(request.resource_group.as_deref())?;
    let existing_project = request.source_mode == "existingProject";

    let local_stage = stage_local_source(&request, cache_root)?;
    let entry = Some(
        resolve_entry_with_local_source(
            &request,
            database,
            local_stage.as_ref().map(|stage| stage.path()),
        )
        .await?,
    );
    if let Some(manifest) = entry.as_ref().and_then(|entry| entry.manifest.as_ref()) {
        crate::tools::ensure_manifest_requirements(manifest).await?;
    }
    let snapshot =
        ProjectTemplateSnapshot::new(entry.as_ref().expect("resolved template").clone())?;
    let environment_assignments = environment_assignments(&request, entry.as_ref())?;
    let destination = ensure_destination(&request.destination, existing_project)?;
    let name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("azd-project")
        .to_string();
    let mut project = ProjectSummary {
        id: format!("{}-{}", name, request.environment),
        name,
        path: database
            .project_by_path(&destination.display().to_string())?
            .map(|project| project.path)
            .unwrap_or_else(|| destination.display().to_string()),
        environment: Some(request.environment.clone()),
        template_id: Some(request.template_id.clone()),
        revision: entry
            .as_ref()
            .filter(|entry| entry.commit != "local")
            .map(|entry| entry.commit.clone())
            .or_else(|| request.source_revision.clone()),
        last_operation: Some(Utc::now().to_rfc3339()),
        state: "attention".into(),
    };

    database.upsert_project(&project)?;
    database.save_project_template(&project, &snapshot)?;

    if !existing_project {
        let mut temporary_checkout = None;
        let source = match request.source_mode.as_str() {
            "catalog" | "externalCatalog" => {
                let entry = entry
                    .as_ref()
                    .ok_or_else(|| AppError::Invalid("Missing catalog entry.".into()))?;
                let checkout = checkout_pinned_template(
                    cache_root,
                    &entry.repository,
                    &entry.commit,
                    &entry.id,
                )
                .await?;
                let source = template_path(checkout.path(), entry)?;
                temporary_checkout = Some(checkout);
                source
            }
            "localTemplate" => {
                let source = local_stage
                    .as_ref()
                    .ok_or_else(|| AppError::Invalid("Approved local source is unavailable.".into()))?
                    .path()
                    .to_path_buf();
                if !source.join("azure.yaml").is_file() {
                    return Err(AppError::Invalid(
                        "Approved local source does not contain azure.yaml.".into(),
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
                if !parsed.username().is_empty()
                    || parsed.password().is_some()
                    || parsed.query().is_some()
                    || parsed.fragment().is_some()
                {
                    return Err(AppError::Url(
                        "Direct repository URLs cannot contain credentials, query parameters, or fragments."
                            .into(),
                    ));
                }
                if commit.len() != 40
                    || !commit
                        .chars()
                        .all(|character| character.is_ascii_hexdigit())
                {
                    return Err(AppError::Invalid("Invalid pinned GitHub revision.".into()));
                }
                let checkout =
                    checkout_pinned_template(cache_root, repository, commit, &request.template_id)
                        .await?;
                let source = template_path(
                    checkout.path(),
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
                        manifest_sha256: None,
                        source: Some("external".into()),
                        trusted: false,
                    },
                )?;
                temporary_checkout = Some(checkout);
                source
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
        drop(temporary_checkout);
    } else if !destination.join("azure.yaml").is_file() {
        return Err(AppError::Invalid(
            "Existing project does not contain azure.yaml.".into(),
        ));
    }

    let existing_environment_missing = if existing_project {
        let environments = azd::environment_list(&destination).await?;
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

    azd::environment_set_values(&destination, &request.environment, &environment_assignments)
        .await?;

    project.state = "ready".into();
    project.last_operation = Some(Utc::now().to_rfc3339());
    database.upsert_project(&project)?;
    Ok(project)
}

#[cfg(test)]
mod tests {
    use super::{
        environment_assignments, environment_exists, validate_optional_guid,
        validate_resource_group, validate_template_tree, MAX_TEMPLATE_FILE_BYTES,
    };
    use crate::{
        catalog::entry_by_id,
        models::{PrepareDeploymentRequest, ProjectEnvironment},
    };
    use serde_json::json;

    #[test]
    fn template_tree_limits_files_and_total_content() {
        let valid = "100644 blob 0123456789abcdef 128\tazure.yaml\n100644 blob fedcba9876543210 64\tsrc/main.ts\n";
        assert!(validate_template_tree(valid).is_ok());
        let oversized = format!(
            "100644 blob 0123456789abcdef {}\tlarge.bin\n",
            MAX_TEMPLATE_FILE_BYTES + 1
        );
        assert!(validate_template_tree(&oversized).is_err());
        assert!(validate_template_tree("[output truncated]").is_err());
    }
    use std::collections::HashMap;

    fn myworkid_request() -> PrepareDeploymentRequest {
        PrepareDeploymentRequest {
            template_id: "myworkid".into(),
            source_mode: "catalog".into(),
            execution_approval_token: "test-approval".into(),
            source_path: None,
            source_revision: None,
            source_binding: None,
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

    #[tokio::test]
    #[ignore = "requires installed azd; initializes only a temporary hook-free local fixture"]
    async fn installed_azd_prepares_the_selected_destination() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("fixture-source");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("azure.yaml"), "name: local-preparation-smoke\n").unwrap();
        let inspection = crate::catalog::inspect_local(source.to_str().unwrap()).unwrap();
        let mut request = myworkid_request();
        request.template_id = inspection.entry.id;
        request.source_mode = "localTemplate".into();
        request.source_path = Some(source.display().to_string());
        request.source_binding = inspection.source_binding;
        request.destination = root
            .path()
            .join("selected-destination")
            .display()
            .to_string();
        request.tenant = None;
        request.subscription = None;
        request.resource_group = None;
        request.environment_values.clear();
        let database = crate::persistence::Database::open(&root.path().join("review.db")).unwrap();
        let project = super::prepare(request, &root.path().join("cache"), &database)
            .await
            .unwrap();
        assert!(std::path::Path::new(&project.path)
            .join("azure.yaml")
            .is_file());
        assert!(std::path::Path::new(&project.path)
            .join(".azure/dev/.env")
            .is_file());
    }

    #[tokio::test]
    async fn staged_local_contract_preserves_reviewed_identity() {
        let source = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();
        std::fs::write(source.path().join("azure.yaml"), "name: staged-sample").unwrap();
        let inspected = crate::catalog::inspect_local(source.path().to_str().unwrap()).unwrap();
        let mut request = myworkid_request();
        request.template_id = inspected.entry.id.clone();
        request.source_mode = "localTemplate".into();
        request.source_path = Some(source.path().display().to_string());
        request.source_binding = inspected.source_binding;
        let staged = super::stage_local_source(&request, cache.path())
            .unwrap()
            .unwrap();
        let database = crate::persistence::Database::open(&cache.path().join("review.db")).unwrap();
        let entry =
            super::resolve_entry_with_local_source(&request, &database, Some(staged.path()))
                .await
                .unwrap();
        assert_eq!(entry.id, inspected.entry.id);
        assert_eq!(
            entry.repository,
            request.source_binding.unwrap().source_path
        );
        assert_eq!(entry.commit, "local");
    }

    #[tokio::test]
    #[ignore = "requires Git and public GitHub access; fetches an immutable reference template without executing it"]
    async fn installed_git_fetches_a_pinned_public_template() {
        let cache = tempfile::tempdir().unwrap();
        let checkout = super::checkout_pinned_template(
            cache.path(),
            "https://github.com/nathanmcnulty/azd-reference",
            "20a5059c53c105f8c6999f33fa0852410eac8ace",
            "reference-transport-smoke",
        )
        .await
        .unwrap();
        assert!(checkout.path().join("skeleton/azure.yaml").is_file());
        assert!(checkout.path().join("skeleton/azd-gui.json").is_file());
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
    fn allows_maester_declared_azure_rbac_scopes_without_overriding_target() {
        let entry = entry_by_id("maester-automation").unwrap();
        let mut request = myworkid_request();
        request.environment_values =
            HashMap::from([("AZURE_RBAC_SCOPES".into(), json!("/subscriptions/example"))]);
        let values = environment_assignments(&request, Some(&entry))
            .unwrap()
            .into_iter()
            .collect::<HashMap<_, _>>();
        assert_eq!(values["AZURE_RBAC_SCOPES"], "/subscriptions/example");
        assert_eq!(
            values["AZURE_SUBSCRIPTION_ID"],
            request.subscription.unwrap()
        );
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
