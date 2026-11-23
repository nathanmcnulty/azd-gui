//! Read-only release metadata from the application-reviewed Reference repository.
use reqwest::Client;
use semver::Version;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

const API: &str = "https://api.github.com/repos/nathanmcnulty/azd-reference/git";
const SOURCE: &str = "https://github.com/nathanmcnulty/azd-reference";
const TAG_PREFIX: &str = "component/";
const DECLARATION: &str = "azd-component-update: ";
const MAX_TAGS: usize = 48;
const MAX_BYTES: usize = 128 * 1024;
const FRESH_FOR: Duration = Duration::from_secs(15 * 60);
const RETRY_AFTER: Duration = Duration::from_secs(30);

#[derive(Clone, Deserialize)]
struct Object {
    sha: String,
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Deserialize)]
struct Reference {
    #[serde(rename = "ref")]
    name: String,
    object: Object,
}

#[derive(Clone, Deserialize)]
struct Tag {
    tag: String,
    sha: String,
    object: Object,
    message: String,
    verification: Verification,
}

#[derive(Clone, Deserialize)]
struct Verification {
    verified: bool,
    reason: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Declaration {
    component_id: String,
    version: String,
    breaking: bool,
    #[serde(default, deserialize_with = "optional_note")]
    migration_notes: Option<String>,
}

fn optional_note<'de, D: serde::Deserializer<'de>>(value: D) -> Result<Option<String>, D::Error> {
    String::deserialize(value).map(Some)
}

struct Release {
    version: Version,
    revision: Option<String>,
    compatibility: Option<Declaration>,
}

type Releases = HashMap<String, Vec<Release>>;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Installed {
    id: String,
    version: String,
    source_repository: String,
    source_revision: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Update {
    id: String,
    state: &'static str,
    version: Option<String>,
    compatibility: &'static str,
    complete: bool,
    breaking_versions: Vec<String>,
    migration_notes: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Results {
    state: &'static str,
    checked_at: Option<String>,
    updates: Vec<Update>,
}

#[derive(Default)]
pub struct Cache(Mutex<CacheEntry>);

#[derive(Default)]
struct CacheEntry {
    last_attempt: Option<(Instant, bool)>,
    snapshot: Option<(String, Releases)>,
    tags: HashMap<String, Tag>,
}

fn should_fetch(last: Option<(Instant, bool)>, requested: Instant, force: bool) -> bool {
    last.is_none_or(|(finished, success)| {
        let age = requested.saturating_duration_since(finished);
        age >= if force || !success {
            RETRY_AFTER
        } else {
            FRESH_FOR
        }
    })
}

fn reviewed_source(source: &str) -> bool {
    let source = source.strip_suffix('/').unwrap_or(source);
    source
        .strip_suffix(".git")
        .unwrap_or(source)
        .eq_ignore_ascii_case(SOURCE)
}

fn valid_id(id: &str) -> bool {
    (3..=64).contains(&id.len())
        && id.as_bytes()[0].is_ascii_lowercase()
        && id.as_bytes()[id.len() - 1].is_ascii_alphanumeric()
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn valid_sha(sha: &str) -> bool {
    sha.len() == 40 && sha.bytes().all(|b| b.is_ascii_hexdigit())
}

fn declaration(message: &str, id: &str, version: &Version) -> Option<Declaration> {
    let mut declarations = message
        .lines()
        .filter_map(|line| line.strip_prefix(DECLARATION));
    let value: Declaration = serde_json::from_str(declarations.next()?).ok()?;
    if declarations.next().is_some()
        || value.component_id != id
        || Version::parse(&value.version).ok().as_ref() != Some(version)
        || value.migration_notes.as_ref().is_some_and(|notes| {
            notes.is_empty() || notes.chars().count() > 1000 || notes.chars().any(char::is_control)
        })
    {
        return None;
    }
    Some(value)
}

async fn read_json<T: DeserializeOwned>(client: &Client, route: &str) -> Option<T> {
    let mut response = client
        .get(format!("{API}/{route}"))
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?;
    if response.headers().contains_key(reqwest::header::LINK)
        || response
            .content_length()
            .is_some_and(|size| size > MAX_BYTES as u64)
    {
        return None;
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.ok()? {
        if bytes.len() + chunk.len() > MAX_BYTES {
            return None;
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).ok()
}

async fn fetch_releases(cache: &mut HashMap<String, Tag>) -> Option<Releases> {
    let client = Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(4))
        .timeout(Duration::from_secs(8))
        .user_agent("azd-gui-component-metadata")
        .build()
        .ok()?;
    let refs: Vec<Reference> = read_json(&client, "matching-refs/tags/component/").await?;
    if refs.len() > MAX_TAGS {
        return None;
    }
    // Reuse verified and unsigned tag objects while their refs remain in the feed.
    cache.retain(|sha, _| refs.iter().any(|item| item.object.sha == *sha));
    let mut releases = Releases::new();
    for item in refs {
        let Some(name) = item.name.strip_prefix("refs/tags/") else {
            continue;
        };
        let Some((id, version)) = name
            .strip_prefix(TAG_PREFIX)
            .and_then(|name| name.rsplit_once("/v"))
        else {
            continue;
        };
        let Ok(version) = Version::parse(version) else {
            continue;
        };
        if !valid_id(id) || !valid_sha(&item.object.sha) {
            continue;
        }
        let history = releases.entry(id.into()).or_default();
        if item.object.kind != "tag" {
            history.push(Release {
                version,
                revision: None,
                compatibility: None,
            });
            continue;
        }
        let tag = if let Some(tag) = cache.get(&item.object.sha) {
            tag.clone()
        } else {
            let tag: Tag = read_json(&client, &format!("tags/{}", item.object.sha)).await?;
            if tag.sha != item.object.sha {
                return None;
            }
            if tag.verification.verified || tag.verification.reason == "unsigned" {
                cache.insert(item.object.sha, tag.clone());
            }
            tag
        };
        let verified = tag.verification.verified
            && tag.tag == name
            && tag.object.kind == "commit"
            && valid_sha(&tag.object.sha);
        history.push(Release {
            revision: verified.then_some(tag.object.sha),
            compatibility: if verified {
                declaration(&tag.message, id, &version)
            } else {
                None
            },
            version,
        });
    }
    Some(releases)
}

fn update(installed: &Installed, releases: Option<&Releases>) -> Update {
    let mut result = Update {
        id: installed.id.clone(),
        state: "unavailable",
        version: None,
        compatibility: "unknown",
        complete: false,
        breaking_versions: Vec::new(),
        migration_notes: Vec::new(),
    };
    if !reviewed_source(&installed.source_repository) || !valid_id(&installed.id) {
        return result;
    }
    let Some(history) = releases.and_then(|releases| releases.get(&installed.id)) else {
        return result;
    };
    if !history.iter().any(|release| release.revision.is_some()) {
        return result;
    }
    let Ok(current) = Version::parse(&installed.version) else {
        return result;
    };
    let mut newer: Vec<_> = history
        .iter()
        .filter(|release| release.version.cmp_precedence(&current).is_gt())
        .collect();
    newer.sort_by(|a, b| a.version.cmp_precedence(&b.version));
    // A stable installation is only offered a stable update. Include intermediate
    // pre-release declarations in the interval for prerelease installations.
    let Some(latest) = newer.iter().rev().find(|release| {
        release.revision.is_some() && (!current.pre.is_empty() || release.version.pre.is_empty())
    }) else {
        result.state = "noNewer";
        return result;
    };
    result.state = "updateAvailable";
    result.version = Some(latest.version.to_string());
    // An exact release at the lower bound anchors coverage to this tag history.
    result.complete = history.iter().any(|release| {
        release.version == current
            && release
                .revision
                .as_ref()
                .is_some_and(|revision| revision.eq_ignore_ascii_case(&installed.source_revision))
    });
    for release in newer.iter().filter(|release| {
        !release.version.cmp_precedence(&latest.version).is_gt()
            && (!current.pre.is_empty() || release.version.pre.is_empty())
    }) {
        match &release.compatibility {
            Some(declaration) if declaration.breaking => {
                result.breaking_versions.push(release.version.to_string());
                if let Some(notes) = &declaration.migration_notes {
                    result.migration_notes.push(notes.clone());
                }
            }
            Some(_) => {}
            None => result.complete = false,
        }
    }
    result.compatibility = if !result.breaking_versions.is_empty() {
        "breaking"
    } else if result.complete {
        "noBreakingDeclared"
    } else {
        "unknown"
    };
    result
}

#[tauri::command]
pub async fn check_component_updates(
    cache: tauri::State<'_, Cache>,
    installed: Vec<Installed>,
    force: bool,
) -> crate::AppResult<Results> {
    let requested = Instant::now();
    let mut cache = cache.0.lock().await;
    if should_fetch(cache.last_attempt, requested, force) {
        let fetch =
            tokio::time::timeout(Duration::from_secs(18), fetch_releases(&mut cache.tags)).await;
        let success = if let Ok(Some(releases)) = fetch {
            cache.snapshot = Some((crate::timestamp::now(), releases));
            true
        } else {
            false
        };
        cache.last_attempt = Some((Instant::now(), success));
    }
    Ok(cache.results(&installed))
}

impl CacheEntry {
    fn results(&self, installed: &[Installed]) -> Results {
        let state = match (&self.snapshot, self.last_attempt) {
            (Some(_), Some((_, true))) => "fresh",
            (Some(_), _) => "stale",
            _ => "unavailable",
        };
        Results {
            state,
            checked_at: self.snapshot.as_ref().map(|(time, _)| time.clone()),
            updates: installed
                .iter()
                .take(128)
                .map(|item| update(item, self.snapshot.as_ref().map(|(_, releases)| releases)))
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn installed(source: &str) -> Installed {
        let lock: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/deployment-validation-consumer-lock.json"
        ))
        .unwrap();
        let mut component: Installed =
            serde_json::from_value(lock["components"][0].clone()).unwrap();
        component.source_repository = source.into();
        component
    }

    #[test]
    fn a_failed_refresh_retains_the_last_successful_snapshot_as_stale() {
        let mut cache = CacheEntry {
            last_attempt: Some((Instant::now(), false)),
            ..Default::default()
        };
        assert_eq!(cache.results(&[]).state, "unavailable");
        cache.snapshot = Some(("2026-11-23T10:00:00+00:00".into(), Releases::new()));
        let result = cache.results(&[]);
        assert_eq!(result.state, "stale");
        assert_eq!(
            result.checked_at.as_deref(),
            Some("2026-11-23T10:00:00+00:00")
        );
    }

    #[tokio::test]
    #[ignore = "requires anonymous network access to the reviewed Reference repository"]
    async fn reads_the_reviewed_reference_release_over_https() {
        let mut tags = HashMap::new();
        let releases = tokio::time::timeout(Duration::from_secs(18), fetch_releases(&mut tags))
            .await
            .expect("the approved source exceeded its deadline")
            .expect("the approved source was unavailable");
        let pin = installed(SOURCE);
        assert!(releases[&pin.id]
            .iter()
            .any(|release| release.version.to_string() == pin.version
                && release.revision.as_deref() == Some(pin.source_revision.as_str())));
    }

    fn release(version: &str, json: Option<&str>) -> Release {
        let version = Version::parse(version).unwrap();
        let compatibility = json.and_then(|json| {
            declaration(
                &format!("{DECLARATION}{json}"),
                "deployment-validation",
                &version,
            )
        });
        Release {
            version,
            revision: Some(installed(SOURCE).source_revision),
            compatibility,
        }
    }

    #[test]
    fn warns_about_a_skipped_breaking_release_even_when_other_coverage_is_missing() {
        let breaking =
            include_str!("../tests/fixtures/deployment-validation-component-update.json");
        let releases = Releases::from([(
            "deployment-validation".into(),
            vec![
                release("2.0.1", None),
                release("1.1.1", None),
                release("2.0.0", Some(&breaking.replace('\n', ""))),
            ],
        )]);
        let result = update(&installed(SOURCE), Some(&releases));
        assert_eq!(result.version.as_deref(), Some("2.0.1"));
        assert_eq!(result.compatibility, "breaking");
        assert_eq!(result.breaking_versions, ["2.0.0"]);
        assert!(result.migration_notes[0].contains("PowerShell 7.6"));
        assert!(!result.complete);
    }

    #[test]
    fn distinguishes_missing_metadata_and_untrusted_sources_from_explicit_declarations() {
        let unchanged =
            r#"{"componentId":"deployment-validation","version":"1.1.2","breaking":false}"#;
        let releases = Releases::from([(
            "deployment-validation".into(),
            vec![
                release("1.1.1", None),
                release("1.1.2", Some(unchanged)),
                release("2.0.0-preview", None),
            ],
        )]);
        let mut installed = installed(SOURCE);
        assert_eq!(
            update(&installed, Some(&releases)).compatibility,
            "noBreakingDeclared"
        );
        installed.source_revision = "b".repeat(40);
        assert_eq!(update(&installed, Some(&releases)).compatibility, "unknown");
        assert_eq!(update(&installed, None).state, "unavailable");
        installed.source_repository = "https://github.com/private/example".into();
        assert_eq!(update(&installed, Some(&releases)).state, "unavailable");
        assert!(release("1.1.3", Some(unchanged)).compatibility.is_none());
    }

    #[test]
    fn coalesces_in_flight_requests_and_bounds_retries_and_freshness() {
        let started = Instant::now();
        let finished = started + Duration::from_secs(2);
        assert!(should_fetch(None, started, false));
        assert!(!should_fetch(Some((finished, true)), started, true));
        assert!(!should_fetch(Some((finished, false)), started, false));
        assert!(should_fetch(
            Some((finished, false)),
            finished + RETRY_AFTER,
            false
        ));
        assert!(should_fetch(
            Some((finished, true)),
            finished + RETRY_AFTER,
            true
        ));
        assert!(!should_fetch(
            Some((finished, true)),
            finished + RETRY_AFTER,
            false
        ));
        assert!(should_fetch(
            Some((finished, true)),
            finished + FRESH_FOR,
            false
        ));
    }
}
