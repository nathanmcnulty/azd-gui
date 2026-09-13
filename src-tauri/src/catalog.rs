use crate::{
    command_runner::{capture, git_command},
    error::{AppError, AppResult},
    models::{
        Catalog, CatalogEntry, CatalogFile, Configuration, ConnectionRequirement, Prerequisite,
        TemplateInspection, TemplateManifest,
    },
};
use once_cell::sync::Lazy;
use regex::Regex;
use sha2::{Digest, Sha256};
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

pub(crate) async fn pinned_git_options(url: &str) -> AppResult<Vec<String>> {
    let origin = Url::parse(url).map_err(|error| AppError::Url(error.to_string()))?;
    if origin.scheme() != "https" {
        return Err(AppError::Url(
            "Template repositories must use HTTPS.".into(),
        ));
    }
    reject_private_url_components(&origin, "Template repository URL")?;
    let addresses = public_origin_addresses(&origin).await?;
    let addresses = addresses
        .iter()
        .take(64)
        .map(|address| match address.ip() {
            IpAddr::V4(ip) => ip.to_string(),
            IpAddr::V6(ip) => format!("[{ip}]"),
        })
        .collect::<Vec<_>>()
        .join(",");
    Ok(vec![
        "-c".into(),
        "http.curloptResolve=".into(),
        "-c".into(),
        format!(
            "http.curloptResolve={}:{}:{addresses}",
            origin.host_str().unwrap(),
            origin.port_or_known_default().unwrap()
        ),
    ])
}

async fn restricted_http_client(origin: &Url) -> AppResult<reqwest::Client> {
    let origin = origin.clone();
    let host = origin
        .host_str()
        .ok_or_else(|| AppError::Url("URL must have a host.".into()))?
        .to_string();
    let addresses = public_origin_addresses(&origin).await?;
    reqwest::Client::builder()
        .no_proxy()
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
    use std::io::Read;
    require_payload_size(Some(std::fs::metadata(path)?.len()), None, limit, label)?;
    let mut text = String::new();
    std::fs::File::open(path)?
        .take(limit as u64 + 1)
        .read_to_string(&mut text)?;
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

fn normalized_sha256(value: &str, label: &str) -> AppResult<String> {
    if value.len() != 64 || !value.chars().all(|character| character.is_ascii_hexdigit()) {
        return Err(AppError::Invalid(format!(
            "{label} must be a 64-character SHA-256 digest."
        )));
    }
    Ok(value.to_ascii_lowercase())
}

fn external_manifest_digest(
    manifest: Option<&str>,
    manifest_sha256: Option<&str>,
) -> AppResult<Option<String>> {
    match (manifest, manifest_sha256) {
        (Some(_), Some(digest)) => normalized_sha256(digest, "External manifest digest").map(Some),
        (Some(_), None) => Err(AppError::Invalid(
            "External manifest sidecars require a SHA-256 digest in the catalog.".into(),
        )),
        (None, Some(_)) => Err(AppError::Invalid(
            "External manifest digest must name a manifest sidecar.".into(),
        )),
        (None, None) => Ok(None),
    }
}

fn require_manifest_digest(bytes: &[u8], expected_digest: &str) -> AppResult<()> {
    let actual_digest = format!("{:x}", Sha256::digest(bytes));
    if actual_digest == expected_digest {
        Ok(())
    } else {
        Err(AppError::Invalid(
            "External manifest bytes do not match the catalog SHA-256 digest.".into(),
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
            crate::manifest::validate(manifest)?;
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
            manifest_sha256: None,
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
        let manifest_sha256 =
            external_manifest_digest(entry.manifest.as_deref(), entry.manifest_sha256.as_deref())?;
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
            if !same_origin(&resolved_catalog_url, &manifest_url) {
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
            require_manifest_digest(
                &manifest_bytes,
                manifest_sha256
                    .as_deref()
                    .expect("external manifest digest was validated"),
            )?;
            let manifest: TemplateManifest = serde_json::from_slice(&manifest_bytes)?;
            if manifest.schema_version != "1.0" {
                return Err(AppError::Invalid(format!(
                    "Unsupported template manifest version: {}",
                    manifest.schema_version
                )));
            }
            crate::manifest::validate(&manifest)?;
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
            manifest_sha256,
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

/// The caller supplies a verified pinned checkout, scoped to the template subdirectory.
pub(crate) fn validate_external_checkout(entry: &CatalogEntry, source: &Path) -> AppResult<()> {
    let declared = entry.manifest.as_ref().ok_or_else(|| AppError::Invalid(
        "External gallery preparation requires a reviewed manifest sidecar matching the template's embedded azd-gui.json. Review the catalog or inspect the repository directly.".into()
    ))?;
    let digest = entry.manifest_sha256.as_deref().ok_or_else(|| {
        AppError::Invalid(
            "External gallery manifest has no reviewed digest. Review the catalog again.".into(),
        )
    })?;
    let inspection = inspect_local(
        source
            .to_str()
            .ok_or_else(|| AppError::Invalid("Template path is not valid Unicode.".into()))?,
    )?;
    if inspection.manifest_source != "embedded" {
        return Err(AppError::Invalid(
            "The pinned template must embed azd-gui.json matching the reviewed gallery sidecar. Review the catalog or inspect the repository directly.".into()
        ));
    }
    let text = read_bounded_local_text(
        &source.join("azd-gui.json"),
        TEMPLATE_MANIFEST_LIMIT,
        "azd-gui.json",
    )?;
    require_manifest_digest(text.as_bytes(), digest).map_err(|_| AppError::Invalid(
        "The pinned template's azd-gui.json does not match the reviewed gallery sidecar. Review the catalog again.".into()
    ))?;
    // Inspection adds interactive requirements detected from azure.yaml. A sidecar
    // must describe those requirements before approval, rather than weakening them.
    if serde_json::to_value(inspection.entry.manifest.as_ref())?
        != serde_json::to_value(Some(declared))?
    {
        return Err(AppError::Invalid(
            "The pinned template's effective manifest differs from the reviewed gallery contract, including detected interactive hooks. Review an updated catalog before preparation.".into()
        ));
    }
    Ok(())
}

pub fn inspect_local(path: &str) -> AppResult<TemplateInspection> {
    let source_binding = crate::local_source::inspect(Path::new(path))?;
    let canonical = PathBuf::from(&source_binding.source_path);
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
    let (mut manifest, manifest_source) = if embedded_path.is_file() {
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
    crate::manifest::validate(&manifest)?;
    let (interactive_hooks, windows_hooks, posix_hooks) = hook_characteristics(&yaml);
    manifest.requires_interactive_terminal |= interactive_hooks;
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
    crate::local_source::verify_for_path(&source_binding, &canonical)?;
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
            manifest_sha256: None,
            source: Some("local".into()),
            trusted: false,
        },
        source_path: Some(canonical.display().to_string()),
        source_binding: Some(source_binding),
        azure_yaml_found: true,
        manifest_source,
        interactive_hooks,
        warnings,
    })
}

pub async fn inspect_github(url: &str) -> AppResult<TemplateInspection> {
    inspect_github_revision(url, None).await
}

pub(crate) async fn inspect_github_at_revision(
    url: &str,
    revision: &str,
) -> AppResult<TemplateInspection> {
    if !COMMIT_REVISION.is_match(revision) {
        return Err(AppError::Invalid(
            "A full pinned GitHub revision is required.".into(),
        ));
    }
    inspect_github_revision(url, Some(revision)).await
}

async fn inspect_github_revision(
    url: &str,
    revision: Option<&str>,
) -> AppResult<TemplateInspection> {
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
    let commit = if let Some(revision) = revision {
        revision.to_string()
    } else {
        let directory = tempfile::tempdir()?;
        let mut args = pinned_git_options(url).await?;
        args.extend(["ls-remote".into(), url.into(), "HEAD".into()]);
        let output = capture(git_command(directory.path(), args), Duration::from_secs(30)).await?;
        if !output.success {
            return Err(AppError::Command(format!(
                "Could not resolve repository HEAD: {}",
                output.stderr
            )));
        }
        output
            .stdout
            .split_whitespace()
            .next()
            .filter(|value| {
                value.len() == 40 && value.chars().all(|character| character.is_ascii_hexdigit())
            })
            .ok_or_else(|| {
                AppError::Invalid("GitHub did not return a valid commit revision.".into())
            })?
            .to_string()
    };
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
    let (mut manifest, manifest_source) = if embedded_manifest.status().is_success() {
        let manifest_bytes =
            read_bounded_response(embedded_manifest, TEMPLATE_MANIFEST_LIMIT, "azd-gui.json")
                .await?;
        (
            serde_json::from_slice::<TemplateManifest>(&manifest_bytes)?,
            "embedded",
        )
    } else if embedded_manifest.status() == reqwest::StatusCode::NOT_FOUND {
        (inferred_manifest(&yaml), "inferred")
    } else {
        return Err(AppError::Invalid(format!(
            "Could not read the pinned template manifest (HTTP {}).",
            embedded_manifest.status()
        )));
    };
    if manifest.schema_version != "1.0" {
        return Err(AppError::Invalid(format!(
            "Unsupported template manifest version: {}",
            manifest.schema_version
        )));
    }
    crate::manifest::validate(&manifest)?;
    let name = yaml
        .get("name")
        .and_then(serde_yaml::Value::as_str)
        .unwrap_or(repository_name);
    let (interactive_hooks, _, _) = hook_characteristics(&yaml);
    manifest.requires_interactive_terminal |= interactive_hooks;
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
            manifest_sha256: None,
            source: Some("external".into()),
            trusted: false,
        },
        source_path: None,
        source_binding: None,
        azure_yaml_found: true,
        manifest_source: manifest_source.into(),
        interactive_hooks,
        warnings: vec![
            "Direct GitHub templates are unvetted. Review the pinned source and hooks before deploying."
                .into(),
        ],
    })
}

fn hook_characteristics(yaml: &serde_yaml::Value) -> (bool, bool, bool) {
    let mut containers = vec![yaml];
    if let Some(services) = yaml.get("services").and_then(serde_yaml::Value::as_mapping) {
        containers.extend(services.values());
    }
    if let Some(layers) = yaml.get("infra").and_then(|infra| infra.get("layers")) {
        match layers {
            serde_yaml::Value::Sequence(layers) => containers.extend(layers),
            serde_yaml::Value::Mapping(layers) => containers.extend(layers.values()),
            _ => {}
        }
    }
    let mut pending: Vec<_> = containers
        .into_iter()
        .filter_map(|item| item.get("hooks"))
        .collect();
    let (mut interactive, mut windows, mut posix) = (false, false, false);
    while let Some(node) = pending.pop() {
        match node {
            serde_yaml::Value::Mapping(entries) => {
                interactive |=
                    node.get("interactive").and_then(serde_yaml::Value::as_bool) == Some(true);
                windows |= node.get("windows").is_some();
                posix |= node.get("posix").is_some();
                pending.extend(entries.values());
            }
            serde_yaml::Value::Sequence(entries) => pending.extend(entries),
            _ => {}
        }
    }
    (interactive, windows, posix)
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
            condition: None,
        }],
        connections: vec![ConnectionRequirement {
            id: "azd".into(),
            name: "Azure Developer CLI".into(),
            required: true,
            scopes: vec![],
            reason: "Deploy the local azd project.".into(),
            condition: None,
        }],
        configuration: Configuration::default(),
        recommendation_profiles: vec![],
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

pub(crate) fn validate_external_entry_snapshot(entry: &CatalogEntry) -> AppResult<()> {
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
        crate::manifest::validate(manifest)?;
    }
    external_manifest_digest(
        entry.manifest.as_ref().map(|_| "persisted"),
        entry.manifest_sha256.as_deref(),
    )?;
    Ok(())
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
        validate_external_entry_snapshot(entry)?;
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
    #[test]
    fn hook_inspection_uses_yaml_structure_and_preserves_terminal_requirement() {
        let source = tempfile::tempdir().unwrap();
        std::fs::write(source.path().join("azure.yaml"), "name: hook-test\nhooks:\n  preprovision: {shell: pwsh, interactive: true, run: ./setup.ps1}\n").unwrap();
        let inspection = super::inspect_local(source.path().to_str().unwrap()).unwrap();
        assert!(inspection.interactive_hooks);
        assert!(
            inspection
                .entry
                .manifest
                .unwrap()
                .requires_interactive_terminal
        );
        let yaml = serde_yaml::from_str("name: sample\n# interactive: true\nmetadata: {windows: documentation}\nhooks: {preprovision: {run: 'echo interactive: true', interactive: false}}\n").unwrap();
        assert_eq!(super::hook_characteristics(&yaml), (false, false, false));
        let yaml = serde_yaml::from_str("services:\n  api:\n    hooks:\n      predeploy:\n        - windows: {interactive: true, shell: pwsh, run: ./setup.ps1}\ninfra:\n  layers:\n    - name: shared\n      hooks:\n        preprovision: {posix: {run: ./setup.sh}}\n").unwrap();
        assert_eq!(super::hook_characteristics(&yaml), (true, true, true));
    }

    use super::is_public_ip;
    use super::{
        bundled_catalog, external_manifest_digest, inspect_local, normalize_catalog_url,
        require_catalog_entry_count, require_manifest_digest, require_payload_size, same_origin,
        validate_external_snapshot, AZURE_YAML_LIMIT, MAX_CATALOG_ENTRIES,
    };
    use sha2::{Digest, Sha256};
    use std::{fs, net::IpAddr};
    use tempfile::tempdir;
    use url::Url;

    #[test]
    fn bundled_entries_are_pinned_and_trusted() {
        let catalog = bundled_catalog().unwrap();
        assert!(!catalog.entries.is_empty());
        assert!(catalog.entries.iter().all(|entry| entry.trusted
            && entry.commit.len() == 40
            && entry.manifest_sha256.is_none()));
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
        assert!(!same_origin(
            &origin,
            &Url::parse("https://catalog.example:9443/redirected.json").unwrap()
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
    fn external_manifest_sidecars_require_and_verify_a_digest() {
        assert!(external_manifest_digest(Some("manifest.json"), None).is_err());
        assert!(external_manifest_digest(None, Some("0".repeat(64).as_str())).is_err());

        let bytes = br#"{"schemaVersion":"1.0"}"#;
        let digest = format!("{:x}", Sha256::digest(bytes));
        let digest = external_manifest_digest(Some("manifest.json"), Some(&digest))
            .unwrap()
            .unwrap();
        assert!(require_manifest_digest(bytes, &digest).is_ok());
        assert!(require_manifest_digest(br#"{"schemaVersion":"changed"}"#, &digest).is_err());
    }

    #[test]
    fn external_checkout_requires_matching_embedded_bytes_and_effective_contract() {
        let root = tempdir().unwrap();
        let source = root.path().join("nested-template");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("azure.yaml"), "name: binding-test\n").unwrap();
        let mut entry = bundled_catalog().unwrap().entries.remove(0);
        entry.manifest = Some(serde_json::from_str(r#"{"schemaVersion":"1.0"}"#).unwrap());
        let bytes = serde_json::to_vec(entry.manifest.as_ref().unwrap()).unwrap();
        entry.manifest_sha256 = Some(format!("{:x}", Sha256::digest(&bytes)));
        // A manifest at the repository root cannot satisfy a subdirectory template.
        std::fs::write(root.path().join("azd-gui.json"), &bytes).unwrap();
        assert!(super::validate_external_checkout(&entry, &source).is_err());
        std::fs::write(source.join("azd-gui.json"), &bytes).unwrap();
        super::validate_external_checkout(&entry, &source).unwrap();

        // Even semantically equivalent reformatted bytes require a catalog update.
        std::fs::write(
            source.join("azd-gui.json"),
            [bytes.as_slice(), b"\n"].concat(),
        )
        .unwrap();
        assert!(super::validate_external_checkout(&entry, &source).is_err());
        std::fs::write(source.join("azd-gui.json"), &bytes).unwrap();
        entry
            .manifest
            .as_mut()
            .unwrap()
            .requires_interactive_terminal = true;
        assert!(super::validate_external_checkout(&entry, &source).is_err());
        entry
            .manifest
            .as_mut()
            .unwrap()
            .requires_interactive_terminal = false;
        std::fs::write(source.join("azure.yaml"), "name: binding-test\nservices:\n  web:\n    hooks:\n      predeploy:\n        interactive: true\n        run: echo prompt\n").unwrap();
        assert!(super::validate_external_checkout(&entry, &source).is_err());
        entry.manifest = None;
        assert!(super::validate_external_checkout(&entry, &source).is_err());
    }

    #[test]
    fn legacy_external_snapshot_without_manifest_digest_requires_review() {
        let mut catalog = bundled_catalog().unwrap();
        catalog.id = "legacy-review".into();
        for entry in &mut catalog.entries {
            entry.id = format!("external-legacy-review.{}", entry.id);
            entry.source = Some("external".into());
            entry.trusted = false;
        }
        assert!(validate_external_snapshot(&catalog).is_err());

        for entry in &mut catalog.entries {
            entry.manifest_sha256 = Some("0".repeat(64));
        }
        assert!(validate_external_snapshot(&catalog).is_ok());
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
        assert!(inspection.entry.manifest_sha256.is_none());
        assert_eq!(
            inspection.source_binding.unwrap().source_path,
            std::fs::canonicalize(directory.path())
                .unwrap()
                .display()
                .to_string()
        );
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
