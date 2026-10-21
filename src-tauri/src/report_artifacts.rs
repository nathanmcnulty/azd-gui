//! Optional read-only reports declared by the saved reviewed template contract.
//! Payloads are untrusted text, never commands or proof of operational verification.
use crate::{
    component_provenance::{bounded_bytes, safe_relative_path, ReadError, Root},
    error::{AppError, AppResult},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, path::Path};
const MAX_BYTES: u64 = 64 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactContract {
    pub schema_version: String,
    pub artifacts: Vec<ArtifactDeclaration>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactDeclaration {
    pub id: String,
    pub label: String,
    pub path: String,
    pub format: Format,
    pub stage: Stage,
    pub required: bool,
    pub sensitive: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Json,
    Text,
    Markdown,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Stage {
    Plan,
    Up,
    Provision,
    Deploy,
    Verify,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadContext {
    pub snapshot_digest: String,
    pub environment: Option<String>,
    pub revision: Option<String>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactRead {
    pub context: Option<ReadContext>,
    pub observed_at: String,
    pub artifacts: Vec<ArtifactResult>,
}
impl ArtifactRead {
    pub fn absent() -> Self {
        Self {
            context: None,
            observed_at: chrono::Utc::now().to_rfc3339(),
            artifacts: vec![],
        }
    }
}
#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ArtifactState {
    Available,
    Hidden,
    Missing,
    UnsafePath,
    TooLarge,
    Invalid,
    Unreadable,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactResult {
    pub declaration: ArtifactDeclaration,
    pub state: ArtifactState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<String>,
}
pub fn validate(contract: &ArtifactContract) -> AppResult<()> {
    let mut ids = HashSet::new();
    let mut paths = HashSet::new();
    let invalid = contract.schema_version != "1.0" || contract.artifacts.len() > 32 || contract.artifacts.iter().any(|a| {
        a.id.is_empty() || a.id.len() > 64 || !a.id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        || !ids.insert(a.id.as_str()) || !paths.insert(a.path.to_ascii_lowercase())
        || a.label.trim().is_empty() || a.label.len() > 160 || a.label.chars().any(char::is_control)
        || !safe_relative_path(&a.path)
        // Keep reports away from credentials/configuration and other workspace files.
        || !a.path.starts_with("reports/") || a.path.split('/').any(|s| s.starts_with('.'))
    });
    if invalid {
        Err(AppError::Invalid(
            "Unsupported report artifact contract.".into(),
        ))
    } else {
        Ok(())
    }
}
fn state(error: ReadError) -> ArtifactState {
    use crate::component_provenance::Reason;
    match error {
        ReadError::Missing => ArtifactState::Missing,
        ReadError::Unsupported(Reason::UnsafePath) => ArtifactState::UnsafePath,
        ReadError::Unsupported(Reason::LimitExceeded) => ArtifactState::TooLarge,
        _ => ArtifactState::Unreadable,
    }
}
pub fn read(
    root: &Path,
    contract: &ArtifactContract,
    context: ReadContext,
    reveal_id: Option<&str>,
) -> ArtifactRead {
    if validate(contract).is_err() {
        return ArtifactRead::absent();
    }
    let anchored = Root::open(root);
    let artifacts = contract
        .artifacts
        .iter()
        .map(|a| {
            let mut result = ArtifactResult {
                declaration: a.clone(),
                state: ArtifactState::Hidden,
                content: None,
                sha256: None,
                modified_at: None,
            };
            // Hidden content is not opened, parsed, hashed or returned at all.
            if a.sensitive && reveal_id != Some(a.id.as_str()) {
                return result;
            }
            let opened = match &anchored {
                Ok(root) => root.file(&a.path),
                Err(_) => {
                    result.state = ArtifactState::UnsafePath;
                    return result;
                }
            };
            let file = match opened {
                Ok(file) => file,
                Err(error) => {
                    result.state = state(error);
                    return result;
                }
            };
            result.modified_at = file
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339());
            let bytes = match bounded_bytes(file, MAX_BYTES) {
                Ok(bytes) => bytes,
                Err(error) => {
                    result.state = state(error);
                    return result;
                }
            };
            let text = match std::str::from_utf8(&bytes) {
                Ok(text)
                    if !text
                        .chars()
                        .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t')) =>
                {
                    text
                }
                _ => {
                    result.state = ArtifactState::Invalid;
                    return result;
                }
            };
            if matches!(a.format, Format::Json)
                && serde_json::from_str::<serde_json::Value>(text).is_err()
            {
                result.state = ArtifactState::Invalid;
                return result;
            }
            result.sha256 = Some(format!("{:x}", Sha256::digest(&bytes)));
            result.content = Some(text.to_owned());
            result.state = ArtifactState::Available;
            result
        })
        .collect();
    ArtifactRead {
        context: Some(context),
        observed_at: chrono::Utc::now().to_rfc3339(),
        artifacts,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn contract(path: &str, sensitive: bool) -> ArtifactContract {
        ArtifactContract {
            schema_version: "1.0".into(),
            artifacts: vec![ArtifactDeclaration {
                id: "plan".into(),
                label: "Plan".into(),
                path: path.into(),
                format: Format::Json,
                stage: Stage::Plan,
                required: true,
                sensitive,
            }],
        }
    }
    fn context() -> ReadContext {
        ReadContext {
            snapshot_digest: "a".repeat(64),
            environment: Some("test".into()),
            revision: Some("b".repeat(40)),
        }
    }
    fn fixture() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir(d.path().join("reports")).unwrap();
        d
    }
    #[test]
    fn reads_json_with_observed_context_and_hash_without_claiming_verification() {
        let d = fixture();
        std::fs::write(d.path().join("reports/plan.json"), "{\"ok\":true}").unwrap();
        let r = read(
            d.path(),
            &contract("reports/plan.json", false),
            context(),
            None,
        );
        assert_eq!(r.artifacts[0].state, ArtifactState::Available);
        assert_eq!(r.artifacts[0].sha256.as_ref().unwrap().len(), 64);
        assert!(r.artifacts[0].modified_at.is_some());
        assert_eq!(r.context.unwrap().environment.as_deref(), Some("test"));
    }
    #[test]
    fn sensitive_files_are_not_read_until_explicit_reveal_and_are_not_revealed_by_other_id() {
        let d = fixture();
        let c = contract("reports/absent.json", true);
        for reveal in [None, Some("other")] {
            let r = read(d.path(), &c, context(), reveal);
            assert_eq!(r.artifacts[0].state, ArtifactState::Hidden);
            assert!(r.artifacts[0].content.is_none());
            assert!(r.artifacts[0].modified_at.is_none());
            assert!(r.artifacts[0].sha256.is_none());
        }
        assert_eq!(
            read(d.path(), &c, context(), Some("plan")).artifacts[0].state,
            ArtifactState::Missing
        );
        std::fs::write(d.path().join("reports/absent.json"), "{}").unwrap();
        assert_eq!(
            read(d.path(), &c, context(), Some("plan")).artifacts[0].state,
            ArtifactState::Available
        );
    }
    #[test]
    fn rejects_unsafe_metadata_and_duplicate_paths() {
        for path in [
            "../secret",
            "reports/../secret",
            "/reports/a",
            "reports/a\\b",
            "reports/a:b",
            "reports/.env",
            "reports/CON",
            "reports/a.",
            "reports//a",
            "src/a",
            "reports/a\n",
        ] {
            assert!(validate(&contract(path, false)).is_err(), "{path}");
        }
        let mut c = contract("reports/a", false);
        c.artifacts.push(c.artifacts[0].clone());
        assert!(validate(&c).is_err());
        c.artifacts[1].id = "other".into();
        assert!(validate(&c).is_err());
        c.artifacts[1].path = "reports/A".into();
        assert!(validate(&c).is_err());
        c.artifacts.pop();
        c.schema_version = "2.0".into();
        assert!(validate(&c).is_err());
    }
    #[test]
    fn bounds_content_and_rejects_malformed_json_binary_controls_and_nonfiles() {
        let d = fixture();
        let p = d.path().join("reports/a");
        let c = contract("reports/a", false);
        for bytes in [
            b"{".to_vec(),
            vec![255],
            b"\"\x1b\"".to_vec(),
            b"\"\0\"".to_vec(),
        ] {
            std::fs::write(&p, bytes).unwrap();
            assert_eq!(
                read(d.path(), &c, context(), None).artifacts[0].state,
                ArtifactState::Invalid
            );
        }
        std::fs::write(&p, vec![b' '; MAX_BYTES as usize + 1]).unwrap();
        assert_eq!(
            read(d.path(), &c, context(), None).artifacts[0].state,
            ArtifactState::TooLarge
        );
        std::fs::remove_file(&p).unwrap();
        std::fs::create_dir(&p).unwrap();
        assert_eq!(
            read(d.path(), &c, context(), None).artifacts[0].state,
            ArtifactState::UnsafePath
        );
    }
    #[cfg(unix)]
    #[test]
    fn rejects_symlink_file_and_directory_escape() {
        use std::os::unix::fs::symlink;
        let d = fixture();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("a"), "{}").unwrap();
        symlink(outside.path().join("a"), d.path().join("reports/a")).unwrap();
        assert_eq!(
            read(d.path(), &contract("reports/a", false), context(), None).artifacts[0].state,
            ArtifactState::UnsafePath
        );
        symlink(outside.path(), d.path().join("reports/link")).unwrap();
        assert_eq!(
            read(
                d.path(),
                &contract("reports/link/a", false),
                context(),
                None
            )
            .artifacts[0]
                .state,
            ArtifactState::UnsafePath
        );
    }
    #[test]
    fn payload_is_not_interpreted_as_actions_or_validation_evidence() {
        let d = fixture();
        let p = d.path().join("reports/a");
        let text =
            "<script>alert(1)</script>\n[run](https://example.test/token)\nverification: passed";
        std::fs::write(&p, text).unwrap();
        let mut c = contract("reports/a", false);
        c.artifacts[0].format = Format::Markdown;
        let r = read(d.path(), &c, context(), None);
        assert_eq!(r.artifacts[0].content.as_deref(), Some(text));
    }
    #[test]
    fn rejects_executable_and_unknown_contract_fields() {
        for json in [
            r#"{"schemaVersion":"1.0","artifacts":[],"command":"pwsh"}"#,
            r#"{"schemaVersion":"1.0","artifacts":[],"verification":[]}"#,
        ] {
            assert!(serde_json::from_str::<ArtifactContract>(json).is_err());
        }
    }
}
