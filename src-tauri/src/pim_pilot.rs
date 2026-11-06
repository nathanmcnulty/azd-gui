//! Project-bound selected-role pilot host. The pinned template owns deployment.
use crate::{
    error::{AppError, AppResult},
    graph_session::{GraphBinding, GraphWorker},
    pim_plan, AppState,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    path::{Path, PathBuf},
};
use tauri::{AppHandle, State};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Input {
    project_path: String,
    environment: String,
    input: pim_plan::Input,
}

impl Input {
    fn binding(&self, state: &AppState) -> AppResult<GraphBinding> {
        if self.environment.is_empty()
            || self.environment.len() > 64
            || !self
                .environment
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
        {
            return Err(AppError::Invalid(
                "Choose a named project environment (letters, digits, _ or -).".into(),
            ));
        }
        let mut binding = self.input.clone().review()?.binding;
        binding.purpose = Some("pimPilot".into());
        binding
            .scopes
            .retain(|scope| scope != "AuthenticationContext.Read.All");
        binding
            .scopes
            .push("AuthenticationContext.ReadWrite.All".into());
        binding.scopes.extend([
            "Policy.ReadWrite.ConditionalAccess".into(),
            "RoleManagementPolicy.ReadWrite.Directory".into(),
        ]);
        binding.scopes.sort();
        let project = state
            .database
            .project_by_path(&self.project_path)?
            .ok_or_else(|| AppError::Invalid("Register a pilot workspace first.".into()))?;
        let snapshot = state.database.project_template_exact(&project.path)?;
        binding.requirements_digest = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&(
                pim_plan::SOURCE,
                &binding,
                self,
                &project.id,
                &project.template_id,
                &project.revision,
                snapshot.map(|s| s.digest)
            ))?)
        );
        binding.validate()?;
        Ok(binding)
    }
    fn directory(&self, state: &AppState) -> AppResult<PathBuf> {
        let project = state
            .database
            .project_by_path(&self.project_path)?
            .ok_or_else(|| AppError::Invalid("Select a registered PIM project.".into()))?;
        let snapshot = state.database.project_template_exact(&project.path)?;
        if project.template_id.as_deref() != Some("pim-pilot")
            && !snapshot.is_some_and(|s| {
                s.entry
                    .repository
                    .trim_end_matches(".git")
                    .trim_end_matches('/')
                    == "https://github.com/nathanmcnulty/azd-pim"
            })
        {
            return Err(AppError::Invalid(
                "Select a PIM project or register a dedicated pilot workspace.".into(),
            ));
        }
        let root = Path::new(&project.path);
        let target = root.join(".azure").join(&self.environment);
        reject_aliases(&target)?;
        Ok(target)
    }
}

#[tauri::command]
pub fn register_pim_pilot(
    state: State<'_, AppState>,
    project_path: String,
) -> AppResult<crate::models::ProjectSummary> {
    let root = Path::new(&project_path);
    reject_aliases(root)?;
    let root = std::fs::canonicalize(root)?;
    if !root.is_dir() {
        return Err(AppError::Invalid(
            "Choose an existing pilot workspace directory.".into(),
        ));
    }
    let path = root.display().to_string();
    if let Some(project) = state.database.project_by_path(&path)? {
        return Ok(project);
    }
    let project = crate::models::ProjectSummary {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Selected-role PIM pilot".into(),
        path,
        environment: Some("pilot".into()),
        template_id: Some("pim-pilot".into()),
        revision: Some(pim_plan::SOURCE.into()),
        last_operation: None,
        state: "attention".into(),
    };
    state.database.upsert_project(&project)?;
    Ok(project)
}

pub(crate) fn reject_aliases(target: &Path) -> AppResult<()> {
    for path in target.ancestors() {
        if let Ok(metadata) = std::fs::symlink_metadata(path) {
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if metadata.file_attributes() & 0x400 != 0 {
                    return Err(AppError::Invalid(
                        "Pilot paths cannot contain reparse points.".into(),
                    ));
                }
            }
            if metadata.file_type().is_symlink() {
                return Err(AppError::Invalid(
                    "Pilot paths cannot contain aliases.".into(),
                ));
            }
        }
    }
    Ok(())
}

// Local inspection never creates directories, opens Graph, or restores authority.
fn saved_directory(state: &AppState, project_path: &str, environment: &str) -> AppResult<PathBuf> {
    if environment.is_empty()
        || environment.len() > 64
        || !environment
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
    {
        return Err(AppError::Invalid("Choose a valid environment.".into()));
    }
    let project = state
        .database
        .projects()?
        .into_iter()
        .find(|p| p.path == project_path)
        .ok_or_else(|| AppError::Invalid("Select an exact registered project.".into()))?;
    let directory = Path::new(&project.path).join(".azure").join(environment);
    reject_aliases(&directory)?;
    Ok(directory)
}

// Pinned-handle reader reconciled from PR #164 (a7fdd02); no provenance UI imported.
fn unsafe_path() -> AppError {
    AppError::Invalid("Unsafe saved pilot path.".into())
}
// On Unix, every lookup is relative to an already-open directory descriptor.
// O_NOFOLLOW applies to every segment, so rename/link races cannot escape the root.
#[cfg(unix)]
#[derive(Debug)]
pub(crate) struct Root(std::fs::File);

#[cfg(unix)]
impl Root {
    pub(crate) fn open(path: &Path) -> Result<Self, AppError> {
        use std::os::{
            fd::{AsRawFd, FromRawFd},
            unix::fs::OpenOptionsExt,
        };
        if !path.is_absolute() {
            return Err(unsafe_path());
        }
        let mut directory = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open("/")
            .map_err(|_| unsafe_path())?;
        for component in path.components() {
            match component {
                std::path::Component::RootDir => {}
                std::path::Component::Normal(name) => {
                    use std::os::unix::ffi::OsStrExt;
                    let name =
                        std::ffi::CString::new(name.as_bytes()).map_err(|_| unsafe_path())?;
                    // SAFETY: the parent descriptor and NUL-terminated name are live;
                    // no creation flags are passed and the result is immediately owned.
                    let fd = unsafe {
                        libc::openat(
                            directory.as_raw_fd(),
                            name.as_ptr(),
                            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                        )
                    };
                    if fd < 0 {
                        return Err(AppError::Io(std::io::Error::last_os_error()));
                    }
                    directory = unsafe { std::fs::File::from_raw_fd(fd) };
                }
                _ => return Err(unsafe_path()),
            }
        }
        Ok(Self(directory))
    }

    pub(crate) fn for_write(_path: &Path) -> AppResult<Self> {
        Err(AppError::Invalid(
            "Use the supported Windows app for this workflow.".into(),
        ))
    }

    fn file(&self, relative: &str) -> Result<std::fs::File, AppError> {
        use std::os::fd::{AsRawFd, FromRawFd};
        let mut directory = self.0.try_clone().map_err(AppError::Io)?;
        let parts: Vec<_> = relative.split('/').collect();
        for (index, part) in parts.iter().enumerate() {
            let name = std::ffi::CString::new(*part).map_err(|_| unsafe_path())?;
            let last = index + 1 == parts.len();
            let flags = libc::O_RDONLY
                | libc::O_NOFOLLOW
                | libc::O_CLOEXEC
                | if last {
                    libc::O_NONBLOCK
                } else {
                    libc::O_DIRECTORY
                };
            // SAFETY: directory owns a live descriptor; name is NUL-terminated, flags
            // omit O_CREAT. A successful new descriptor is immediately owned by std::fs::File.
            let fd = unsafe { libc::openat(directory.as_raw_fd(), name.as_ptr(), flags) };
            if fd < 0 {
                let error = std::io::Error::last_os_error();
                return Err(
                    if matches!(error.raw_os_error(), Some(libc::ELOOP | libc::ENOTDIR)) {
                        unsafe_path()
                    } else {
                        AppError::Io(error)
                    },
                );
            }
            let file = unsafe { std::fs::File::from_raw_fd(fd) };
            if last {
                if !file.metadata().map_err(AppError::Io)?.is_file() {
                    return Err(unsafe_path());
                }
                return Ok(file);
            }
            directory = file;
        }
        Err(unsafe_path())
    }
}

// Windows directory handles deny delete sharing while descendants are opened/read.
// Keep every ancestor pinned, and inspect the opened handle itself for reparse points.
#[cfg(windows)]
#[derive(Debug)]
pub(crate) struct Root {
    path: std::path::PathBuf,
    _ancestors: Vec<std::fs::File>,
    _write_guard: Option<std::fs::File>,
}

#[cfg(windows)]
fn windows_open(path: &Path, directory: bool) -> Result<std::fs::File, AppError> {
    windows_open_shared(path, directory, 1)
}

#[cfg(windows)]
fn windows_open_shared(
    path: &Path,
    directory: bool,
    share: u32,
) -> Result<std::fs::File, AppError> {
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    let file = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(share)
        .custom_flags(0x00200000 | 0x02000000) // OPEN_REPARSE_POINT | BACKUP_SEMANTICS
        .open(path)
        .map_err(AppError::Io)?;
    let metadata = file.metadata().map_err(AppError::Io)?;
    if metadata.file_attributes() & 0x400 != 0
        || if directory {
            !metadata.is_dir()
        } else {
            !metadata.is_file()
        }
    {
        return Err(unsafe_path());
    }
    Ok(file)
}

#[cfg(windows)]
impl Root {
    pub(crate) fn open(path: &Path) -> Result<Self, AppError> {
        // Do not canonicalize away links before inspecting them.
        if !path.is_absolute() {
            return Err(unsafe_path());
        }
        let mut current = std::path::PathBuf::new();
        let mut ancestors = Vec::new();
        for component in path.components() {
            match component {
                std::path::Component::Prefix(prefix) => {
                    // Only local drive paths; UNC/device/network paths are outside this slice.
                    if !matches!(
                        prefix.kind(),
                        std::path::Prefix::Disk(_) | std::path::Prefix::VerbatimDisk(_)
                    ) {
                        return Err(unsafe_path());
                    }
                    current.push(component);
                }
                std::path::Component::RootDir | std::path::Component::Normal(_) => {
                    current.push(component);
                    let file = if let Some(parent) = ancestors.last() {
                        crate::windows_file_handles::open_existing(
                            parent,
                            component.as_os_str().to_str().ok_or_else(unsafe_path)?,
                            true,
                        )?
                    } else {
                        windows_open(&current, true)?
                    };
                    ancestors.push(file);
                }
                _ => return Err(unsafe_path()),
            }
        }
        Ok(Self {
            path: current,
            _ancestors: ancestors,
            _write_guard: None,
        })
    }

    pub(crate) fn for_write(path: &Path) -> AppResult<Self> {
        use std::os::windows::fs::MetadataExt;
        let checked = Self::open(path)?;
        let guard = crate::windows_file_handles::create_guard(
            checked._ancestors.last().ok_or_else(unsafe_path)?,
        )?;
        // A reparse conversion before guard installation fails this recheck;
        // afterward NTFS rejects conversion while the directory is nonempty.
        for file in &checked._ancestors {
            if file.metadata()?.file_attributes() & 0x400 != 0 {
                return Err(unsafe_path());
            }
        }
        // Original handles deny deletion throughout this transition. Their
        // pinned child directories and the final guard keep every path nonempty,
        // preventing in-place reparse conversion while opening writable shares.
        let mut ancestors = Vec::new();
        for ancestor in checked
            .path
            .ancestors()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
        {
            ancestors.push(windows_open_shared(ancestor, true, 3)?);
        }
        Ok(Self {
            path: checked.path.clone(),
            _ancestors: ancestors,
            _write_guard: Some(guard),
        })
    }

    fn file(&self, relative: &str) -> Result<std::fs::File, AppError> {
        crate::windows_file_handles::open_existing(
            self._ancestors.last().ok_or_else(unsafe_path)?,
            relative,
            false,
        )
    }
}

fn saved_json(directory: &Path, name: &str) -> AppResult<Option<serde_json::Value>> {
    let root = match Root::open(directory) {
        Ok(root) => root,
        Err(AppError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(None)
        }
        Err(error) => return Err(error),
    };
    let file = match root.file(name) {
        Ok(file) => file,
        Err(AppError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(None)
        }
        Err(error) => return Err(error),
    };
    if !file.metadata()?.is_file() || file.metadata()?.len() > 1024 * 1024 {
        return Err(AppError::Invalid(
            "Saved pilot file is unsupported or exceeds 1 MiB.".into(),
        ));
    }
    let mut bytes = Vec::new();
    file.take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 1024 * 1024 {
        return Err(AppError::Invalid("Saved pilot file exceeds limit.".into()));
    }
    Ok(Some(serde_json::from_slice(&bytes)?))
}

#[tauri::command]
pub fn save_pim_pilot_draft(state: State<'_, AppState>, input: Input) -> AppResult<()> {
    save_draft(&state, input)
}

fn save_draft(state: &AppState, input: Input) -> AppResult<()> {
    if !cfg!(windows) {
        return Err(AppError::Invalid(
            "Use the supported Windows app for this workflow.".into(),
        ));
    }
    saved_directory(state, &input.project_path, &input.environment)?;
    let _guard = state.operations.begin_environment_change()?;
    input.binding(state)?;
    let directory = input.directory(state)?;
    let mut pins = vec![Root::open(Path::new(&input.project_path))
        .map_err(|error| AppError::Invalid(format!("Open project root failed: {error}")))?];
    for path in [
        Path::new(&input.project_path).join(".azure"),
        directory.clone(),
    ] {
        match std::fs::create_dir(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => {
                return Err(AppError::Invalid(format!(
                    "Create pilot directory failed: {error}"
                )))
            }
        }
        pins.push(
            Root::open(&path).map_err(|error| {
                AppError::Invalid(format!("Pin pilot directory failed: {error}"))
            })?,
        );
    }
    let _write_root = Root::for_write(&directory)
        .map_err(|error| AppError::Invalid(format!("Write directory guard failed: {error}")))?;
    drop(pins);
    let lock_path = directory.join("azd-pim-gui.lock");
    reject_aliases(&lock_path)?;
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(0);
    }
    let lock = options
        .open(lock_path)
        .map_err(|error| AppError::Invalid(format!("Open pilot lock failed: {error}")))?;
    lock.try_lock()
        .map_err(|_| AppError::Invalid("Close the pilot session before saving a draft.".into()))?;
    let path = directory.join("azd-pim-gui-draft.json");
    reject_aliases(&path)?;
    let saved = serde_json::json!({"identity":{"account":input.input.account,"tenantId":input.input.tenant_id,"clientId":input.input.client_id},"configuration":input.input.configuration,"approvalRestorable":false});
    let temporary = directory.join(format!("azd-pim-draft-{}.tmp", uuid::Uuid::new_v4()));
    std::fs::write(&temporary, serde_json::to_vec_pretty(&saved)?)
        .map_err(|error| AppError::Invalid(format!("Write pilot draft failed: {error}")))?;
    std::fs::rename(temporary, path)
        .map_err(|error| AppError::Invalid(format!("Commit pilot draft failed: {error}")))?;
    Ok(())
}

#[tauri::command]
pub fn load_pim_pilot(
    state: State<'_, AppState>,
    project_path: String,
    environment: String,
) -> AppResult<Option<serde_json::Value>> {
    load_draft(&state, &project_path, &environment)
}

fn load_draft(
    state: &AppState,
    project_path: &str,
    environment: &str,
) -> AppResult<Option<serde_json::Value>> {
    let directory = saved_directory(state, project_path, environment)?;
    let saved = saved_json(&directory, "azd-pim-gui-draft.json")?;
    saved.map(|saved| {
        let identity = saved.get("identity").ok_or_else(unsafe_path)?;
        let input: pim_plan::Input = serde_json::from_value(serde_json::json!({"reviewId":uuid::Uuid::new_v4().to_string(), "account":identity.get("account"), "tenantId":identity.get("tenantId"), "clientId":identity.get("clientId"), "configuration":saved.get("configuration")}))?;
        input.clone().review()?;
        Ok(serde_json::json!({"identity":identity,"configuration":input.configuration}))
    }).transpose()
}

fn pending_count(state: &serde_json::Value) -> usize {
    let count = |section: &str, keys: &[&str]| {
        state
            .get(section)
            .and_then(|v| v.as_object())
            .map_or(0, |entries| {
                entries
                    .values()
                    .filter(|v| {
                        keys.is_empty()
                            || keys
                                .iter()
                                .any(|key| v.get(key).is_some_and(|v| !v.is_null() && v != false))
                    })
                    .count()
            })
    };
    count("pendingRoleRules", &[])
        + count("contexts", &["pendingIntent"])
        + count(
            "conditionalAccessPolicies",
            &["pendingCreate", "pendingIntent"],
        )
}

#[tauri::command]
pub fn select_pim_environment(
    state: State<'_, AppState>,
    project_path: String,
    environment: String,
) -> AppResult<()> {
    saved_directory(&state, &project_path, &environment)?;
    let mut project = state
        .database
        .project_by_path(&project_path)?
        .ok_or_else(unsafe_path)?;
    let snapshot = state.database.project_template_exact(&project.path)?;
    if project.template_id.as_deref() != Some("pim-pilot")
        && !snapshot.is_some_and(|s| {
            s.entry
                .repository
                .trim_end_matches(".git")
                .trim_end_matches('/')
                == "https://github.com/nathanmcnulty/azd-pim"
        })
    {
        return Err(unsafe_path());
    }
    project.environment = Some(environment);
    state.database.upsert_project(&project)
}

#[tauri::command]
pub fn inspect_pim_pilot(
    state: State<'_, AppState>,
    project_path: String,
    environment: String,
    reveal: bool,
) -> AppResult<serde_json::Value> {
    let directory = saved_directory(&state, &project_path, &environment)?;
    inspect_saved(&directory, reveal)
}

fn inspect_saved(directory: &Path, reveal: bool) -> AppResult<serde_json::Value> {
    let state = saved_json(directory, "azd-pim-state.json")?;
    let receipt = saved_json(directory, "azd-pim-gui-receipt.json")?;
    let draft = saved_json(directory, "azd-pim-gui-draft.json")?;
    let mut attempts = Vec::new();
    if directory.exists() {
        let entries: Vec<_> = std::fs::read_dir(directory)?
            .take(2049)
            .collect::<Result<_, _>>()?;
        if entries.len() > 2048 {
            return Err(AppError::Invalid(
                "Pilot directory exceeds inspection limit.".into(),
            ));
        }
        for entry in entries {
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Some(id) = name
                .strip_prefix("azd-pim-gui-")
                .and_then(|v| v.strip_suffix(".json"))
            {
                if uuid::Uuid::parse_str(id).is_ok() {
                    attempts.push(name);
                }
            }
            if attempts.len() > 256 {
                return Err(AppError::Invalid(
                    "Too many saved pilot attempts to inspect.".into(),
                ));
            }
        }
    }
    attempts.sort();
    Ok(serde_json::json!({
        "historical": true, "approvalRestorable": false, "hasDraft": draft.is_some(),
        "hasState": state.is_some(), "pendingWrites": state.as_ref().map_or(0, pending_count),
        "attemptCount": attempts.len(), "lastStartedAt": receipt.as_ref().and_then(|v|v.get("startedAt")).and_then(|v|v.as_str()).and_then(|v|chrono::DateTime::parse_from_rfc3339(v).ok()).map(|v|v.to_rfc3339()), "lastStatus": receipt.as_ref().and_then(|v| v.get("status")).and_then(|v| v.as_str()),
        "details": if reveal { Some(serde_json::json!({"state":state,"receipt":receipt,"attempts":attempts})) } else { None }
    }))
}

#[tauri::command]
pub fn review_pim_pilot(state: State<'_, AppState>, input: Input) -> AppResult<GraphBinding> {
    let binding = input.binding(&state)?;
    input.directory(&state)?;
    Ok(binding)
}

#[tauri::command]
pub async fn connect_pim_pilot(
    app: AppHandle,
    state: State<'_, AppState>,
    input: Input,
    expected_digest: String,
) -> AppResult<serde_json::Value> {
    let binding = input.binding(&state)?;
    if binding.requirements_digest != expected_digest {
        return Err(AppError::Invalid("Pilot configuration changed.".into()));
    }
    let directory = input.directory(&state)?;
    let ticket = state.graph_sessions.begin(&binding)?;
    let approved = app.dialog().message(format!("Sign in for the selected-role pilot.\nAccount: {}\nTenant: {}\nProject: {}\nEnvironment: {}\nPermissions: {}\n\nThis session can write core PIM controls only after a separate application approval. Stop if new consent is requested; consent is not authorized by this workflow.", binding.account, binding.tenant_id, input.project_path, input.environment, binding.scopes.join(", ")))
        .title("Review pilot sign-in").kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom("Sign in and recheck".into(), "Cancel".into())).blocking_show();
    if !approved {
        state.graph_sessions.invalidate_ticket(ticket);
        return Err(AppError::Invalid("Pilot sign-in cancelled.".into()));
    }
    let _guard = state.operations.begin_preparation()?;
    let worker = GraphWorker::spawn(binding.clone()).await?;
    state.graph_sessions.install(ticket, worker.clone()).await?;
    let result = async {
        state.graph_sessions.ensure_current(ticket, &worker)?;
        worker.authenticate().await?;
        worker.probe().await?;
        let result = worker.pilot(false, serde_json::json!({"directory": directory, "configuration":input.input.configuration})).await?;
        state.graph_sessions.ensure_current(ticket, &worker)?;
        Ok(result)
    }.await;
    if result.is_err() {
        state.graph_sessions.invalidate_worker(&worker);
        worker.close().await;
    }
    result
}

#[tauri::command]
pub async fn run_pim_pilot(
    app: AppHandle,
    state: State<'_, AppState>,
    input: Input,
    expected_digest: String,
    approved_digest: Option<String>,
) -> AppResult<serde_json::Value> {
    let binding = input.binding(&state)?;
    if binding.requirements_digest != expected_digest {
        return Err(AppError::Invalid(
            "Pilot identity or configuration changed.".into(),
        ));
    }
    let directory = input.directory(&state)?;
    let worker = state.graph_sessions.pilot_worker(&expected_digest)?;
    let _guard = state.operations.begin_preparation()?;
    let parameters = serde_json::json!({"directory":directory,"configuration":input.input.configuration,"approvedDigest":approved_digest});
    if let Some(digest) = approved_digest {
        // The native host retrieves the exact worker review before prompting.
        let current = worker.pilot(false, parameters.clone()).await?;
        if current.get("digest").and_then(|d| d.as_str()) != Some(&digest) {
            return Err(AppError::Invalid(
                "Live state or ownership changed. Recheck and review the new proposal.".into(),
            ));
        }
        let approved = app.dialog().message(format!("Apply the reviewed selected-role PIM pilot?\nTenant: {}\nAccount: {}\nProject: {}\nEnvironment: {}\nReview: {}\n\nThis can create/adopt authentication contexts and enabled Conditional Access policies, replace selected role-context links and remove legacy activation MFA only where shown in the review. Existing objects are authorized only by this exact review. Cancellation can leave partial or uncertain changes; recheck before another approval.", binding.tenant_id, binding.account, input.project_path, input.environment, digest))
            .title("Approve PIM pilot application").kind(MessageDialogKind::Warning)
            .buttons(MessageDialogButtons::OkCancelCustom("Apply reviewed pilot".into(), "Cancel".into())).blocking_show();
        if !approved {
            state.graph_sessions.invalidate_worker(&worker);
            worker.close().await;
            return Err(AppError::Invalid(
                "Pilot application cancelled. Reconnect and review before applying.".into(),
            ));
        }
        worker.pilot(true, parameters).await
    } else {
        worker.pilot(false, parameters).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{models::ProjectSummary, persistence::Database};
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };

    fn fixture() -> (tempfile::TempDir, AppState, Input) {
        let root = tempfile::tempdir().unwrap();
        let state = AppState {
            database: Arc::new(Database::open(&root.path().join("state.sqlite")).unwrap()),
            operations: Default::default(),
            environment_writes: tokio::sync::Mutex::new(()),
            template_cache: root.path().join("cache"),
            inspected_manifests: Mutex::new(HashMap::new()),
            reviewed_contracts: Mutex::new(HashMap::new()),
            catalog_reviews: Default::default(),
            execution_approvals: Default::default(),
            tool_install_approvals: Default::default(),
            graph_sessions: Default::default(),
        };
        let path = root.path().display().to_string();
        state
            .database
            .upsert_project(&ProjectSummary {
                id: "pilot".into(),
                name: "Pilot".into(),
                path: path.clone(),
                environment: Some("pilot".into()),
                template_id: Some("pim-pilot".into()),
                revision: Some(pim_plan::SOURCE.into()),
                last_operation: None,
                state: "attention".into(),
            })
            .unwrap();
        let input = serde_json::from_value(serde_json::json!({"projectPath":path,"environment":"pilot","input":{
            "reviewId":"99999999-9999-4999-8999-999999999999","account":"operator@example.test","tenantId":"11111111-1111-4111-8111-111111111111",
            "clientId":"22222222-2222-4222-8222-222222222222","configuration":{"privilegedRoleIds":["33333333-3333-4333-8333-333333333333"],"lessPrivilegedRoleIds":[],
            "privilegedAuthenticationProfile":"mfa","lessPrivilegedAuthenticationProfile":"mfa","privilegedDeviceRequirement":"none","lessPrivilegedDeviceRequirement":"none"}}})).unwrap();
        (root, state, input)
    }

    #[test]
    fn pilot_host_binds_project_environment_identity_configuration_and_registration() {
        let (_root, state, input) = fixture();
        let original = input.binding(&state).unwrap();
        assert_eq!(original.purpose.as_deref(), Some("pimPilot"));
        assert_eq!(original.scopes.len(), 5);
        assert!(input.directory(&state).is_ok());
        for change in ["environment", "tenant", "config"] {
            let mut changed = input.clone();
            match change {
                "environment" => changed.environment = "another".into(),
                "tenant" => changed.input.tenant_id = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".into(),
                _ => changed.input.configuration.preview_existing_objects = true,
            }
            assert_ne!(
                original.requirements_digest,
                changed.binding(&state).unwrap().requirements_digest
            );
        }
        let mut project = state
            .database
            .project_by_path(&input.project_path)
            .unwrap()
            .unwrap();
        project.revision = Some("changed".into());
        state.database.upsert_project(&project).unwrap();
        assert_ne!(
            original.requirements_digest,
            input.binding(&state).unwrap().requirements_digest
        );
        project.template_id = Some("unrelated".into());
        state.database.upsert_project(&project).unwrap();
        assert!(input.directory(&state).is_err());
    }

    #[test]
    fn pilot_host_rejects_unregistered_alias_paths_and_environment_traversal() {
        let (root, state, input) = fixture();
        for environment in ["", "../pilot", "a/b", "a\\b", "pilot.", "C:", "a b"] {
            let mut changed = input.clone();
            changed.environment = environment.into();
            assert!(changed.binding(&state).is_err());
        }
        let mut changed = input.clone();
        changed.project_path = root.path().join("other").display().to_string();
        assert!(changed.binding(&state).is_err());
        assert!(changed.directory(&state).is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(root.path(), root.path().join("alias")).unwrap();
            assert!(reject_aliases(&root.path().join("alias/.azure/pilot")).is_err());
        }
    }
    #[test]
    fn saved_overview_is_local_private_and_never_restores_authority() {
        let (root, state, input) = fixture();
        let directory = saved_directory(&state, &input.project_path, "pilot").unwrap();
        let empty = inspect_saved(&directory, false).unwrap();
        assert_eq!(empty["attemptCount"], 0);
        assert!(!directory.exists());
        std::fs::create_dir_all(&directory).unwrap();
        let checkpoint = serde_json::json!({"pendingRoleRules":{"role":{}},"contexts":{"privileged":{"pendingIntent":{}}},"conditionalAccessPolicies":{"policy":{"pendingCreate":{}}}});
        std::fs::write(directory.join("azd-pim-state.json"), checkpoint.to_string()).unwrap();
        std::fs::write(directory.join("azd-pim-gui-receipt.json"),r#"{"status":"partialOrUncertain","account":"private@example.test","digest":"old-approval"}"#).unwrap();
        std::fs::write(
            directory.join("azd-pim-gui-33333333-3333-4333-8333-333333333333.json"),
            "{}",
        )
        .unwrap();
        let overview = inspect_saved(&directory, false).unwrap();
        assert_eq!(overview["pendingWrites"], 3);
        assert_eq!(overview["attemptCount"], 1);
        assert_eq!(overview["approvalRestorable"], false);
        assert!(overview["details"].is_null());
        assert!(!overview.to_string().contains("private@example.test"));
        assert!(inspect_saved(&directory, true)
            .unwrap()
            .to_string()
            .contains("private@example.test"));
        std::fs::write(
            directory.join("azd-pim-state.json"),
            vec![b' '; 1024 * 1024 + 1],
        )
        .unwrap();
        assert!(inspect_saved(&directory, false).is_err());
        assert!(saved_directory(&state, &input.project_path, "../pilot").is_err());
        assert!(saved_directory(
            &state,
            &root.path().join("unknown").display().to_string(),
            "pilot"
        )
        .is_err());
    }
    #[test]
    #[cfg(windows)]
    fn local_draft_round_trip_preserves_recovery_and_restores_no_approval() {
        let (_root, state, input) = fixture();
        save_draft(&state, input.clone()).unwrap();
        let directory = input.directory(&state).unwrap();
        let checkpoint = r#"{"pendingRoleRules":{"selected-role":{}}}"#;
        std::fs::write(directory.join("azd-pim-state.json"), checkpoint).unwrap();
        let mut changed = input.clone();
        changed.input.configuration.preview_existing_objects = true;
        save_draft(&state, changed).unwrap();
        assert_eq!(
            std::fs::read_to_string(directory.join("azd-pim-state.json")).unwrap(),
            checkpoint
        );
        let restored = load_draft(&state, &input.project_path, &input.environment)
            .unwrap()
            .unwrap();
        assert_eq!(restored["configuration"]["previewExistingObjects"], true);
        assert!(restored.get("digest").is_none());
        assert!(restored.get("approval").is_none());
        std::fs::write(
            directory.join("azd-pim-gui-draft.json"),
            r#"{"identity":{},"configuration":{}}"#,
        )
        .unwrap();
        assert!(load_draft(&state, &input.project_path, &input.environment).is_err());
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            let path = directory.join("azd-pim-state.json");
            let _exclusive = std::fs::OpenOptions::new()
                .read(true)
                .share_mode(0)
                .open(path)
                .unwrap();
            assert!(inspect_saved(&directory, false).is_err());
        }
    }
    #[test]
    #[cfg(windows)]
    fn writer_guard_allows_atomic_reports_and_blocks_reparse_rename_and_delete() {
        use std::{
            ffi::c_void,
            os::windows::{fs::OpenOptionsExt, io::AsRawHandle},
        };
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn DeviceIoControl(
                device: *mut c_void,
                code: u32,
                input: *const c_void,
                input_size: u32,
                output: *mut c_void,
                output_size: u32,
                returned: *mut u32,
                overlapped: *mut c_void,
            ) -> i32;
        }
        fn mountpoint(path: &Path, target: &Path) -> bool {
            let file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .share_mode(3)
                .custom_flags(0x02000000 | 0x00200000)
                .open(path)
                .unwrap();
            let print = target.display().to_string();
            let substitute = format!(r"\??\{}", print.trim_start_matches(r"\\?\"));
            let sub: Vec<u16> = substitute.encode_utf16().collect();
            let display: Vec<u16> = print.encode_utf16().collect();
            let length = 8 + (sub.len() + 1 + display.len() + 1) * 2;
            let mut buffer = Vec::new();
            buffer.extend_from_slice(&0xA0000003u32.to_le_bytes());
            buffer.extend_from_slice(&(length as u16).to_le_bytes());
            buffer.extend_from_slice(&0u16.to_le_bytes());
            for value in [
                0,
                (sub.len() * 2) as u16,
                ((sub.len() + 1) * 2) as u16,
                (display.len() * 2) as u16,
            ] {
                buffer.extend_from_slice(&value.to_le_bytes());
            }
            for value in sub.into_iter().chain([0]).chain(display).chain([0]) {
                buffer.extend_from_slice(&value.to_le_bytes());
            }
            let mut returned = 0;
            // SAFETY: the mount-point buffer is complete and live; synchronous call.
            unsafe {
                DeviceIoControl(
                    file.as_raw_handle(),
                    0x000900A4,
                    buffer.as_ptr().cast(),
                    buffer.len() as u32,
                    std::ptr::null_mut(),
                    0,
                    &mut returned,
                    std::ptr::null_mut(),
                ) != 0
            }
        }
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let baseline = root.path().join("unguarded");
        std::fs::create_dir(&baseline).unwrap();
        assert!(
            mountpoint(&baseline, outside.path()),
            "The baseline must permit an ordinary directory junction"
        );
        assert!(Root::open(&baseline).is_err());
        std::fs::remove_dir(&baseline).unwrap();
        let directory = root.path().join("reports");
        std::fs::create_dir(&directory).unwrap();
        {
            let guard = Root::for_write(&directory).unwrap();
            assert!(!mountpoint(&directory, outside.path()));
            assert!(std::fs::rename(&directory, root.path().join("moved")).is_err());
            assert!(std::fs::remove_dir(&directory).is_err());
            std::fs::write(directory.join("report.json"), "old report").unwrap();
            std::fs::write(directory.join("report.tmp"), "new report").unwrap();
            std::fs::rename(directory.join("report.tmp"), directory.join("report.json")).unwrap();
            assert_eq!(
                std::fs::read_to_string(directory.join("report.json")).unwrap(),
                "new report"
            );
            // Final reads remain handle-relative even while writable ancestors are held.
            assert!(guard.file("report.json").is_ok());
        }
        assert!(
            !std::fs::read_dir(&directory).unwrap().any(|entry| entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".azd-gui-write-")),
            "Guard must delete itself on close"
        );
    }
}
