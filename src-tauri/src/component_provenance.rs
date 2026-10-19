//! Read-only projection of Reference's azd-components.lock.json 1.0 contract.
//! The manifest is untrusted metadata. Nothing here fetches sources or invokes a sync command.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    fs::{self, File},
    io::{self, Read},
    path::Path,
};

const LOCK_FILE: &str = "azd-components.lock.json";
const MAX_LOCK_BYTES: u64 = 1024 * 1024;
const MAX_COMPONENTS: usize = 128;
const MAX_FILES: usize = 2_000;
const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 64 * 1024 * 1024;
const MAX_PATH_BYTES: usize = 1_024;
const MAX_DEPTH: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ReadState {
    Available,
    NotFound,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Verified,
    Modified,
    Missing,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Reason {
    InvalidMetadata,
    ManifestVersion,
    UnsafePath,
    LimitExceeded,
    Unreadable,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentProvenanceRead {
    pub state: ReadState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<Reason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_version: Option<String>,
    pub unknown_metadata: bool,
    pub components: Vec<ComponentProvenance>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentProvenance {
    pub id: String,
    pub version: String,
    pub source_repository: String,
    pub source_revision: String,
    pub status: Status,
    pub unknown_metadata: bool,
    pub files: Vec<ManagedFile>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedFile {
    pub source: String,
    pub target: String,
    pub sha256: String,
    pub status: Status,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<Reason>,
}

// Extra fields are deliberately accepted, never evaluated, and not projected to the UI.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawLock {
    manifest_version: String,
    baseline: Option<String>,
    components: Vec<RawComponent>,
    #[serde(flatten)]
    extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawComponent {
    id: String,
    version: String,
    source_repository: String,
    source_revision: String,
    files: Vec<RawFile>,
    #[serde(flatten)]
    extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Deserialize)]
struct RawFile {
    source: String,
    target: String,
    sha256: String,
    #[serde(flatten)]
    extra: BTreeMap<String, serde_json::Value>,
}

impl ComponentProvenanceRead {
    fn unavailable(state: ReadState, reason: Option<Reason>) -> Self {
        Self {
            state,
            reason,
            manifest_version: None,
            unknown_metadata: false,
            components: Vec::new(),
        }
    }
}

#[derive(Debug)]
enum ReadError {
    Missing,
    Unsupported(Reason),
}

fn io_error(error: io::Error) -> ReadError {
    if error.kind() == io::ErrorKind::NotFound {
        ReadError::Missing
    } else {
        ReadError::Unsupported(Reason::Unreadable)
    }
}

fn unsafe_path() -> ReadError {
    ReadError::Unsupported(Reason::UnsafePath)
}

fn safe_relative_path(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_PATH_BYTES
        && !value.contains(['\\', ':'])
        && !value.chars().any(char::is_control)
        && value.split('/').count() <= MAX_DEPTH
        && value.split('/').all(|part| {
            !part.is_empty() && part != "." && part != ".."
                // Avoid Windows trailing-dot/space aliases and device names on every platform.
                && !part.ends_with(['.', ' '])
                && !matches!(part.split('.').next().unwrap_or("").to_ascii_uppercase().as_str(),
                    "CON" | "PRN" | "AUX" | "NUL" | "COM1" | "COM2" | "COM3" | "COM4" |
                    "COM5" | "COM6" | "COM7" | "COM8" | "COM9" | "LPT1" | "LPT2" |
                    "LPT3" | "LPT4" | "LPT5" | "LPT6" | "LPT7" | "LPT8" | "LPT9")
        })
}

// On Unix, every lookup is relative to an already-open directory descriptor.
// O_NOFOLLOW applies to every segment, so rename/link races cannot escape the root.
#[cfg(unix)]
struct Root(File);

#[cfg(unix)]
impl Root {
    fn open(path: &Path) -> Result<Self, ReadError> {
        use std::os::{
            fd::{AsRawFd, FromRawFd},
            unix::fs::OpenOptionsExt,
        };
        if !path.is_absolute() {
            return Err(unsafe_path());
        }
        let mut directory = fs::OpenOptions::new()
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
                        return Err(unsafe_path());
                    }
                    directory = unsafe { File::from_raw_fd(fd) };
                }
                _ => return Err(unsafe_path()),
            }
        }
        Ok(Self(directory))
    }

    fn file(&self, relative: &str) -> Result<File, ReadError> {
        use std::os::fd::{AsRawFd, FromRawFd};
        let mut directory = self.0.try_clone().map_err(io_error)?;
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
            // omit O_CREAT. A successful new descriptor is immediately owned by File.
            let fd = unsafe { libc::openat(directory.as_raw_fd(), name.as_ptr(), flags) };
            if fd < 0 {
                let error = io::Error::last_os_error();
                return Err(
                    if matches!(error.raw_os_error(), Some(libc::ELOOP | libc::ENOTDIR)) {
                        unsafe_path()
                    } else {
                        io_error(error)
                    },
                );
            }
            let file = unsafe { File::from_raw_fd(fd) };
            if last {
                if !file.metadata().map_err(io_error)?.is_file() {
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
struct Root {
    path: std::path::PathBuf,
    _ancestors: Vec<File>,
}

#[cfg(windows)]
fn windows_open(path: &Path, directory: bool) -> Result<File, ReadError> {
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    let file = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .custom_flags(0x00200000 | 0x02000000) // OPEN_REPARSE_POINT | BACKUP_SEMANTICS
        .open(path)
        .map_err(io_error)?;
    let metadata = file.metadata().map_err(io_error)?;
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
    fn open(path: &Path) -> Result<Self, ReadError> {
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
                    ancestors.push(windows_open(&current, true)?);
                }
                _ => return Err(unsafe_path()),
            }
        }
        Ok(Self {
            path: current,
            _ancestors: ancestors,
        })
    }

    fn file(&self, relative: &str) -> Result<File, ReadError> {
        let mut current = self.path.clone();
        let mut parents = Vec::new();
        let parts: Vec<_> = relative.split('/').collect();
        for (index, part) in parts.iter().enumerate() {
            current.push(part);
            let last = index + 1 == parts.len();
            let file = windows_open(&current, !last)?;
            if last {
                return Ok(file);
            }
            parents.push(file);
        }
        Err(unsafe_path())
    }
}

fn bounded_bytes(mut file: File, limit: u64) -> Result<Vec<u8>, ReadError> {
    if file.metadata().map_err(io_error)?.len() > limit {
        return Err(ReadError::Unsupported(Reason::LimitExceeded));
    }
    let mut bytes = Vec::new();
    file.by_ref()
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    if bytes.len() as u64 > limit {
        return Err(ReadError::Unsupported(Reason::LimitExceeded));
    }
    Ok(bytes)
}

fn valid_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn uri_chars(value: &str, extra: &[u8]) -> bool {
    value.bytes().all(|byte| {
        byte.is_ascii_alphanumeric() || b"-._~!$&'()*+,;=%".contains(&byte) || extra.contains(&byte)
    })
}

fn valid_source_uri(value: &str) -> bool {
    // Validate the original URI components before WHATWG URL normalization.
    let bytes = value.as_bytes();
    if value.len() > 2_048
        || !bytes.iter().enumerate().all(|(index, byte)| {
            *byte != b'%'
                || bytes.get(index + 1).is_some_and(u8::is_ascii_hexdigit)
                    && bytes.get(index + 2).is_some_and(u8::is_ascii_hexdigit)
        })
    {
        return false;
    }
    let Some((scheme, rest)) = value.split_once(':') else {
        return false;
    };
    if !matches_pattern(scheme, r"^[A-Za-z][A-Za-z0-9+.-]*$") {
        return false;
    }
    let (before_fragment, fragment) = rest.split_once('#').unwrap_or((rest, ""));
    let (hierarchy, query) = before_fragment
        .split_once('?')
        .unwrap_or((before_fragment, ""));
    if !uri_chars(fragment, b":@/?") || !uri_chars(query, b":@/?") {
        return false;
    }
    let path = if let Some(authority_path) = hierarchy.strip_prefix("//") {
        let (authority, path) = authority_path
            .split_once('/')
            .unwrap_or((authority_path, ""));
        let host_port = if let Some((user_info, host_port)) = authority.split_once('@') {
            if !uri_chars(user_info, b":") {
                return false;
            }
            host_port
        } else {
            authority
        };
        if let Some(literal) = host_port.strip_prefix('[') {
            let Some((address, port)) = literal.split_once(']') else {
                return false;
            };
            if address.parse::<std::net::Ipv6Addr>().is_err()
                || !port.is_empty()
                    && !port
                        .strip_prefix(':')
                        .is_some_and(|port| port.bytes().all(|byte| byte.is_ascii_digit()))
            {
                return false;
            }
        } else {
            let (host, port) = host_port.split_once(':').unwrap_or((host_port, ""));
            if !uri_chars(host, b"") || !port.bytes().all(|byte| byte.is_ascii_digit()) {
                return false;
            }
        }
        path
    } else {
        hierarchy
    };
    uri_chars(path, b":@/") && url::Url::parse(value).is_ok()
}

fn matches_pattern(value: &str, pattern: &str) -> bool {
    regex::Regex::new(pattern)
        .expect("static metadata pattern")
        .is_match(value)
}

pub fn read(project_root: &Path) -> ComponentProvenanceRead {
    match read_inner(project_root) {
        Ok(result) => result,
        Err(ReadError::Missing) => ComponentProvenanceRead::unavailable(ReadState::NotFound, None),
        Err(ReadError::Unsupported(reason)) => {
            ComponentProvenanceRead::unavailable(ReadState::Unsupported, Some(reason))
        }
    }
}

fn read_inner(project_root: &Path) -> Result<ComponentProvenanceRead, ReadError> {
    let root = Root::open(project_root)?;
    let bytes = bounded_bytes(root.file(LOCK_FILE)?, MAX_LOCK_BYTES)?;
    let invalid = || ReadError::Unsupported(Reason::InvalidMetadata);
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    let version = value
        .get("manifestVersion")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(invalid)?;
    if version != "1.0" {
        let mut result = ComponentProvenanceRead::unavailable(
            ReadState::Unsupported,
            Some(Reason::ManifestVersion),
        );
        if version.len() <= 64 && !version.chars().any(char::is_control) {
            result.manifest_version = Some(version.into());
        }
        return Ok(result);
    }
    if value
        .get("baseline")
        .is_some_and(|baseline| !baseline.is_string())
    {
        return Err(invalid());
    }
    let lock: RawLock = serde_json::from_value(value).map_err(|_| invalid())?;
    if lock.components.len() > MAX_COMPONENTS
        || lock
            .components
            .iter()
            .map(|item| item.files.len())
            .sum::<usize>()
            > MAX_FILES
    {
        return Err(ReadError::Unsupported(Reason::LimitExceeded));
    }
    if lock.baseline.as_ref().is_some_and(|value| {
        value.len() > 64 || !matches_pattern(value, r"^[0-9]{4}\.[0-9]{2}(?:\.[0-9]+)?$")
    }) {
        return Err(invalid());
    }
    let mut ids = HashSet::new();
    // Validate shape first, so an invalid later component cannot partially validate the lock.
    for item in &lock.components {
        if !matches_pattern(&item.id, r"^[a-z][a-z0-9-]{1,62}[a-z0-9]$")
            || !ids.insert(&item.id)
            || item.version.len() > 128
            || !matches_pattern(
                &item.version,
                r"^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-[0-9A-Za-z.-]+)?$",
            )
            || !valid_hex(&item.source_revision, 40)
            || !valid_source_uri(&item.source_repository)
            || item.files.is_empty()
        {
            return Err(invalid());
        }
    }
    let mut result = ComponentProvenanceRead {
        state: ReadState::Available,
        reason: None,
        manifest_version: Some(lock.manifest_version),
        unknown_metadata: !lock.extra.is_empty(),
        components: Vec::new(),
    };
    let mut remaining = MAX_TOTAL_BYTES;
    let mut targets = HashSet::new();
    for item in lock.components {
        let mut component = ComponentProvenance {
            id: item.id,
            version: item.version,
            source_repository: item.source_repository,
            source_revision: item.source_revision,
            status: Status::Verified,
            unknown_metadata: !item.extra.is_empty(),
            files: Vec::new(),
        };
        for file in item.files {
            component.unknown_metadata |= !file.extra.is_empty();
            let verified = if !safe_relative_path(&file.source) || !safe_relative_path(&file.target)
            {
                Err(unsafe_path())
            } else if !valid_hex(&file.sha256, 64)
                || !targets.insert(file.target.to_ascii_lowercase())
            {
                Err(invalid())
            } else {
                verify_file(&root, &file.target, &file.sha256, &mut remaining)
            };
            let (status, reason) = match verified {
                Ok(status) => (status, None),
                Err(ReadError::Missing) => (Status::Missing, None),
                Err(ReadError::Unsupported(reason)) => (Status::Unsupported, Some(reason)),
            };
            // Unsafe/oversized path strings never become display or lookup inputs elsewhere.
            component.files.push(ManagedFile {
                source: display_metadata(file.source, MAX_PATH_BYTES),
                target: display_metadata(file.target, MAX_PATH_BYTES),
                sha256: if valid_hex(&file.sha256, 64) {
                    file.sha256
                } else {
                    String::new()
                },
                status,
                reason,
            });
        }
        component.status = [Status::Unsupported, Status::Missing, Status::Modified]
            .into_iter()
            .find(|status| component.files.iter().any(|file| file.status == *status))
            .unwrap_or(Status::Verified);
        result.components.push(component);
    }
    Ok(result)
}

fn display_metadata(value: String, limit: usize) -> String {
    if value.len() > limit || value.chars().any(char::is_control) {
        "[unsupported path]".into()
    } else {
        value
    }
}

fn verify_file(
    root: &Root,
    target: &str,
    expected: &str,
    remaining: &mut u64,
) -> Result<Status, ReadError> {
    let mut file = root.file(target)?;
    let metadata = file.metadata().map_err(io_error)?;
    let limit = MAX_FILE_BYTES.min(*remaining);
    if metadata.len() > limit {
        return Err(ReadError::Unsupported(Reason::LimitExceeded));
    }
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut bytes = 0u64;
    loop {
        // At most one byte beyond the budget is read to detect growing files.
        let capacity = buffer.len().min((limit - bytes + 1) as usize);
        let count = file.read(&mut buffer[..capacity]).map_err(io_error)?;
        if count == 0 {
            break;
        }
        bytes += count as u64;
        *remaining = remaining.saturating_sub(count as u64);
        if bytes > limit {
            return Err(ReadError::Unsupported(Reason::LimitExceeded));
        }
        hasher.update(&buffer[..count]);
    }
    if bytes != metadata.len() {
        return Err(ReadError::Unsupported(Reason::Unreadable));
    }
    Ok(if format!("{:x}", hasher.finalize()) == expected {
        Status::Verified
    } else {
        Status::Modified
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    fn tempdir() -> io::Result<tempfile::TempDir> {
        tempfile::tempdir_in(fs::canonicalize(std::env::temp_dir())?)
    }

    fn fixture() -> Value {
        serde_json::from_str(include_str!(
            "../tests/fixtures/azd-components-lock-v1.json"
        ))
        .unwrap()
    }

    fn write_lock(root: &Path, value: &Value) {
        fs::write(root.join(LOCK_FILE), serde_json::to_vec(value).unwrap()).unwrap();
    }

    fn write_managed_file(root: &Path, value: &mut Value, content: &[u8]) {
        let target = root.join(
            value["components"][0]["files"][0]["target"]
                .as_str()
                .unwrap(),
        );
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(target, content).unwrap();
        value["components"][0]["files"][0]["sha256"] =
            json!(format!("{:x}", Sha256::digest(content)));
        write_lock(root, value);
    }

    #[test]
    fn absent_lock_preserves_existing_behavior() {
        let root = tempdir().unwrap();
        fs::write(root.path().join("azure.yaml"), "name: existing").unwrap();
        let result = read(root.path());
        assert_eq!(result.state, ReadState::NotFound);
        assert!(result.components.is_empty());
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    }

    #[test]
    fn accepts_the_pinned_reference_contract_and_preserves_exact_provenance() {
        let root = tempdir().unwrap();
        write_lock(root.path(), &fixture());
        let result = read(root.path());
        assert_eq!(result.state, ReadState::Available);
        let component = &result.components[0];
        assert_eq!(component.id, "deployment-validation");
        assert_eq!(component.version, "0.1.0");
        assert_eq!(
            component.source_revision,
            "0123456789abcdef0123456789abcdef01234567"
        );
        assert_eq!(
            component.source_repository,
            "https://github.com/example/azd-reference"
        );
        assert_eq!(component.status, Status::Missing);
    }

    #[test]
    fn hashes_bytes_and_distinguishes_verified_modified_and_missing_without_writes() {
        let root = tempdir().unwrap();
        let mut value = fixture();
        let target = root.path().join("scripts/vendor/engine.psm1");
        write_managed_file(root.path(), &mut value, b"exact\r\nbytes\0");
        let lock_before = fs::read(root.path().join(LOCK_FILE)).unwrap();
        fs::write(root.path().join("extension.ps1"), "project owned").unwrap();
        assert_eq!(read(root.path()).components[0].status, Status::Verified);
        fs::write(&target, b"exact\nbytes\0").unwrap();
        assert_eq!(read(root.path()).components[0].status, Status::Modified);
        fs::remove_file(target).unwrap();
        assert_eq!(read(root.path()).components[0].status, Status::Missing);
        assert_eq!(fs::read(root.path().join(LOCK_FILE)).unwrap(), lock_before);
        assert_eq!(
            fs::read_to_string(root.path().join("extension.ps1")).unwrap(),
            "project owned"
        );
    }

    #[test]
    fn accepts_unknown_metadata_without_exposing_or_executing_it() {
        let root = tempdir().unwrap();
        let mut value = fixture();
        value["syncCommand"] = json!("untrusted-command-marker");
        value["components"][0]["extensionPoints"] = json!(["../../untrusted-extension-marker"]);
        value["components"][0]["files"][0]["unknown"] =
            json!({"secret": "untrusted-secret-marker"});
        write_managed_file(root.path(), &mut value, b"trusted local bytes");
        let result = read(root.path());
        assert!(result.unknown_metadata);
        assert!(result.components[0].unknown_metadata);
        assert_eq!(result.components[0].status, Status::Verified);
        assert!(!serde_json::to_string(&result)
            .unwrap()
            .contains("untrusted-"));
    }

    #[test]
    fn unsupported_versions_and_invalid_metadata_never_claim_verification() {
        let root = tempdir().unwrap();
        let mut value = fixture();
        value["manifestVersion"] = json!("2.0");
        value["components"] = json!({"newContract": true});
        write_lock(root.path(), &value);
        let result = read(root.path());
        assert_eq!(result.state, ReadState::Unsupported);
        assert_eq!(result.reason, Some(Reason::ManifestVersion));
        assert_eq!(result.manifest_version.as_deref(), Some("2.0"));
        assert!(result.components.is_empty());
        for (key, invalid) in [
            ("id", json!("../bad")),
            ("version", json!("1.2")),
            ("sourceRevision", json!("main")),
            ("sourceRepository", json!("relative/path")),
            ("files", json!([])),
        ] {
            let mut value = fixture();
            value["components"][0][key] = invalid;
            write_lock(root.path(), &value);
            assert_eq!(read(root.path()).state, ReadState::Unsupported, "{key}");
        }
        fs::write(root.path().join(LOCK_FILE), b"{ invalid json").unwrap();
        assert_eq!(read(root.path()).reason, Some(Reason::InvalidMetadata));
    }

    #[test]
    fn rejects_uri_spellings_that_a_url_parser_would_normalize() {
        let root = tempdir().unwrap();
        for uri in [
            "https://example.com/a b",
            "https://example.com/[",
            "https://example.com/a#b#c",
            "https://example.com/?x=[",
            "https://a@b@example.com/source",
            "https://example.com/%zz",
            "https://example.com/%",
            "https://example.com/<script>",
            "https://example.com/é",
            r"https:\\example.com\source",
        ] {
            let mut value = fixture();
            value["components"][0]["sourceRepository"] = json!(uri);
            write_managed_file(root.path(), &mut value, b"bytes");
            assert_eq!(
                read(root.path()).reason,
                Some(Reason::InvalidMetadata),
                "{uri}"
            );
        }
        for uri in [
            "https://example.com/a%20b?ref=x#source",
            "urn:example:source",
            "ssh://git@example.com/source.git",
            "https://[::1]:443/source?ref=x/y?z#part/a?b",
        ] {
            assert!(valid_source_uri(uri), "{uri}");
        }
    }

    #[test]
    fn bounds_manifest_components_files_and_metadata() {
        let root = tempdir().unwrap();
        fs::write(
            root.path().join(LOCK_FILE),
            vec![b' '; MAX_LOCK_BYTES as usize + 1],
        )
        .unwrap();
        assert_eq!(read(root.path()).reason, Some(Reason::LimitExceeded));
        let mut value = fixture();
        value["components"] = json!(vec![value["components"][0].clone(); MAX_COMPONENTS + 1]);
        write_lock(root.path(), &value);
        assert_eq!(read(root.path()).reason, Some(Reason::LimitExceeded));
        let mut value = fixture();
        value["components"][0]["files"] = json!(vec![
            value["components"][0]["files"][0].clone();
            MAX_FILES + 1
        ]);
        write_lock(root.path(), &value);
        assert_eq!(read(root.path()).reason, Some(Reason::LimitExceeded));
        let mut value = fixture();
        value["components"][0]["files"][0]["target"] = json!("x".repeat(MAX_PATH_BYTES + 1));
        write_lock(root.path(), &value);
        let result = read(root.path());
        assert_eq!(result.components[0].status, Status::Unsupported);
        assert_eq!(result.components[0].files[0].target, "[unsupported path]");
    }

    #[test]
    fn rejects_traversal_external_alias_and_malformed_hash_paths() {
        let root = tempdir().unwrap();
        for path in [
            "../outside",
            "a/../../outside",
            "/etc/passwd",
            "C:/outside",
            "C:outside",
            "//host/share/file",
            "a\\file",
            "./file",
            "a/./file",
            "a//file",
            "a/file:stream",
            "a/NUL.txt",
            "a/CON",
            "a/file.",
            "a/file ",
            "a/\0file",
        ] {
            for key in ["source", "target"] {
                let mut value = fixture();
                value["components"][0]["files"][0][key] = json!(path);
                write_lock(root.path(), &value);
                assert_eq!(
                    read(root.path()).components[0].files[0].reason,
                    Some(Reason::UnsafePath),
                    "{key}: {path}"
                );
            }
        }
        assert!(!safe_relative_path(&vec!["a"; MAX_DEPTH + 1].join("/")));
        let mut value = fixture();
        value["components"][0]["files"][0]["sha256"] = json!("A".repeat(64));
        write_lock(root.path(), &value);
        assert_eq!(read(root.path()).components[0].status, Status::Unsupported);
    }

    #[test]
    fn duplicate_ids_and_targets_are_not_fully_verified() {
        let root = tempdir().unwrap();
        let mut value = fixture();
        write_managed_file(root.path(), &mut value, b"bytes");
        let component = value["components"][0].clone();
        value["components"].as_array_mut().unwrap().push(component);
        write_lock(root.path(), &value);
        assert_eq!(read(root.path()).state, ReadState::Unsupported);
        value["components"][1]["id"] = json!("another-component");
        write_lock(root.path(), &value);
        assert_eq!(read(root.path()).components[1].status, Status::Unsupported);
    }

    #[test]
    fn bounds_individual_and_total_hashed_bytes() {
        let root = tempdir().unwrap();
        let mut value = fixture();
        write_managed_file(root.path(), &mut value, b"abc");
        let opened = Root::open(root.path()).unwrap();
        let mut remaining = 3;
        assert_eq!(
            verify_file(
                &opened,
                "scripts/vendor/engine.psm1",
                value["components"][0]["files"][0]["sha256"]
                    .as_str()
                    .unwrap(),
                &mut remaining
            )
            .unwrap(),
            Status::Verified
        );
        assert_eq!(remaining, 0);
        assert!(matches!(
            verify_file(&opened, "scripts/vendor/engine.psm1", "", &mut remaining),
            Err(ReadError::Unsupported(Reason::LimitExceeded))
        ));
        File::create(root.path().join("scripts/vendor/engine.psm1"))
            .unwrap()
            .set_len(MAX_FILE_BYTES + 1)
            .unwrap();
        assert_eq!(
            read(root.path()).components[0].files[0].reason,
            Some(Reason::LimitExceeded)
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_roots_locks_files_and_intermediate_directories() {
        use std::os::unix::fs::symlink;
        let parent = tempdir().unwrap();
        let external = tempdir().unwrap();
        let root = parent.path().join("project");
        fs::create_dir(&root).unwrap();
        symlink(&root, parent.path().join("linked-project")).unwrap();
        assert_eq!(
            read(&parent.path().join("linked-project")).reason,
            Some(Reason::UnsafePath)
        );
        fs::write(external.path().join(LOCK_FILE), fixture().to_string()).unwrap();
        symlink(external.path().join(LOCK_FILE), root.join(LOCK_FILE)).unwrap();
        assert_eq!(read(&root).reason, Some(Reason::UnsafePath));
        fs::remove_file(root.join(LOCK_FILE)).unwrap();
        write_lock(&root, &fixture());
        symlink(external.path(), root.join("scripts")).unwrap();
        assert_eq!(
            read(&root).components[0].files[0].reason,
            Some(Reason::UnsafePath)
        );
        fs::remove_file(root.join("scripts")).unwrap();
        fs::create_dir_all(root.join("scripts/vendor")).unwrap();
        symlink(
            external.path().join("absent"),
            root.join("scripts/vendor/engine.psm1"),
        )
        .unwrap();
        assert_eq!(
            read(&root).components[0].files[0].reason,
            Some(Reason::UnsafePath)
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinks_in_project_root_ancestors() {
        use std::os::unix::fs::symlink;
        let parent = tempdir().unwrap();
        let external = tempdir().unwrap();
        fs::create_dir(external.path().join("project")).unwrap();
        let mut value = fixture();
        write_managed_file(&external.path().join("project"), &mut value, b"outside");
        symlink(external.path(), parent.path().join("linked-ancestor")).unwrap();
        let result = read(&parent.path().join("linked-ancestor/project"));
        assert_eq!(result.state, ReadState::Unsupported);
        assert_eq!(result.reason, Some(Reason::UnsafePath));
    }

    #[test]
    fn an_optional_baseline_must_be_a_string_when_present() {
        let root = tempdir().unwrap();
        let mut value = fixture();
        for invalid in [json!(null), json!(12), json!([])] {
            value["baseline"] = invalid;
            write_lock(root.path(), &value);
            assert_eq!(read(root.path()).reason, Some(Reason::InvalidMetadata));
        }
        value.as_object_mut().unwrap().remove("baseline");
        write_lock(root.path(), &value);
        assert_eq!(read(root.path()).state, ReadState::Available);
    }

    #[cfg(unix)]
    #[test]
    fn an_open_root_stays_anchored_when_its_path_is_replaced() {
        use std::os::unix::fs::symlink;
        let parent = tempdir().unwrap();
        let root = parent.path().join("project");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("file"), b"inside").unwrap();
        let opened = Root::open(&root).unwrap();
        let external = tempdir().unwrap();
        fs::write(external.path().join("file"), b"outside").unwrap();
        fs::rename(&root, parent.path().join("moved")).unwrap();
        symlink(external.path(), &root).unwrap();
        assert_eq!(
            bounded_bytes(opened.file("file").unwrap(), 100).unwrap(),
            b"inside"
        );
    }

    #[cfg(unix)]
    #[test]
    fn nonregular_files_are_rejected_without_blocking() {
        let root = tempdir().unwrap();
        write_lock(root.path(), &fixture());
        fs::create_dir_all(root.path().join("scripts/vendor")).unwrap();
        let path = std::ffi::CString::new(
            root.path()
                .join("scripts/vendor/engine.psm1")
                .to_str()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
        assert_eq!(
            read(root.path()).components[0].files[0].reason,
            Some(Reason::UnsafePath)
        );
    }

    #[cfg(windows)]
    #[test]
    fn rejects_windows_junction_escape() {
        use std::os::windows::process::CommandExt;
        let root = tempdir().unwrap();
        let external = tempdir().unwrap();
        write_lock(root.path(), &fixture());
        let status = std::process::Command::new("cmd.exe")
            .creation_flags(0x08000000)
            .args(["/C", "mklink", "/J"])
            .arg(root.path().join("scripts"))
            .arg(external.path())
            .status()
            .unwrap();
        assert!(status.success());
        assert_eq!(
            read(root.path()).components[0].files[0].reason,
            Some(Reason::UnsafePath)
        );
    }
}
