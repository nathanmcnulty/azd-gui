use crate::{
    error::{AppError, AppResult},
    models::OperationEvent,
    redaction::redact,
};
use chrono::Utc;
use std::{collections::HashMap, path::PathBuf, process::Stdio, sync::Arc, time::Duration};
use tauri::{AppHandle, Emitter};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    sync::{oneshot, Mutex},
};
use uuid::Uuid;

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
            Self::AzureCli => "az",
            Self::PowerShell => "pwsh",
            Self::Git => "git",
            #[cfg(test)]
            Self::Test(command) => command,
        }
    }
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

    fn process(&self) -> AppResult<Command> {
        self.validate()?;
        let mut command = Command::new(self.executable.command());
        command
            .args(&self.args)
            .kill_on_drop(true)
            .stdin(Stdio::null());
        if let Some(cwd) = &self.cwd {
            command.current_dir(cwd);
        }
        for (key, value) in &self.environment {
            command.env(key, value);
        }
        Ok(command)
    }
}

#[derive(Clone, Default)]
pub struct OperationManager {
    cancellations: Arc<Mutex<HashMap<String, oneshot::Sender<()>>>>,
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
}

pub async fn capture(spec: CommandSpec, timeout: Duration) -> AppResult<CapturedOutput> {
    let executable = spec.executable.command().to_string();
    let mut command = spec.process()?;
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let output = tokio::time::timeout(timeout, command.output())
        .await
        .map_err(|_| AppError::Timeout(executable.clone()))?
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                AppError::MissingExecutable(executable)
            } else {
                AppError::Io(error)
            }
        })?;
    Ok(CapturedOutput {
        stdout: redact(&String::from_utf8_lossy(&output.stdout)),
        stderr: redact(&String::from_utf8_lossy(&output.stderr)),
        success: output.status.success(),
    })
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
) {
    let _ = app.emit(
        "operation-event",
        OperationEvent {
            operation_id: operation_id.into(),
            kind: kind.into(),
            state: state.into(),
            timestamp: Utc::now().to_rfc3339(),
            stream: stream.into(),
            level: level.into(),
            message: redact(&message.into()),
            progress,
        },
    );
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

        let mut command = match spec.process() {
            Ok(command) => command,
            Err(error) => {
                emit_event(
                    &app,
                    &operation_id,
                    &kind,
                    "failed",
                    "system",
                    "error",
                    error.to_string(),
                    None,
                );
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
                emit_event(
                    &app,
                    &operation_id,
                    &kind,
                    "failed",
                    "system",
                    "error",
                    message,
                    None,
                );
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
            format!("Started {}.", spec.executable.command()),
            Some(2),
        );

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let stdout_app = app.clone();
        let stdout_id = operation_id.clone();
        let stdout_kind = kind.clone();
        let stdout_task = tauri::async_runtime::spawn(async move {
            if let Some(stdout) = stdout {
                let mut lines = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    emit_event(
                        &stdout_app,
                        &stdout_id,
                        &stdout_kind,
                        "running",
                        "stdout",
                        "info",
                        &line,
                        inferred_progress(&line),
                    );
                }
            }
        });
        let stderr_app = app.clone();
        let stderr_id = operation_id.clone();
        let stderr_kind = kind.clone();
        let stderr_task = tauri::async_runtime::spawn(async move {
            if let Some(stderr) = stderr {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    emit_event(
                        &stderr_app,
                        &stderr_id,
                        &stderr_kind,
                        "running",
                        "stderr",
                        "warning",
                        line,
                        None,
                    );
                }
            }
        });

        let mut cancelled = false;
        let status = tokio::select! {
            value = child.wait() => value,
            _ = &mut cancel_receiver => {
                cancelled = true;
                let _ = child.kill().await;
                child.wait().await
            }
        };
        let _ = stdout_task.await;
        let _ = stderr_task.await;
        operations.cancellations.lock().await.remove(&operation_id);

        if cancelled {
            emit_event(
                &app,
                &operation_id,
                &kind,
                "cancelled",
                "system",
                "warning",
                "Operation cancelled.",
                None,
            );
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
                    None,
                ),
                Err(error) => emit_event(
                    &app,
                    &operation_id,
                    &kind,
                    "failed",
                    "system",
                    "error",
                    error.to_string(),
                    None,
                ),
            }
        }
    });

    Ok(result_id)
}

#[cfg(test)]
mod tests {
    use super::{capture, CommandSpec, Executable};
    use crate::error::AppError;
    use std::time::Duration;

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
}
