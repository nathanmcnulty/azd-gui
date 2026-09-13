use crate::{
    error::{AppError, AppResult},
    models::LocalSourceBinding,
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};
use tempfile::TempDir;

const MAX_ENTRIES: usize = 4_000;
const MAX_TOTAL_BYTES: u64 = 64 * 1024 * 1024;
const COPY_BUFFER_BYTES: usize = 64 * 1024;
const EXCLUDED_DIRECTORIES: &[&str] = &[".git", ".azure"];

fn rejected(message: &str) -> AppError {
    AppError::Invalid(format!("Local source binding rejected: {message}"))
}

fn is_link_or_reparse(path: &Path) -> AppResult<bool> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Ok(true);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        return Ok(metadata.file_attributes() & 0x400 != 0);
    }
    #[cfg(not(windows))]
    Ok(false)
}

fn canonical_root(path: &Path) -> AppResult<PathBuf> {
    if is_link_or_reparse(path)? {
        return Err(rejected(
            "source root cannot be a symbolic link or reparse point",
        ));
    }
    let canonical = fs::canonicalize(path)?;
    if !canonical.is_dir() {
        return Err(rejected("source root must be a folder"));
    }
    Ok(canonical)
}

fn relative_name(root: &Path, path: &Path) -> AppResult<String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| rejected("source entry escaped its root"))?;
    let value = relative
        .to_str()
        .ok_or_else(|| rejected("source paths must use Unicode names"))?
        .replace('\\', "/");
    if value.is_empty() || value.len() > 1_024 {
        return Err(rejected("source path is empty or too long"));
    }
    Ok(value)
}

fn hash_record(hasher: &mut Sha256, kind: u8, name: &str, size: u64) {
    hasher.update([kind]);
    hasher.update((name.len() as u32).to_be_bytes());
    hasher.update(name.as_bytes());
    hasher.update(size.to_be_bytes());
}

struct TreeWriter<'a> {
    root: &'a Path,
    destination: Option<&'a Path>,
    hasher: Sha256,
    entries: usize,
    bytes: u64,
}

impl<'a> TreeWriter<'a> {
    fn new(root: &'a Path, destination: Option<&'a Path>) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"azd-gui-local-source-v1\0");
        // .git is VCS metadata and .azure is azd runtime state. Every other source directory,
        // including hidden directories, is hashed and staged rather than being silently skipped.
        hasher.update(b"excluded:.git,.azure\0");
        Self {
            root,
            destination,
            hasher,
            entries: 0,
            bytes: 0,
        }
    }

    fn visit_directory(&mut self, directory: &Path) -> AppResult<()> {
        let mut entries = fs::read_dir(directory)?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let name = entry.file_name();
            if directory == self.root
                && name
                    .to_str()
                    .is_some_and(|name| EXCLUDED_DIRECTORIES.contains(&name))
            {
                continue;
            }
            self.visit_entry(&path)?;
        }
        Ok(())
    }

    fn visit_entry(&mut self, source: &Path) -> AppResult<()> {
        self.entries += 1;
        if self.entries > MAX_ENTRIES {
            return Err(rejected("source tree contains too many entries"));
        }
        if is_link_or_reparse(source)? {
            return Err(rejected(
                "source tree cannot contain symbolic links or reparse points",
            ));
        }
        let metadata = fs::metadata(source)?;
        let name = relative_name(self.root, source)?;
        if metadata.is_dir() {
            hash_record(&mut self.hasher, b'D', &name, 0);
            if let Some(destination) = self.destination {
                fs::create_dir_all(destination.join(&name))?;
            }
            return self.visit_directory(source);
        }
        if !metadata.is_file() {
            return Err(rejected(
                "source tree can contain only regular files and folders",
            ));
        }
        self.bytes = self
            .bytes
            .checked_add(metadata.len())
            .ok_or_else(|| rejected("source tree is too large"))?;
        if self.bytes > MAX_TOTAL_BYTES {
            return Err(rejected("source tree exceeds the 64 MiB staging limit"));
        }
        hash_record(&mut self.hasher, b'F', &name, metadata.len());
        let mut input = fs::File::open(source)?;
        let destination_path = self.destination.map(|destination| destination.join(&name));
        let mut output = destination_path
            .as_ref()
            .map(fs::File::create)
            .transpose()?;
        let mut buffer = [0u8; COPY_BUFFER_BYTES];
        loop {
            let count = input.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            self.hasher.update(&buffer[..count]);
            if let Some(output) = &mut output {
                output.write_all(&buffer[..count])?;
            }
        }
        drop(output);
        if let Some(destination_path) = destination_path {
            fs::set_permissions(destination_path, metadata.permissions())?;
        }
        Ok(())
    }

    fn finish(self) -> String {
        format!("{:x}", self.hasher.finalize())
    }
}

fn digest_source(root: &Path, destination: Option<&Path>) -> AppResult<String> {
    let mut writer = TreeWriter::new(root, destination);
    writer.visit_directory(root)?;
    Ok(writer.finish())
}

fn validate_binding_shape(binding: &LocalSourceBinding) -> AppResult<()> {
    if binding.source_path.len() > 32_768
        || binding.content_digest.len() != 64
        || !binding
            .content_digest
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Err(rejected("source binding is malformed"));
    }
    Ok(())
}

pub fn inspect(path: &Path) -> AppResult<LocalSourceBinding> {
    let root = canonical_root(path)?;
    Ok(LocalSourceBinding {
        source_path: root.display().to_string(),
        content_digest: digest_source(&root, None)?,
    })
}

pub fn verify_for_path(binding: &LocalSourceBinding, path: &Path) -> AppResult<()> {
    validate_binding_shape(binding)?;
    let root = canonical_root(path)?;
    if root.display().to_string() != binding.source_path {
        return Err(rejected("source path changed since inspection"));
    }
    if digest_source(&root, None)? != binding.content_digest {
        return Err(rejected(
            "source content or manifest changed since inspection",
        ));
    }
    Ok(())
}

pub fn stage_for_path(
    binding: &LocalSourceBinding,
    path: &Path,
    cache_root: &Path,
) -> AppResult<TempDir> {
    validate_binding_shape(binding)?;
    let root = canonical_root(path)?;
    if root.display().to_string() != binding.source_path {
        return Err(rejected("source path changed since inspection"));
    }
    fs::create_dir_all(cache_root)?;
    let staging = tempfile::Builder::new()
        .prefix("azd-gui-local-source-")
        .tempdir_in(cache_root)?;
    let digest = digest_source(&root, Some(staging.path()))?;
    if digest != binding.content_digest {
        return Err(rejected(
            "source content or manifest changed since inspection",
        ));
    }
    Ok(staging)
}

#[cfg(test)]
mod tests {
    use super::{inspect, stage_for_path, verify_for_path};
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn staged_source_matches_the_reviewed_tree_and_excludes_runtime_state() {
        let source = tempdir().unwrap();
        fs::write(source.path().join("azure.yaml"), "name: sample").unwrap();
        fs::create_dir(source.path().join("scripts")).unwrap();
        fs::write(source.path().join("scripts/run.ps1"), "Write-Host reviewed").unwrap();
        fs::create_dir(source.path().join(".azure")).unwrap();
        fs::write(source.path().join(".azure/.env"), "LOCAL_STATE=ignored").unwrap();

        let binding = inspect(source.path()).unwrap();
        fs::write(source.path().join(".azure/.env"), "LOCAL_STATE=changed").unwrap();
        verify_for_path(&binding, source.path()).unwrap();
        let cache = tempdir().unwrap();
        let staged = stage_for_path(&binding, source.path(), cache.path()).unwrap();
        assert!(staged.path().join("scripts/run.ps1").is_file());
        assert!(!staged.path().join(".azure").exists());
    }

    #[test]
    fn rejects_content_changes_before_staging() {
        let source = tempdir().unwrap();
        fs::write(source.path().join("azure.yaml"), "name: sample").unwrap();
        let binding = inspect(source.path()).unwrap();
        fs::write(source.path().join("azure.yaml"), "name: changed").unwrap();

        assert!(verify_for_path(&binding, source.path()).is_err());
        let cache = tempdir().unwrap();
        assert!(stage_for_path(&binding, source.path(), cache.path()).is_err());
    }
}
