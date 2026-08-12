use crate::{
    command_runner::{capture, CommandSpec, Executable},
    error::{AppError, AppResult},
    models::{
        Catalog, CatalogEntry, CatalogFile, Configuration, ConnectionRequirement, Prerequisite,
        TemplateInspection, TemplateManifest,
    },
    permissions,
};
use once_cell::sync::Lazy;
use regex::Regex;
use std::path::{Path, PathBuf};
use std::time::Duration;
use url::Url;

const CATALOG_JSON: &str = include_str!("../../catalog/azd-gui.catalog.json");
const MYWORKID_MANIFEST: &str = include_str!("../../catalog/manifests/myworkid.json");
const MAESTER_MANIFEST: &str = include_str!("../../catalog/manifests/maester-automation.json");
const ENTRA_HEALTH_MANIFEST: &str =
    include_str!("../../catalog/manifests/entra-health-monitoring.json");
static COMMIT_REVISION: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[a-fA-F0-9]{40}$").expect("static commit regex"));

fn reject_private_url_components(url: &Url, label: &str) -> AppResult<()> {
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(AppError::Url(format!(
            "{label} cannot contain credentials, query parameters, or fragments."
        )));
    }
    Ok(())
}

fn same_origin(left: &Url, right: &Url) -> bool {
    left.scheme() == "https"
        && right.scheme() == "https"
        && left.host_str() == right.host_str()
        && left.port_or_known_default() == right.port_or_known_default()
}

fn restricted_http_client(origin: &Url) -> AppResult<reqwest::Client> {
    let origin = origin.clone();
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::custom(move |attempt| {
            if attempt.previous().len() >= 5 {
                return attempt.error("Too many redirects.");
            }
            if same_origin(&origin, attempt.url()) {
                attempt.follow()
            } else {
                attempt.error("Redirects must remain on the original HTTPS origin.")
            }
        }))
        .build()
        .map_err(AppError::from)
}

fn require_response_origin(origin: &Url, response: &reqwest::Response) -> AppResult<()> {
    if same_origin(origin, response.url()) {
        Ok(())
    } else {
        Err(AppError::Url(
            "The response left the original HTTPS origin.".into(),
        ))
    }
}

fn manifest_json(path: &str) -> AppResult<&'static str> {
    match path.replace('\\', "/").as_str() {
        "manifests/myworkid.json" => Ok(MYWORKID_MANIFEST),
        "manifests/maester-automation.json" => Ok(MAESTER_MANIFEST),
        "manifests/entra-health-monitoring.json" => Ok(ENTRA_HEALTH_MANIFEST),
        _ => Err(AppError::Invalid(format!(
            "Bundled catalog references an unknown manifest: {path}"
        ))),
    }
}

fn validate_entry(entry: &CatalogFileEntryRef<'_>) -> AppResult<()> {
    if !COMMIT_REVISION.is_match(entry.commit) {
        return Err(AppError::Invalid(format!(
            "Catalog entry {} is not pinned to a full commit.",
            entry.id
        )));
    }
    let url = Url::parse(entry.repository).map_err(|error| AppError::Url(error.to_string()))?;
    if url.scheme() != "https" {
        return Err(AppError::Url("Catalog repositories must use HTTPS.".into()));
    }
    reject_private_url_components(&url, "Catalog repository URL")?;
    if entry
        .subdirectory
        .is_some_and(|path| path.contains("..") || Path::new(path).is_absolute())
    {
        return Err(AppError::Invalid(
            "Template subdirectory must remain inside the repository.".into(),
        ));
    }
    Ok(())
}

struct CatalogFileEntryRef<'a> {
    id: &'a str,
    repository: &'a str,
    commit: &'a str,
    subdirectory: Option<&'a str>,
}

pub fn bundled_catalog() -> AppResult<Catalog> {
    let file: CatalogFile = serde_json::from_str(CATALOG_JSON)?;
    let mut entries = Vec::with_capacity(file.entries.len());
    for entry in file.entries {
        validate_entry(&CatalogFileEntryRef {
            id: &entry.id,
            repository: &entry.repository,
            commit: &entry.commit,
            subdirectory: entry.subdirectory.as_deref(),
        })?;
        let manifest: Option<TemplateManifest> = entry
            .manifest
            .as_deref()
            .map(manifest_json)
            .transpose()?
            .map(serde_json::from_str)
            .transpose()?;
        if let Some(manifest) = &manifest {
            permissions::validate(&manifest.permission_checks)?;
        }
        entries.push(CatalogEntry {
            id: entry.id,
            title: entry.title,
            summary: entry.summary,
            description: entry.description,
            repository: entry.repository,
            commit: entry.commit,
            subdirectory: entry.subdirectory,
            tags: entry.tags,
            platforms: entry.platforms,
            manifest,
            source: Some("bundled".into()),
            trusted: true,
        });
    }
    Ok(Catalog {
        schema_version: file.schema_version,
        id: file.id,
        title: file.title,
        entries,
    })
}

pub async fn preview_external_catalog(url: &str) -> AppResult<Catalog> {
    let url = normalize_catalog_url(url)?;
    let client = restricted_http_client(&url)?;
    let response = client
        .get(url.clone())
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await?
        .error_for_status()?;
    require_response_origin(&url, &response)?;
    let resolved_catalog_url = response.url().clone();
    if response.content_length().unwrap_or(0) > 2_000_000 {
        return Err(AppError::Invalid("Catalog exceeds the 2 MB limit.".into()));
    }
    let bytes = response.bytes().await?;
    if bytes.len() > 2_000_000 {
        return Err(AppError::Invalid("Catalog exceeds the 2 MB limit.".into()));
    }
    let file: CatalogFile = serde_json::from_slice(&bytes)?;
    if file.schema_version != "1.0" {
        return Err(AppError::Invalid(format!(
            "Unsupported catalog schema version: {}",
            file.schema_version
        )));
    }
    if file.id == "bundled" {
        return Err(AppError::Invalid(
            "External catalogs cannot use the reserved bundled catalog ID.".into(),
        ));
    }
    let external_catalog_id = file.id.clone();
    let mut entries = Vec::with_capacity(file.entries.len());
    for entry in file.entries {
        validate_entry(&CatalogFileEntryRef {
            id: &entry.id,
            repository: &entry.repository,
            commit: &entry.commit,
            subdirectory: entry.subdirectory.as_deref(),
        })?;
        let manifest = if let Some(path) = entry.manifest.as_deref() {
            if path.contains("..")
                || path.starts_with('/')
                || path.starts_with('\\')
                || !path.ends_with(".json")
            {
                return Err(AppError::Invalid(format!(
                    "External manifest path is not allowed: {path}"
                )));
            }
            let manifest_url = resolved_catalog_url
                .join(path)
                .map_err(|error| AppError::Url(error.to_string()))?;
            if manifest_url.scheme() != "https"
                || manifest_url.host_str() != resolved_catalog_url.host_str()
            {
                return Err(AppError::Url(
                    "External manifests must use the catalog's HTTPS origin.".into(),
                ));
            }
            let response = client
                .get(manifest_url.clone())
                .timeout(std::time::Duration::from_secs(20))
                .send()
                .await?
                .error_for_status()?;
            require_response_origin(&resolved_catalog_url, &response)?;
            if response.content_length().unwrap_or(0) > 512_000 {
                return Err(AppError::Invalid(
                    "External template manifest exceeds the 512 KB limit.".into(),
                ));
            }
            let manifest_bytes = response.bytes().await?;
            if manifest_bytes.len() > 512_000 {
                return Err(AppError::Invalid(
                    "External template manifest exceeds the 512 KB limit.".into(),
                ));
            }
            let manifest: TemplateManifest = serde_json::from_slice(&manifest_bytes)?;
            if manifest.schema_version != "1.0" {
                return Err(AppError::Invalid(format!(
                    "Unsupported template manifest version: {}",
                    manifest.schema_version
                )));
            }
            permissions::validate(&manifest.permission_checks)?;
            Some(manifest)
        } else {
            None
        };
        entries.push(CatalogEntry {
            id: format!("external-{}.{}", external_catalog_id, entry.id),
            title: entry.title,
            summary: entry.summary,
            description: entry.description,
            repository: entry.repository,
            commit: entry.commit,
            subdirectory: entry.subdirectory,
            tags: entry.tags,
            platforms: entry.platforms,
            manifest,
            source: Some("external".into()),
            trusted: false,
        });
    }
    Ok(Catalog {
        schema_version: file.schema_version,
        id: file.id,
        title: file.title,
        entries,
    })
}

pub(crate) fn normalize_catalog_url(input: &str) -> AppResult<Url> {
    let mut url = Url::parse(input).map_err(|error| AppError::Url(error.to_string()))?;
    if url.scheme() != "https" {
        return Err(AppError::Url(
            "Only HTTPS catalog sources are allowed.".into(),
        ));
    }
    reject_private_url_components(&url, "Catalog URL")?;
    if url.host_str() == Some("github.com") {
        let segments = url
            .path_segments()
            .map(|segments| {
                segments
                    .filter(|value| !value.is_empty())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if segments.len() == 2 {
            let repository = segments[1].trim_end_matches(".git");
            url = Url::parse(&format!(
                "https://raw.githubusercontent.com/{}/{}/HEAD/azd-gui.catalog.json",
                segments[0], repository
            ))
            .map_err(|error| AppError::Url(error.to_string()))?;
        }
    }
    if url.host_str().is_none() {
        return Err(AppError::Url("Catalog URL must have a host.".into()));
    }
    reject_private_url_components(&url, "Catalog URL")?;
    Ok(url)
}

pub fn inspect_local(path: &str) -> AppResult<TemplateInspection> {
    let canonical = std::fs::canonicalize(path)?;
    if !canonical.is_dir() {
        return Err(AppError::Invalid("Selected path is not a folder.".into()));
    }
    let azure_yaml = canonical.join("azure.yaml");
    if !azure_yaml.is_file() {
        return Err(AppError::Invalid(
            "The selected folder does not contain azure.yaml.".into(),
        ));
    }
    let yaml_text = std::fs::read_to_string(&azure_yaml)?;
    let yaml: serde_yaml::Value = serde_yaml::from_str(&yaml_text)?;
    let name = yaml
        .get("name")
        .and_then(serde_yaml::Value::as_str)
        .unwrap_or("local-azd-project")
        .to_string();
    let embedded_path = canonical.join("azd-gui.json");
    let (manifest, manifest_source) = if embedded_path.is_file() {
        (
            serde_json::from_str(&std::fs::read_to_string(embedded_path)?)?,
            "embedded".to_string(),
        )
    } else {
        (inferred_manifest(&yaml), "inferred".to_string())
    };
    if manifest.schema_version != "1.0" {
        return Err(AppError::Invalid(format!(
            "Unsupported template manifest version: {}",
            manifest.schema_version
        )));
    }
    permissions::validate(&manifest.permission_checks)?;
    let interactive_hooks = yaml_text.to_ascii_lowercase().contains("interactive: true");
    let windows_hooks = yaml_text.contains("windows:");
    let posix_hooks = yaml_text.contains("posix:");
    let platforms = if windows_hooks && !posix_hooks {
        vec!["windows".into()]
    } else {
        vec!["windows".into(), "macos".into(), "linux".into()]
    };
    let mut warnings =
        vec!["Local templates are unvetted. Review azure.yaml hooks before deploying.".into()];
    if interactive_hooks {
        warnings.push(
            "Interactive hooks were detected. Run this template in an external terminal if --no-prompt cannot complete."
                .into(),
        );
    }
    Ok(TemplateInspection {
        entry: CatalogEntry {
            id: format!("local-{}", name.to_ascii_lowercase().replace(' ', "-")),
            title: name.clone(),
            summary: "Local Azure Developer CLI project".into(),
            description: format!("Local project loaded from {}", canonical.display()),
            repository: canonical.display().to_string(),
            commit: "local".into(),
            subdirectory: None,
            tags: vec!["Local".into(), "Unvetted".into()],
            platforms,
            manifest: Some(manifest),
            source: Some("local".into()),
            trusted: false,
        },
        source_path: Some(canonical.display().to_string()),
        azure_yaml_found: true,
        manifest_source,
        interactive_hooks,
        warnings,
    })
}

pub async fn inspect_github(url: &str) -> AppResult<TemplateInspection> {
    let parsed = Url::parse(url).map_err(|error| AppError::Url(error.to_string()))?;
    if parsed.scheme() != "https" || parsed.host_str() != Some("github.com") {
        return Err(AppError::Url(
            "Direct repository sources must be HTTPS GitHub URLs.".into(),
        ));
    }
    reject_private_url_components(&parsed, "GitHub repository URL")?;
    let segments = parsed
        .path_segments()
        .map(|segments| {
            segments
                .filter(|value| !value.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if segments.len() != 2 {
        return Err(AppError::Url(
            "Use a repository URL in the form https://github.com/owner/repository.".into(),
        ));
    }
    let output = capture(
        CommandSpec::new(Executable::Git, ["ls-remote", url, "HEAD"]),
        Duration::from_secs(30),
    )
    .await?;
    if !output.success {
        return Err(AppError::Command(format!(
            "Could not resolve repository HEAD: {}",
            output.stderr
        )));
    }
    let commit = output
        .stdout
        .split_whitespace()
        .next()
        .filter(|value| {
            value.len() == 40 && value.chars().all(|character| character.is_ascii_hexdigit())
        })
        .ok_or_else(|| AppError::Invalid("GitHub did not return a valid commit revision.".into()))?
        .to_string();
    let repository_name = segments[1].trim_end_matches(".git");
    let raw_base = format!(
        "https://raw.githubusercontent.com/{}/{}/{}",
        segments[0], repository_name, commit
    );
    let raw_origin = Url::parse(&raw_base).map_err(|error| AppError::Url(error.to_string()))?;
    let client = restricted_http_client(&raw_origin)?;
    let azure_response = client
        .get(format!("{raw_base}/azure.yaml"))
        .timeout(Duration::from_secs(20))
        .send()
        .await?
        .error_for_status()?;
    require_response_origin(&raw_origin, &azure_response)?;
    let yaml_text = azure_response.text().await?;
    let yaml: serde_yaml::Value = serde_yaml::from_str(&yaml_text)?;
    let embedded_manifest = client
        .get(format!("{raw_base}/azd-gui.json"))
        .timeout(Duration::from_secs(20))
        .send()
        .await?;
    require_response_origin(&raw_origin, &embedded_manifest)?;
    let (manifest, manifest_source) = if embedded_manifest.status().is_success() {
        (
            serde_json::from_slice::<TemplateManifest>(&embedded_manifest.bytes().await?)?,
            "embedded",
        )
    } else {
        (inferred_manifest(&yaml), "inferred")
    };
    if manifest.schema_version != "1.0" {
        return Err(AppError::Invalid(format!(
            "Unsupported template manifest version: {}",
            manifest.schema_version
        )));
    }
    permissions::validate(&manifest.permission_checks)?;
    let name = yaml
        .get("name")
        .and_then(serde_yaml::Value::as_str)
        .unwrap_or(repository_name);
    let interactive_hooks = yaml_text.to_ascii_lowercase().contains("interactive: true");
    Ok(TemplateInspection {
        entry: CatalogEntry {
            id: format!("github-{}", repository_name.to_ascii_lowercase()),
            title: name.into(),
            summary: format!("Direct GitHub template from {}/{}", segments[0], repository_name),
            description: "Unvetted GitHub template resolved to an immutable commit.".into(),
            repository: url.trim_end_matches(".git").into(),
            commit,
            subdirectory: None,
            tags: vec!["GitHub".into(), "Unvetted".into()],
            platforms: vec!["windows".into(), "macos".into(), "linux".into()],
            manifest: Some(manifest),
            source: Some("external".into()),
            trusted: false,
        },
        source_path: None,
        azure_yaml_found: true,
        manifest_source: manifest_source.into(),
        interactive_hooks,
        warnings: vec![
            "Direct GitHub templates are unvetted. Review the pinned source and hooks before deploying."
                .into(),
        ],
    })
}

fn inferred_manifest(yaml: &serde_yaml::Value) -> TemplateManifest {
    let minimum_azd_version = yaml
        .get("requiredVersions")
        .and_then(|value| value.get("azd"))
        .and_then(serde_yaml::Value::as_str)
        .map(ToOwned::to_owned);
    TemplateManifest {
        schema_version: "1.0".into(),
        minimum_azd_version: minimum_azd_version.clone(),
        requires_interactive_terminal: false,
        prerequisites: vec![Prerequisite {
            id: "azd".into(),
            name: "Azure Developer CLI".into(),
            kind: "tool".into(),
            minimum_version: minimum_azd_version,
            required: true,
            install_provider: Some("manual".into()),
            install_id: None,
            documentation_url: Some(
                "https://learn.microsoft.com/azure/developer/azure-developer-cli/install-azd"
                    .into(),
            ),
        }],
        connections: vec![ConnectionRequirement {
            id: "azd".into(),
            name: "Azure Developer CLI".into(),
            required: true,
            scopes: vec![],
            reason: "Deploy the local azd project.".into(),
        }],
        configuration: Configuration::default(),
        permission_checks: vec![],
    }
}

pub fn entry_by_id(template_id: &str) -> AppResult<CatalogEntry> {
    bundled_catalog()?
        .entries
        .into_iter()
        .find(|entry| entry.id == template_id)
        .ok_or_else(|| AppError::Invalid(format!("Unknown bundled template: {template_id}")))
}

pub fn validate_external_snapshot(catalog: &Catalog) -> AppResult<()> {
    if catalog.schema_version != "1.0" || catalog.id == "bundled" {
        return Err(AppError::Invalid(
            "External catalog snapshot has an unsupported identity or schema version.".into(),
        ));
    }
    let expected_prefix = format!("external-{}.", catalog.id);
    for entry in &catalog.entries {
        if !entry.id.starts_with(&expected_prefix) {
            return Err(AppError::Invalid(format!(
                "External entry ID is not namespaced to its catalog: {}",
                entry.id
            )));
        }
        validate_entry(&CatalogFileEntryRef {
            id: &entry.id,
            repository: &entry.repository,
            commit: &entry.commit,
            subdirectory: entry.subdirectory.as_deref(),
        })?;
        if entry
            .manifest
            .as_ref()
            .is_some_and(|manifest| manifest.schema_version != "1.0")
        {
            return Err(AppError::Invalid(format!(
                "Unsupported manifest schema for {}.",
                entry.id
            )));
        }
        if let Some(manifest) = &entry.manifest {
            permissions::validate(&manifest.permission_checks)?;
        }
    }
    Ok(())
}

pub fn template_path(cache_path: &Path, entry: &CatalogEntry) -> AppResult<PathBuf> {
    let path = entry
        .subdirectory
        .as_ref()
        .map(|subdirectory| cache_path.join(subdirectory))
        .unwrap_or_else(|| cache_path.to_path_buf());
    if !path.join("azure.yaml").is_file() {
        return Err(AppError::Invalid(format!(
            "Pinned template does not contain azure.yaml at {}",
            path.display()
        )));
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::{bundled_catalog, inspect_local, normalize_catalog_url, same_origin};
    use std::fs;
    use tempfile::tempdir;
    use url::Url;

    #[test]
    fn bundled_entries_are_pinned_and_trusted() {
        let catalog = bundled_catalog().unwrap();
        assert!(!catalog.entries.is_empty());
        assert!(catalog
            .entries
            .iter()
            .all(|entry| entry.trusted && entry.commit.len() == 40));
    }

    #[test]
    fn catalog_urls_reject_embedded_or_query_credentials() {
        for url in [
            "https://token@catalog.example/azd-gui.catalog.json",
            "https://catalog.example/azd-gui.catalog.json?sig=secret",
            "https://catalog.example/azd-gui.catalog.json#token",
        ] {
            assert!(normalize_catalog_url(url).is_err());
        }
        assert!(normalize_catalog_url("https://catalog.example/azd-gui.catalog.json").is_ok());
    }

    #[test]
    fn redirect_origins_require_https_host_and_port_alignment() {
        let origin = Url::parse("https://catalog.example:8443/catalog.json").unwrap();
        assert!(same_origin(
            &origin,
            &Url::parse("https://catalog.example:8443/redirected.json").unwrap()
        ));
        assert!(!same_origin(
            &origin,
            &Url::parse("https://other.example:8443/redirected.json").unwrap()
        ));
        assert!(!same_origin(
            &origin,
            &Url::parse("http://catalog.example:8443/redirected.json").unwrap()
        ));
    }

    #[test]
    fn local_inspection_requires_azure_yaml() {
        let directory = tempdir().unwrap();
        assert!(inspect_local(directory.path().to_str().unwrap()).is_err());
        fs::write(directory.path().join("azure.yaml"), "name: sample").unwrap();
        let inspection = inspect_local(directory.path().to_str().unwrap()).unwrap();
        assert_eq!(inspection.manifest_source, "inferred");
        assert!(!inspection.entry.trusted);
    }

    #[test]
    fn local_manifest_rejects_unregistered_permission_checks() {
        let directory = tempdir().unwrap();
        fs::write(directory.path().join("azure.yaml"), "name: sample").unwrap();
        fs::write(
            directory.path().join("azd-gui.json"),
            r#"{
                "schemaVersion": "1.0",
                "prerequisites": [],
                "connections": [],
                "configuration": { "groups": [] },
                "permissionChecks": [{
                    "id": "manifest-command",
                    "name": "Unsafe check",
                    "provider": "azure",
                    "required": true,
                    "description": "Must be rejected before execution."
                }]
            }"#,
        )
        .unwrap();

        let error = inspect_local(directory.path().to_str().unwrap()).unwrap_err();
        assert!(error
            .to_string()
            .contains("Unsupported permission-check provider ID"));
    }
}
