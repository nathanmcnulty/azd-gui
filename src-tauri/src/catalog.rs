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
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
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
const AZURE_YAML_LIMIT: usize = 2_000_000;
const TEMPLATE_MANIFEST_LIMIT: usize = 512_000;
const CATALOG_LIMIT: usize = 2_000_000;
const MAX_CATALOG_ENTRIES: usize = 250;
const MAX_CATALOG_MANIFEST_BYTES: usize = 10_000_000;

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

fn is_public_ipv4(address: Ipv4Addr) -> bool {
    let octets = address.octets();
    !(address.is_private()
        || address.is_loopback()
        || address.is_link_local()
        || address.is_broadcast()
        || address.is_documentation()
        || address.is_unspecified()
        || address.is_multicast()
        || octets[0] == 0
        || (octets[0] == 100 && (64..=127).contains(&octets[1]))
        || (octets[0] == 198 && matches!(octets[1], 18 | 19))
        || octets[0] >= 240)
}

fn is_public_ipv6(address: Ipv6Addr) -> bool {
    let segments = address.segments();
    if segments[..6] == [0, 0, 0, 0, 0, 0xffff] {
        return is_public_ipv4(Ipv4Addr::new(
            (segments[6] >> 8) as u8,
            segments[6] as u8,
            (segments[7] >> 8) as u8,
            segments[7] as u8,
        ));
    }
    !(address.is_loopback()
        || address.is_unspecified()
        || address.is_multicast()
        || (segments[0] & 0xfe00) == 0xfc00
        || (segments[0] & 0xffc0) == 0xfe80
        || (segments[0] & 0xffc0) == 0xfec0
        || (segments[0] == 0x2001 && segments[1] == 0x0db8))
}

fn is_public_ip(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => is_public_ipv4(address),
        IpAddr::V6(address) => is_public_ipv6(address),
    }
}

async fn public_origin_addresses(origin: &Url) -> AppResult<Vec<SocketAddr>> {
    let host = origin
        .host_str()
        .ok_or_else(|| AppError::Url("URL must have a host.".into()))?;
    let port = origin
        .port_or_known_default()
        .ok_or_else(|| AppError::Url("URL must have a known port.".into()))?;
    let mut addresses = tokio::net::lookup_host((host, port))
        .await
        .map_err(AppError::Io)?
        .collect::<Vec<_>>();
    addresses.sort_unstable();
    addresses.dedup();
    if addresses.is_empty() {
        return Err(AppError::Url(format!("{host} did not resolve.")));
    }
    if addresses.iter().any(|address| !is_public_ip(address.ip())) {
        return Err(AppError::Url(
            "Catalog network targets must resolve only to public IP addresses.".into(),
        ));
    }
    Ok(addresses)
}

pub async fn require_public_network_url(url: &str) -> AppResult<()> {
    let parsed = Url::parse(url).map_err(|error| AppError::Url(error.to_string()))?;
    if parsed.scheme() != "https" {
        return Err(AppError::Url(
            "Template repositories must use HTTPS.".into(),
        ));
    }
    reject_private_url_components(&parsed, "Template repository URL")?;
    public_origin_addresses(&parsed).await.map(|_| ())
}

async fn restricted_http_client(origin: &Url) -> AppResult<reqwest::Client> {
    let origin = origin.clone();
    let host = origin
        .host_str()
        .ok_or_else(|| AppError::Url("URL must have a host.".into()))?
        .to_string();
    let addresses = public_origin_addresses(&origin).await?;
    reqwest::Client::builder()
        .resolve_to_addrs(&host, &addresses)
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

fn require_payload_size(
    declared: Option<u64>,
    actual: Option<usize>,
    limit: usize,
    label: &str,
) -> AppResult<()> {
    if declared.is_some_and(|length| length > limit as u64)
        || actual.is_some_and(|length| length > limit)
    {
        Err(AppError::Invalid(format!(
            "{label} exceeds the {} KB limit.",
            limit / 1_000
        )))
    } else {
        Ok(())
    }
}

async fn read_bounded_response(
    mut response: reqwest::Response,
    limit: usize,
    label: &str,
) -> AppResult<Vec<u8>> {
    require_payload_size(response.content_length(), None, limit, label)?;
    let mut bytes = Vec::with_capacity(
        response
            .content_length()
            .and_then(|length| usize::try_from(length).ok())
            .unwrap_or(0)
            .min(limit),
    );
    while let Some(chunk) = response.chunk().await? {
        require_payload_size(
            None,
            Some(bytes.len().saturating_add(chunk.len())),
            limit,
            label,
        )?;
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn read_bounded_local_text(path: &Path, limit: usize, label: &str) -> AppResult<String> {
    require_payload_size(Some(std::fs::metadata(path)?.len()), None, limit, label)?;
    let text = std::fs::read_to_string(path)?;
    require_payload_size(None, Some(text.len()), limit, label)?;
    Ok(text)
}

fn require_catalog_entry_count(count: usize) -> AppResult<()> {
    if count > MAX_CATALOG_ENTRIES {
        Err(AppError::Invalid(format!(
            "Catalog has {count} entries; the limit is {MAX_CATALOG_ENTRIES}."
        )))
    } else {
        Ok(())
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
    require_catalog_entry_count(file.entries.len())?;
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
    let client = restricted_http_client(&url).await?;
    let response = client
        .get(url.clone())
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await?
        .error_for_status()?;
    require_response_origin(&url, &response)?;
    let resolved_catalog_url = response.url().clone();
    let bytes = read_bounded_response(response, CATALOG_LIMIT, "Catalog").await?;
    let file: CatalogFile = serde_json::from_slice(&bytes)?;
    require_catalog_entry_count(file.entries.len())?;
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
    let mut manifest_bytes_total = 0_usize;
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
            let manifest_bytes = read_bounded_response(
                response,
                TEMPLATE_MANIFEST_LIMIT,
                "External template manifest",
            )
            .await?;
            manifest_bytes_total = manifest_bytes_total.saturating_add(manifest_bytes.len());
            require_payload_size(
                None,
                Some(manifest_bytes_total),
                MAX_CATALOG_MANIFEST_BYTES,
                "Catalog manifest total",
            )?;
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
    let yaml_text = read_bounded_local_text(&azure_yaml, AZURE_YAML_LIMIT, "azure.yaml")?;
    let yaml: serde_yaml::Value = serde_yaml::from_str(&yaml_text)?;
    let name = yaml
        .get("name")
        .and_then(serde_yaml::Value::as_str)
        .unwrap_or("local-azd-project")
        .to_string();
    let embedded_path = canonical.join("azd-gui.json");
    let (manifest, manifest_source) = if embedded_path.is_file() {
        (
            serde_json::from_str(&read_bounded_local_text(
                &embedded_path,
                TEMPLATE_MANIFEST_LIMIT,
                "azd-gui.json",
            )?)?,
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
    let client = restricted_http_client(&raw_origin).await?;
    let azure_response = client
        .get(format!("{raw_base}/azure.yaml"))
        .timeout(Duration::from_secs(20))
        .send()
        .await?
        .error_for_status()?;
    require_response_origin(&raw_origin, &azure_response)?;
    require_payload_size(
        azure_response.content_length(),
        None,
        AZURE_YAML_LIMIT,
        "azure.yaml",
    )?;
    let yaml_bytes = read_bounded_response(azure_response, AZURE_YAML_LIMIT, "azure.yaml").await?;
    let yaml_text = std::str::from_utf8(&yaml_bytes)
        .map_err(|_| AppError::Invalid("azure.yaml must use UTF-8 text.".into()))?;
    let yaml: serde_yaml::Value = serde_yaml::from_str(yaml_text)?;
    let embedded_manifest = client
        .get(format!("{raw_base}/azd-gui.json"))
        .timeout(Duration::from_secs(20))
        .send()
        .await?;
    require_response_origin(&raw_origin, &embedded_manifest)?;
    let (manifest, manifest_source) = if embedded_manifest.status().is_success() {
        let manifest_bytes =
            read_bounded_response(embedded_manifest, TEMPLATE_MANIFEST_LIMIT, "azd-gui.json")
                .await?;
        (
            serde_json::from_slice::<TemplateManifest>(&manifest_bytes)?,
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
    require_catalog_entry_count(catalog.entries.len())?;
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
    use super::is_public_ip;
    use super::{
        bundled_catalog, inspect_local, normalize_catalog_url, require_catalog_entry_count,
        require_payload_size, same_origin, AZURE_YAML_LIMIT, MAX_CATALOG_ENTRIES,
    };
    use std::{fs, net::IpAddr};
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
    fn private_and_special_use_network_targets_are_rejected() {
        for address in [
            "127.0.0.1",
            "10.0.0.1",
            "100.64.0.1",
            "169.254.169.254",
            "192.168.1.1",
            "::1",
            "fc00::1",
            "fe80::1",
            "::ffff:127.0.0.1",
        ] {
            assert!(!is_public_ip(address.parse::<IpAddr>().unwrap()));
        }
        assert!(is_public_ip("20.42.1.1".parse::<IpAddr>().unwrap()));
        assert!(is_public_ip(
            "2603:1030:20e:3::23c".parse::<IpAddr>().unwrap()
        ));
    }

    #[test]
    fn direct_template_payload_limits_check_headers_and_actual_bytes() {
        assert!(require_payload_size(Some(513_000), None, 512_000, "manifest").is_err());
        assert!(require_payload_size(None, Some(513_000), 512_000, "manifest").is_err());
        assert!(require_payload_size(Some(512_000), Some(512_000), 512_000, "manifest").is_ok());
    }

    #[test]
    fn catalog_and_local_inspection_limits_are_enforced() {
        assert!(require_catalog_entry_count(MAX_CATALOG_ENTRIES).is_ok());
        assert!(require_catalog_entry_count(MAX_CATALOG_ENTRIES + 1).is_err());

        let directory = tempdir().unwrap();
        fs::write(
            directory.path().join("azure.yaml"),
            vec![b'x'; AZURE_YAML_LIMIT + 1],
        )
        .unwrap();
        assert!(inspect_local(directory.path().to_str().unwrap()).is_err());
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
