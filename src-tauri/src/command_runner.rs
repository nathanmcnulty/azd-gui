use crate::{
    error::{AppError, AppResult},
    models::{OperationEvent, OperationOutcome},
    persistence::Database,
    redaction::redact,
};
use chrono::Utc;
use std::{
    collections::HashMap,
    ffi::OsString,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc,
    },
    time::Duration,
};
use tauri::{AppHandle, Emitter};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
    sync::{oneshot, Mutex},
};
use uuid::Uuid;

const MAX_CAPTURE_BYTES_PER_STREAM: usize = 4 * 1024 * 1024;
const MAX_STREAM_LINE_BYTES: usize = 64 * 1024;
const OUTPUT_TRUNCATED_MARKER: &str = "\n[output truncated]\n";

#[cfg(windows)]
struct ProcessTree {
    job: std::os::windows::io::OwnedHandle,
}

#[cfg(windows)]
impl ProcessTree {
    fn attach(child: &tokio::process::Child) -> std::io::Result<Self> {
        use std::os::windows::io::{AsRawHandle, FromRawHandle};
        use windows_sys::Win32::{
            Foundation::HANDLE,
            System::JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
                SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
                JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            },
        };

        let raw_job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if raw_job.is_null() {
            return Err(std::io::Error::last_os_error());
        }
        let job = unsafe { std::os::windows::io::OwnedHandle::from_raw_handle(raw_job.cast()) };
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let configured = unsafe {
            SetInformationJobObject(
                job.as_raw_handle() as HANDLE,
                JobObjectExtendedLimitInformation,
                (&raw const limits).cast(),
                std::mem::size_of_val(&limits) as u32,
            )
        };
        if configured == 0 {
            return Err(std::io::Error::last_os_error());
        }
        let process = child
            .raw_handle()
            .ok_or_else(|| std::io::Error::other("Child process handle is unavailable."))?;
        let assigned =
            unsafe { AssignProcessToJobObject(job.as_raw_handle() as HANDLE, process as HANDLE) };
        if assigned == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(Self { job })
    }

    fn terminate(&self) {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::{Foundation::HANDLE, System::JobObjects::TerminateJobObject};
        unsafe {
            TerminateJobObject(self.job.as_raw_handle() as HANDLE, 1);
        }
    }
}

#[cfg(unix)]
struct ProcessTree {
    process_group: i32,
}

#[cfg(unix)]
impl ProcessTree {
    fn attach(child: &tokio::process::Child) -> std::io::Result<Self> {
        let process_group = child
            .id()
            .and_then(|id| i32::try_from(id).ok())
            .ok_or_else(|| std::io::Error::other("Child process ID is unavailable."))?;
        Ok(Self { process_group })
    }

    fn terminate(&self) {
        unsafe {
            libc::kill(-self.process_group, libc::SIGKILL);
        }
    }
}

impl Drop for ProcessTree {
    fn drop(&mut self) {
        self.terminate();
    }
}

fn attach_process_tree(child: &mut tokio::process::Child) -> AppResult<Option<ProcessTree>> {
    match ProcessTree::attach(child) {
        Ok(tree) => Ok(Some(tree)),
        Err(_) if child.try_wait()?.is_some() => Ok(None),
        Err(error) => Err(AppError::Io(error)),
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Executable {
    Azd,
    AzureCli,
    PowerShell,
    Git,
    #[cfg(test)]
    Test(&'static str),
}

impl Executable {
    fn command(self) -> &'static str {
        match self {
            Self::Azd => "azd",
            Self::AzureCli => {
                #[cfg(windows)]
                {
                    // The official Windows MSI exposes Azure CLI through this command shim.
                    "az.cmd"
                }
                #[cfg(not(windows))]
                {
                    "az"
                }
            }
            Self::PowerShell => "pwsh",
            Self::Git => "git",
            #[cfg(test)]
            Self::Test(command) => command,
        }
    }

    fn resolve(self, environment: &[(String, String)]) -> AppResult<PathBuf> {
        let command = self.command();
        let path = environment
            .iter()
            .rev()
            .find(|(key, _)| key.eq_ignore_ascii_case("PATH"))
            .map(|(_, value)| OsString::from(value))
            .or_else(|| std::env::var_os("PATH"))
            .unwrap_or_default();
        let extensions = executable_extensions(command, environment);
        for directory in std::env::split_paths(&path) {
            let directory = if directory.is_absolute() {
                directory
            } else if let Ok(current) = std::env::current_dir() {
                current.join(directory)
            } else {
                continue;
            };
            for extension in &extensions {
                let candidate = directory.join(format!("{command}{}", extension.to_string_lossy()));
                if is_executable_file(&candidate) {
                    return candidate.canonicalize().map_err(AppError::Io);
                }
            }
        }
        Err(AppError::MissingExecutable(command.into()))
    }
}

#[cfg(windows)]
fn executable_extensions(command: &str, environment: &[(String, String)]) -> Vec<OsString> {
    if Path::new(command).extension().is_some() {
        return vec![OsString::new()];
    }
    let path_extensions = environment
        .iter()
        .rev()
        .find(|(key, _)| key.eq_ignore_ascii_case("PATHEXT"))
        .map(|(_, value)| value.clone())
        .or_else(|| std::env::var("PATHEXT").ok())
        .unwrap_or_else(|| ".COM;.EXE;.BAT;.CMD".into());
    path_extensions
        .split(';')
        .filter(|value| !value.is_empty())
        .map(OsString::from)
        .collect()
}

#[cfg(not(windows))]
fn executable_extensions(_command: &str, _environment: &[(String, String)]) -> Vec<OsString> {
    vec![OsString::new()]
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = path.metadata() else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(windows)]
    {
        true
    }
}

fn display_executable(path: &Path) -> String {
    let display = path.to_string_lossy();
    display.strip_prefix(r"\\?\").unwrap_or(&display).into()
}

#[derive(Debug, Clone)]
pub struct CommandSpec {
    pub executable: Executable,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub environment: Vec<(String, String)>,
}

impl CommandSpec {
    pub fn new(executable: Executable, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            executable,
            args: args.into_iter().map(Into::into).collect(),
            cwd: None,
            environment: Vec::new(),
        }
    }

    pub fn cwd(mut self, value: impl Into<PathBuf>) -> Self {
        self.cwd = Some(value.into());
        self
    }

    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.environment.push((key.into(), value.into()));
        self
    }

    fn validate(&self) -> AppResult<()> {
        if self.args.len() > 128 {
            return Err(AppError::Invalid("Too many process arguments".into()));
        }
        if self
            .args
            .iter()
            .any(|arg| arg.contains('\0') || arg.len() > 16_384)
        {
            return Err(AppError::Invalid("Unsafe process argument".into()));
        }
        #[cfg(windows)]
        if matches!(self.executable, Executable::AzureCli)
            && self.args.iter().any(|arg| {
                arg.chars()
                    .any(|character| "&|<>^()%!\"\r\n".contains(character))
            })
        {
            return Err(AppError::Invalid(
                "Unsafe character in Azure CLI argument".into(),
            ));
        }
        if let Some(cwd) = &self.cwd {
            if !cwd.is_absolute() || !cwd.is_dir() {
                return Err(AppError::Invalid(format!(
                    "Working directory must be an existing absolute path: {}",
                    cwd.display()
                )));
            }
        }
        Ok(())
    }

    fn process(&self) -> AppResult<(Command, PathBuf)> {
        self.validate()?;
        let executable = self.executable.resolve(&self.environment)?;
        let mut command = Command::new(&executable);
        command
            .args(&self.args)
            .kill_on_drop(true)
            .stdin(Stdio::null());
        #[cfg(unix)]
        command.process_group(0);
        #[cfg(windows)]
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        if let Some(cwd) = &self.cwd {
            command.current_dir(cwd);
        }
        for (key, value) in &self.environment {
            command.env(key, value);
        }
        Ok((command, executable))
    }
}

#[derive(Clone, Default)]
pub struct OperationManager {
    cancellations: Arc<Mutex<HashMap<String, oneshot::Sender<()>>>>,
}

#[derive(Clone)]
pub struct OperationPersistence {
    database: Arc<Database>,
    project_path: String,
}

impl OperationPersistence {
    pub fn new(database: Arc<Database>, project_path: String) -> Self {
        Self {
            database,
            project_path,
        }
    }

    fn record(&self, event: &OperationEvent) {
        if let Err(error) = self.database.record_operation(event, &self.project_path) {
            eprintln!(
                "Could not persist operation {}: {error}",
                event.operation_id
            );
        }
    }
}

impl OperationManager {
    pub async fn cancel(&self, operation_id: &str) -> bool {
        self.cancellations
            .lock()
            .await
            .remove(operation_id)
            .map(|sender| sender.send(()).is_ok())
            .unwrap_or(false)
    }
}

#[derive(Debug)]
pub struct CapturedOutput {
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
    pub executable_path: String,
}

async fn read_bounded_output(
    mut reader: impl AsyncRead + Unpin,
    limit: usize,
) -> std::io::Result<Vec<u8>> {
    let mut captured = Vec::with_capacity(limit.min(64 * 1024));
    let mut chunk = [0_u8; 8 * 1024];
    let mut truncated = false;
    loop {
        let count = reader.read(&mut chunk).await?;
        if count == 0 {
            break;
        }
        let remaining = limit.saturating_sub(captured.len());
        captured.extend_from_slice(&chunk[..count.min(remaining)]);
        truncated |= count > remaining;
    }
    if truncated {
        let marker = OUTPUT_TRUNCATED_MARKER.as_bytes();
        captured.truncate(limit.saturating_sub(marker.len()));
        captured.extend_from_slice(&marker[..marker.len().min(limit)]);
    }
    Ok(captured)
}

#[derive(Default)]
struct BoundedLineBuffer {
    pending: Vec<u8>,
    truncated: bool,
}

impl BoundedLineBuffer {
    fn push(&mut self, bytes: &[u8]) -> Vec<String> {
        let mut lines = Vec::new();
        for byte in bytes {
            if *byte == b'\n' {
                lines.push(self.take_line());
            } else if self.pending.len() < MAX_STREAM_LINE_BYTES {
                self.pending.push(*byte);
            } else {
                self.truncated = true;
            }
        }
        lines
    }

    fn finish(&mut self) -> Option<String> {
        (!self.pending.is_empty() || self.truncated).then(|| self.take_line())
    }

    fn take_line(&mut self) -> String {
        if self.pending.last() == Some(&b'\r') {
            self.pending.pop();
        }
        let mut line = String::from_utf8_lossy(&self.pending).into_owned();
        if self.truncated {
            line.push_str(OUTPUT_TRUNCATED_MARKER.trim());
        }
        self.pending.clear();
        self.truncated = false;
        line
    }
}

pub async fn capture(spec: CommandSpec, timeout: Duration) -> AppResult<CapturedOutput> {
    let executable_name = spec.executable.command().to_string();
    let (mut command, executable) = spec.process()?;
    let executable_path = display_executable(&executable);
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            AppError::MissingExecutable(executable_name)
        } else {
            AppError::Io(error)
        }
    })?;
    let _tree = attach_process_tree(&mut child)?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AppError::Invalid("Child stdout was not captured".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| AppError::Invalid("Child stderr was not captured".into()))?;
    let (status, stdout, stderr) = tokio::time::timeout(timeout, async {
        tokio::join!(
            child.wait(),
            read_bounded_output(stdout, MAX_CAPTURE_BYTES_PER_STREAM),
            read_bounded_output(stderr, MAX_CAPTURE_BYTES_PER_STREAM)
        )
    })
    .await
    .map_err(|_| AppError::Timeout(executable_path.clone()))?;
    let status = status?;
    Ok(CapturedOutput {
        stdout: redact(&String::from_utf8_lossy(&stdout?)),
        stderr: redact(&String::from_utf8_lossy(&stderr?)),
        success: status.success(),
        executable_path,
    })
}

#[allow(clippy::too_many_arguments)]
async fn stream_output(
    mut reader: impl AsyncRead + Unpin,
    app: &AppHandle,
    operation_id: &str,
    kind: &str,
    stream: &str,
    level: &str,
    progress: Option<&AtomicU8>,
) {
    let mut chunk = [0_u8; 8 * 1024];
    let mut lines = BoundedLineBuffer::default();
    loop {
        let count = match reader.read(&mut chunk).await {
            Ok(0) | Err(_) => break,
            Ok(count) => count,
        };
        for line in lines.push(&chunk[..count]) {
            let inferred = progress.and_then(|value| {
                let inferred = inferred_progress(&line);
                if let Some(inferred) = inferred {
                    value.fetch_max(inferred, Ordering::Relaxed);
                }
                inferred
            });
            emit_event(
                app,
                operation_id,
                kind,
                "running",
                stream,
                level,
                line,
                inferred,
            );
        }
    }
    if let Some(line) = lines.finish() {
        emit_event(
            app,
            operation_id,
            kind,
            "running",
            stream,
            level,
            line,
            None,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_event(
    app: &AppHandle,
    operation_id: &str,
    kind: &str,
    state: &str,
    stream: &str,
    level: &str,
    message: impl Into<String>,
    progress: Option<u8>,
) -> OperationEvent {
    let event = OperationEvent {
        operation_id: operation_id.into(),
        kind: kind.into(),
        state: state.into(),
        timestamp: Utc::now().to_rfc3339(),
        stream: stream.into(),
        level: level.into(),
        message: redact(&message.into()),
        progress,
        outcome: terminal_outcome(kind, state),
    };
    let _ = app.emit("operation-event", event.clone());
    event
}

fn operation_timeout(kind: &str) -> Duration {
    match kind {
        "up" | "provision" | "deploy" | "down" => Duration::from_secs(4 * 60 * 60),
        "preview" => Duration::from_secs(60 * 60),
        "refresh" | "show" => Duration::from_secs(30 * 60),
        _ if kind.starts_with("login:") => Duration::from_secs(15 * 60),
        _ if kind.starts_with("install:") => Duration::from_secs(30 * 60),
        _ => Duration::from_secs(60 * 60),
    }
}

fn terminal_outcome(kind: &str, state: &str) -> Option<OperationOutcome> {
    let mutating = matches!(kind, "up" | "provision" | "deploy" | "down");
    let may_run_hooks = kind == "preview";
    if !mutating && !matches!(kind, "preview" | "refresh" | "show") {
        return None;
    }
    let action = match kind {
        "up" => "Deployment",
        "provision" => "Provisioning",
        "preview" => "Infrastructure preview",
        "deploy" => "Service deployment",
        "refresh" => "Environment refresh",
        "show" => "Project refresh",
        "down" => "Resource removal",
        _ => unreachable!("known lifecycle action"),
    };
    let (resource_state, summary, guidance, retryable) = match state {
        "succeeded" => (
            if mutating {
                "changed"
            } else if may_run_hooks {
                "possiblyModified"
            } else {
                "unchanged"
            },
            format!("{action} completed successfully."),
            if mutating {
                "Refresh project state to inspect the resulting Azure resources."
            } else if may_run_hooks {
                "Template hooks may have changed local files, Azure resources, or directory objects. Review the output and refresh project state before continuing."
            } else {
                "No Azure resource changes were applied by this action."
            }
            .into(),
            false,
        ),
        "failed" => (
            if mutating || may_run_hooks {
                "possiblyModified"
            } else {
                "unchanged"
            },
            format!("{action} did not complete."),
            if mutating || may_run_hooks {
                "Review the output and template hooks before retrying. Local files, Azure resources, or directory objects may contain partial changes."
            } else {
                "Review the output, correct the reported issue, and retry the same action."
            }
            .into(),
            true,
        ),
        "cancelled" => (
            if mutating || may_run_hooks {
                "possiblyModified"
            } else {
                "unchanged"
            },
            format!("{action} was cancelled."),
            if mutating || may_run_hooks {
                "The local process stopped, but template hooks or Azure operations already accepted may continue. Review the output and refresh state before retrying."
            } else {
                "No Azure resource changes were applied by this action. Retry when ready."
            }
            .into(),
            true,
        ),
        _ => return None,
    };
    Some(OperationOutcome {
        resource_state: resource_state.into(),
        summary,
        guidance,
        retryable,
    })
}

fn inferred_progress(message: &str) -> Option<u8> {
    let lower = message.to_ascii_lowercase();
    if lower.contains("provision") {
        Some(55)
    } else if lower.contains("package") {
        Some(75)
    } else if lower.contains("deploy") {
        Some(88)
    } else if lower.contains("success") || lower.contains("complete") {
        Some(98)
    } else {
        None
    }
}

pub fn spawn_operation(
    app: AppHandle,
    operations: OperationManager,
    kind: impl Into<String>,
    spec: CommandSpec,
    persistence: Option<OperationPersistence>,
) -> AppResult<String> {
    spec.validate()?;
    let operation_id = Uuid::new_v4().to_string();
    let kind = kind.into();
    emit_event(
        &app,
        &operation_id,
        &kind,
        "queued",
        "system",
        "info",
        "Operation queued.",
        Some(0),
    );

    let result_id = operation_id.clone();
    tauri::async_runtime::spawn(async move {
        let (cancel_sender, mut cancel_receiver) = oneshot::channel();
        operations
            .cancellations
            .lock()
            .await
            .insert(operation_id.clone(), cancel_sender);

        let (mut command, executable) = match spec.process() {
            Ok(command) => command,
            Err(error) => {
                let event = emit_event(
                    &app,
                    &operation_id,
                    &kind,
                    "failed",
                    "system",
                    "error",
                    error.to_string(),
                    None,
                );
                if let Some(persistence) = persistence.as_ref() {
                    persistence.record(&event);
                }
                operations.cancellations.lock().await.remove(&operation_id);
                return;
            }
        };
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                let message = if error.kind() == std::io::ErrorKind::NotFound {
                    format!(
                        "Required executable is not available: {}",
                        spec.executable.command()
                    )
                } else {
                    error.to_string()
                };
                let event = emit_event(
                    &app,
                    &operation_id,
                    &kind,
                    "failed",
                    "system",
                    "error",
                    message,
                    None,
                );
                if let Some(persistence) = persistence.as_ref() {
                    persistence.record(&event);
                }
                operations.cancellations.lock().await.remove(&operation_id);
                return;
            }
        };
        let tree = match attach_process_tree(&mut child) {
            Ok(tree) => tree,
            Err(error) => {
                let _ = child.kill().await;
                let event = emit_event(
                    &app,
                    &operation_id,
                    &kind,
                    "failed",
                    "system",
                    "error",
                    format!("Could not supervise the process tree: {error}"),
                    None,
                );
                if let Some(persistence) = persistence.as_ref() {
                    persistence.record(&event);
                }
                operations.cancellations.lock().await.remove(&operation_id);
                return;
            }
        };

        emit_event(
            &app,
            &operation_id,
            &kind,
            "running",
            "system",
            "info",
            format!("Started {}.", display_executable(&executable)),
            Some(2),
        );

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let last_progress = Arc::new(AtomicU8::new(2));
        let stdout_app = app.clone();
        let stdout_id = operation_id.clone();
        let stdout_kind = kind.clone();
        let stdout_progress = last_progress.clone();
        let mut stdout_task = tauri::async_runtime::spawn(async move {
            if let Some(stdout) = stdout {
                stream_output(
                    stdout,
                    &stdout_app,
                    &stdout_id,
                    &stdout_kind,
                    "stdout",
                    "info",
                    Some(&stdout_progress),
                )
                .await;
            }
        });
        let stderr_app = app.clone();
        let stderr_id = operation_id.clone();
        let stderr_kind = kind.clone();
        let mut stderr_task = tauri::async_runtime::spawn(async move {
            if let Some(stderr) = stderr {
                stream_output(
                    stderr,
                    &stderr_app,
                    &stderr_id,
                    &stderr_kind,
                    "stderr",
                    "warning",
                    None,
                )
                .await;
            }
        });

        let mut cancelled = false;
        let mut timed_out = false;
        let timeout = tokio::time::sleep(operation_timeout(&kind));
        tokio::pin!(timeout);
        let status = tokio::select! {
            value = child.wait() => value,
            _ = &mut cancel_receiver => {
                cancelled = true;
                if let Some(tree) = &tree {
                    tree.terminate();
                }
                let _ = child.kill().await;
                child.wait().await
            }
            _ = &mut timeout => {
                timed_out = true;
                if let Some(tree) = &tree {
                    tree.terminate();
                }
                let _ = child.kill().await;
                child.wait().await
            }
        };
        if tokio::time::timeout(Duration::from_secs(5), async {
            let _ = (&mut stdout_task).await;
            let _ = (&mut stderr_task).await;
        })
        .await
        .is_err()
        {
            stdout_task.abort();
            stderr_task.abort();
        }
        operations.cancellations.lock().await.remove(&operation_id);
        let terminal_progress = last_progress.load(Ordering::Relaxed);

        let event = if cancelled {
            emit_event(
                &app,
                &operation_id,
                &kind,
                "cancelled",
                "system",
                "warning",
                "Operation cancelled.",
                Some(terminal_progress),
            )
        } else if timed_out {
            emit_event(
                &app,
                &operation_id,
                &kind,
                "failed",
                "system",
                "error",
                "Operation exceeded its time limit and was stopped.",
                Some(terminal_progress),
            )
        } else {
            match status {
                Ok(status) if status.success() => emit_event(
                    &app,
                    &operation_id,
                    &kind,
                    "succeeded",
                    "system",
                    "success",
                    "Operation completed successfully.",
                    Some(100),
                ),
                Ok(status) => emit_event(
                    &app,
                    &operation_id,
                    &kind,
                    "failed",
                    "system",
                    "error",
                    format!("Process exited with status {status}."),
                    Some(terminal_progress),
                ),
                Err(error) => emit_event(
                    &app,
                    &operation_id,
                    &kind,
                    "failed",
                    "system",
                    "error",
                    error.to_string(),
                    Some(terminal_progress),
                ),
            }
        };
        if let Some(persistence) = persistence {
            persistence.record(&event);
        }
    });

    Ok(result_id)
}

#[cfg(test)]
mod tests {
    use super::{
        capture, operation_timeout, read_bounded_output, terminal_outcome, BoundedLineBuffer,
        CommandSpec, Executable, OUTPUT_TRUNCATED_MARKER,
    };
    use crate::error::AppError;
    use std::{path::PathBuf, time::Duration};
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn captured_output_is_bounded_and_drained() {
        let (mut writer, reader) = tokio::io::duplex(32);
        let writer_task = tokio::spawn(async move {
            writer.write_all(&vec![b'x'; 256]).await.unwrap();
        });

        let output = read_bounded_output(reader, 64).await.unwrap();
        writer_task.await.unwrap();

        assert_eq!(output.len(), 64);
        assert!(String::from_utf8_lossy(&output).ends_with(OUTPUT_TRUNCATED_MARKER));
    }

    #[test]
    fn streamed_lines_are_bounded_across_chunks() {
        let mut buffer = BoundedLineBuffer::default();
        assert!(buffer.push(&vec![b'x'; 64 * 1024]).is_empty());
        let lines = buffer.push(b"extra\nnext\n");

        assert_eq!(lines.len(), 2);
        assert!(lines[0].ends_with(OUTPUT_TRUNCATED_MARKER.trim()));
        assert!(lines[0].len() <= 64 * 1024 + OUTPUT_TRUNCATED_MARKER.trim().len());
        assert_eq!(lines[1], "next");
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn windows_azure_cli_uses_the_command_shim_from_path() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("az.cmd"),
            "@echo off\r\necho {\"azure-cli\":\"9.8.7\"}\r\n",
        )
        .unwrap();
        let output = capture(
            CommandSpec::new(Executable::AzureCli, ["version", "--output", "json"])
                .env("PATH", directory.path().display().to_string()),
            Duration::from_secs(5),
        )
        .await
        .unwrap();

        assert!(output.success);
        assert!(output.stdout.contains("9.8.7"));
        let resolved = PathBuf::from(&output.executable_path);
        assert!(resolved.is_absolute());
        assert_eq!(resolved.file_name().unwrap(), "az.cmd");
    }

    #[cfg(windows)]
    #[test]
    fn windows_azure_cli_rejects_command_shell_metacharacters() {
        let spec = CommandSpec::new(
            Executable::AzureCli,
            ["rest", "--url", "https://example.test/?safe=true&run=other"],
        );

        assert!(matches!(spec.validate(), Err(AppError::Invalid(_))));
    }

    #[cfg(windows)]
    fn shell(command: &'static str) -> CommandSpec {
        CommandSpec::new(Executable::Test("cmd"), ["/D", "/S", "/C", command])
    }

    #[cfg(not(windows))]
    fn shell(command: &'static str) -> CommandSpec {
        CommandSpec::new(Executable::Test("sh"), ["-c", command])
    }

    #[tokio::test]
    async fn fake_executable_captures_and_redacts_output() {
        let output = capture(
            shell("echo Authorization: Bearer abcdefghijklmnopqrstuvwxyz012345"),
            Duration::from_secs(5),
        )
        .await
        .unwrap();

        assert!(output.success);
        assert!(output.stdout.contains("[REDACTED]"));
        assert!(!output.stdout.contains("abcdefghijklmnopqrstuvwxyz012345"));
    }

    #[tokio::test]
    async fn fake_executable_timeout_is_reported() {
        #[cfg(windows)]
        let spec = shell("ping -n 3 127.0.0.1 >NUL");
        #[cfg(not(windows))]
        let spec = shell("sleep 1");

        let error = capture(spec, Duration::from_millis(10)).await.unwrap_err();
        assert!(matches!(error, AppError::Timeout(_)));
    }

    #[tokio::test]
    async fn missing_fake_executable_is_reported() {
        let error = capture(
            CommandSpec::new(
                Executable::Test("azd-gui-executable-that-does-not-exist"),
                std::iter::empty::<&str>(),
            ),
            Duration::from_secs(1),
        )
        .await
        .unwrap_err();
        assert!(matches!(error, AppError::MissingExecutable(_)));
    }

    #[test]
    fn failed_mutating_operation_reports_possible_partial_changes() {
        let outcome = terminal_outcome("up", "failed").unwrap();
        assert_eq!(outcome.resource_state, "possiblyModified");
        assert!(outcome.retryable);
        assert!(outcome.guidance.contains("partial changes"));
    }

    #[test]
    fn preview_outcomes_report_possible_hook_changes() {
        let succeeded = terminal_outcome("preview", "succeeded").unwrap();
        assert_eq!(succeeded.resource_state, "possiblyModified");
        assert!(succeeded.guidance.contains("Template hooks"));

        let outcome = terminal_outcome("preview", "failed").unwrap();
        assert_eq!(outcome.resource_state, "possiblyModified");
        assert!(outcome.retryable);

        let cancelled = terminal_outcome("preview", "cancelled").unwrap();
        assert_eq!(cancelled.resource_state, "possiblyModified");
    }

    #[test]
    fn non_lifecycle_operations_do_not_claim_resource_outcomes() {
        assert!(terminal_outcome("login:azd", "failed").is_none());
    }

    #[test]
    fn operation_deadlines_are_bounded_by_operation_kind() {
        assert_eq!(operation_timeout("login:azd"), Duration::from_secs(15 * 60));
        assert_eq!(operation_timeout("preview"), Duration::from_secs(60 * 60));
        assert_eq!(operation_timeout("up"), Duration::from_secs(4 * 60 * 60));
    }
}
