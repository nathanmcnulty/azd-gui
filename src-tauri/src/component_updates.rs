//! Read-only release metadata from the application-reviewed Reference repository.
use reqwest::Client;
use semver::Version;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

const API: &str = "https://api.github.com/repos/nathanmcnulty/azd-reference/git";
const SOURCE: &str = "https://github.com/nathanmcnulty/azd-reference";
const TAG_PREFIX: &str = "component/";
const DECLARATION: &str = "azd-component-update: ";
const MAX_REF_BYTES: usize = 1024 * 1024;
const MAX_TAG_BYTES: usize = 128 * 1024;
const MAX_TAG_REQUESTS: usize = 20;
const MAX_REQUESTS_PER_HOUR: usize = 40;
const REQUEST_WINDOW: Duration = Duration::from_secs(60 * 60);
const TAG_CONCURRENCY: usize = 4;
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
    unresolved: bool,
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
    history_incomplete: bool,
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
    requests: VecDeque<Instant>,
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

async fn read_json<T: DeserializeOwned>(client: &Client, route: &str, limit: usize) -> Option<T> {
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
            .is_some_and(|size| size > limit as u64)
    {
        return None;
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.ok()? {
        if bytes.len() + chunk.len() > limit {
            return None;
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).ok()
}

struct Candidate {
    name: String,
    id: String,
    version: Version,
    object: Object,
}

impl Candidate {
    fn from_ref(item: Reference) -> Option<Self> {
        let name = item.name.strip_prefix("refs/tags/")?;
        let (id, version) = name.strip_prefix(TAG_PREFIX)?.rsplit_once("/v")?;
        let version = Version::parse(version).ok()?;
        (valid_id(id) && valid_sha(&item.object.sha)).then(|| Self {
            name: name.into(),
            id: id.into(),
            version,
            object: item.object,
        })
    }

    fn release(self, tag: Option<&Tag>) -> Release {
        let tag = tag.filter(|tag| {
            tag.sha == self.object.sha
                && tag.tag == self.name
                && tag.object.kind == "commit"
                && valid_sha(&tag.object.sha)
        });
        let verified = tag.filter(|tag| tag.verification.verified);
        Release {
            revision: verified.map(|tag| tag.object.sha.clone()),
            unresolved: self.object.kind == "tag" && tag.is_none(),
            compatibility: verified
                .and_then(|tag| declaration(&tag.message, &self.id, &self.version)),
            version: self.version,
        }
    }
}

fn tag_requests<'a>(
    candidates: &'a [Candidate],
    cached: &HashMap<String, Tag>,
    installed: &[Installed],
    limit: usize,
) -> Vec<&'a str> {
    let mut by_component: std::collections::BTreeMap<&str, Vec<&Candidate>> = Default::default();
    for candidate in candidates {
        if candidate.object.kind == "tag" && !cached.contains_key(&candidate.object.sha) {
            by_component
                .entry(&candidate.id)
                .or_default()
                .push(candidate);
        }
    }
    let mut groups: Vec<_> = by_component.into_iter().collect();
    groups.sort_by_key(|(id, _)| {
        !installed
            .iter()
            .any(|pin| pin.id == *id && reviewed_source(&pin.source_repository))
    });
    for (_, group) in &mut groups {
        group.sort_by(|a, b| b.version.cmp_precedence(&a.version));
    }
    // Visit each component before spending another request on older history.
    let mut shas = Vec::new();
    for round in 0..limit {
        for (_, group) in &groups {
            if let Some(candidate) = group.get(round) {
                let sha = candidate.object.sha.as_str();
                if !shas.contains(&sha) {
                    shas.push(sha);
                }
                if shas.len() == limit {
                    return shas;
                }
            }
        }
    }
    shas
}

fn remaining_requests(requests: &mut VecDeque<Instant>, now: Instant) -> usize {
    requests.retain(|time| now.saturating_duration_since(*time) < REQUEST_WINDOW);
    MAX_REQUESTS_PER_HOUR.saturating_sub(requests.len())
}

async fn fetch_releases(cache: &mut CacheEntry, installed: &[Installed]) -> Option<Releases> {
    let budget = remaining_requests(&mut cache.requests, Instant::now());
    if budget == 0 {
        return None;
    }
    let client = Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(3))
        .user_agent("azd-gui-component-metadata")
        .build()
        .ok()?;
    cache.requests.push_back(Instant::now());
    let refs: Vec<Reference> =
        read_json(&client, "matching-refs/tags/component/", MAX_REF_BYTES).await?;
    let candidates: Vec<_> = refs.into_iter().filter_map(Candidate::from_ref).collect();
    let present: std::collections::HashSet<_> = candidates
        .iter()
        .map(|candidate| &candidate.object.sha)
        .collect();
    cache.tags.retain(|sha, _| present.contains(sha));
    let checks = tag_requests(
        &candidates,
        &cache.tags,
        installed,
        (budget - 1).min(MAX_TAG_REQUESTS),
    );
    let mut transient_tags = HashMap::new();
    for batch in checks.chunks(TAG_CONCURRENCY) {
        let mut jobs = tokio::task::JoinSet::new();
        for &sha in batch {
            let client = client.clone();
            let sha = sha.to_owned();
            cache.requests.push_back(Instant::now());
            jobs.spawn(async move {
                let tag: Option<Tag> =
                    read_json(&client, &format!("tags/{sha}"), MAX_TAG_BYTES).await;
                tag.filter(|tag| tag.sha == sha).map(|tag| (sha, tag))
            });
        }
        while let Some(result) = jobs.join_next().await {
            if let Ok(Some((sha, tag))) = result {
                if tag.verification.verified || tag.verification.reason == "unsigned" {
                    cache.tags.insert(sha, tag);
                } else {
                    transient_tags.insert(sha, tag);
                }
            }
        }
    }
    let mut releases = Releases::new();
    for candidate in candidates {
        let tag = cache
            .tags
            .get(&candidate.object.sha)
            .or_else(|| transient_tags.get(&candidate.object.sha));
        releases
            .entry(candidate.id.clone())
            .or_default()
            .push(candidate.release(tag));
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
        history_incomplete: false,
        breaking_versions: Vec::new(),
        migration_notes: Vec::new(),
    };
    if !reviewed_source(&installed.source_repository) || !valid_id(&installed.id) {
        return result;
    }
    let Some(history) = releases.and_then(|releases| releases.get(&installed.id)) else {
        return result;
    };

    let Ok(current) = Version::parse(&installed.version) else {
        return result;
    };
    let eligible = |release: &Release| !current.pre.is_empty() || release.version.pre.is_empty();
    result.history_incomplete = history.iter().any(|release| {
        release.unresolved
            && (release.version == current
                || (eligible(release) && release.version.cmp_precedence(&current).is_gt()))
    });
    if !history.iter().any(|release| release.revision.is_some()) {
        return result;
    }
    let mut newer: Vec<_> = history
        .iter()
        .filter(|release| release.version.cmp_precedence(&current).is_gt())
        .collect();
    newer.sort_by(|a, b| a.version.cmp_precedence(&b.version));
    // A stable installation is only offered a stable update. Include intermediate
    // pre-release declarations in the interval for prerelease installations.
    let Some(latest) = newer
        .iter()
        .rev()
        .find(|release| release.revision.is_some() && eligible(release))
    else {
        if !result.history_incomplete {
            result.state = "noNewer";
        }
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
        !release.version.cmp_precedence(&latest.version).is_gt() && eligible(release)
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
        let success = if let Some(releases) = fetch_releases(&mut cache, &installed).await {
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
        let releases = fetch_releases(&mut CacheEntry::default(), &[])
            .await
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
            unresolved: false,
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
    fn stable_upgrade_uses_the_stable_declaration_after_a_release_candidate() {
        let fixture = include_str!("../tests/fixtures/deployment-validation-component-update.json")
            .replace('\n', "");
        let prerelease = fixture.replace("2.0.0", "2.0.0-rc.1");
        let pin = installed(SOURCE);
        let mut releases = Releases::from([(
            pin.id.clone(),
            vec![
                release("1.1.1", None),
                release("2.0.0-rc.1", Some(&prerelease)),
                release("2.0.0", None),
            ],
        )]);
        let unknown = update(&pin, Some(&releases));
        assert_eq!(unknown.version.as_deref(), Some("2.0.0"));
        assert_eq!(unknown.compatibility, "unknown");
        releases.get_mut(&pin.id).unwrap()[2] = release("2.0.0", Some(&fixture));
        let known = update(&pin, Some(&releases));
        assert_eq!(known.compatibility, "breaking");
        assert_eq!(known.breaking_versions, ["2.0.0"]);
        assert!(known.migration_notes[0].contains("PowerShell 7.6"));
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

    fn candidate(id: &str, number: u32) -> Candidate {
        Candidate::from_ref(Reference {
            name: format!("refs/tags/component/{id}/v{number}.0.0"),
            object: Object {
                sha: format!("{number:040x}"),
                kind: "tag".into(),
            },
        })
        .unwrap()
    }

    #[test]
    fn large_tag_histories_share_budget_across_components_and_prioritize_installed_pins() {
        let mut candidates: Vec<_> = (1..=60)
            .map(|version| candidate("large-history", version))
            .collect();
        candidates.push(candidate("deployment-validation", 61));
        candidates.push(candidate("small-history", 62));
        let selected = tag_requests(&candidates, &HashMap::new(), &[installed(SOURCE)], 3);
        assert_eq!(
            selected,
            [
                format!("{:040x}", 61),
                format!("{:040x}", 60),
                format!("{:040x}", 62)
            ]
        );
        assert!(tag_requests(&candidates, &HashMap::new(), &[], 0).is_empty());
        let now = Instant::now();
        let mut requests = VecDeque::from(vec![now; MAX_REQUESTS_PER_HOUR]);
        assert_eq!(remaining_requests(&mut requests, now), 0);
        assert_eq!(
            remaining_requests(&mut requests, now + REQUEST_WINDOW),
            MAX_REQUESTS_PER_HOUR
        );
    }

    #[test]
    fn unreadable_tags_leave_only_the_affected_component_incomplete() {
        let unresolved = candidate("deployment-validation", 3).release(None);
        let mut other = installed(SOURCE);
        other.id = "small-history".into();
        let mut releases = Releases::from([
            (
                "deployment-validation".into(),
                vec![release("1.1.1", None), unresolved],
            ),
            (
                other.id.clone(),
                vec![release("1.1.1", None), release("2.0.0", None)],
            ),
        ]);
        let limited = update(&installed(SOURCE), Some(&releases));
        assert_eq!(limited.state, "unavailable");
        assert!(limited.history_incomplete);
        let unaffected = update(&other, Some(&releases));
        assert_eq!(unaffected.version.as_deref(), Some("2.0.0"));
        assert!(!unaffected.history_incomplete);

        releases
            .get_mut("deployment-validation")
            .unwrap()
            .push(release("2.0.0", None));
        let known = update(&installed(SOURCE), Some(&releases));
        assert_eq!(known.version.as_deref(), Some("2.0.0"));
        assert!(known.history_incomplete);
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
