use crate::{
    command_runner::resolve_system_executable,
    deployment::validate_environment_name,
    error::{AppError, AppResult},
};
use serde::Serialize;
use std::{ffi::OsString, io, path::Path};

fn shell_path(path: &Path) -> String {
    let path = path.to_string_lossy();
    if let Some(unc) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{unc}")
    } else {
        path.strip_prefix(r"\\?\").unwrap_or(&path).into()
    }
}

#[cfg(not(windows))]
use std::process::Command;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalLaunch {
    pub project_path: String,
    pub environment: Option<String>,
    pub working_directory: String,
    pub command: String,
    pub opened: bool,
    pub detail: Option<String>,
    pub historical: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TerminalProvider {
    program: &'static str,
    args: Vec<OsString>,
}

fn providers(platform: &str, project: &Path) -> Vec<TerminalProvider> {
    match platform {
        "windows" => vec![
            TerminalProvider {
                program: "wt.exe",
                args: vec!["-d".into(), project.as_os_str().into()],
            },
            TerminalProvider {
                program: "powershell.exe",
                args: vec!["-NoExit".into(), "-NoLogo".into()],
            },
        ],
        "macos" => vec![TerminalProvider {
            program: "open",
            args: vec!["-a".into(), "Terminal".into(), project.as_os_str().into()],
        }],
        _ => vec![
            TerminalProvider {
                program: "xdg-terminal-exec",
                args: vec![],
            },
            TerminalProvider {
                program: "x-terminal-emulator",
                args: vec![],
            },
            TerminalProvider {
                program: "gnome-terminal",
                args: vec!["--working-directory".into(), project.as_os_str().into()],
            },
            TerminalProvider {
                program: "konsole",
                args: vec!["--workdir".into(), project.as_os_str().into()],
            },
            TerminalProvider {
                program: "xfce4-terminal",
                args: vec!["--working-directory".into(), project.as_os_str().into()],
            },
            TerminalProvider {
                program: "kitty",
                args: vec!["--directory".into(), project.as_os_str().into()],
            },
        ],
    }
}

#[cfg(not(windows))]
fn spawn(provider: &TerminalProvider, executable: &Path, project: &Path) -> io::Result<()> {
    let mut command = Command::new(executable);
    command.args(&provider.args).current_dir(project);
    command.spawn().map(|_| ())
}

#[cfg(windows)]
fn spawn(provider: &TerminalProvider, executable_path: &Path, project: &Path) -> io::Result<()> {
    use std::{os::windows::ffi::OsStrExt, ptr};
    use windows_sys::Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL};

    let wide = |value: &std::ffi::OsStr| {
        value
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>()
    };
    let shell_executable = shell_path(executable_path);
    let executable = wide(std::ffi::OsStr::new(&shell_executable));
    let parameter_text = if provider.program == "wt.exe" {
        format!("-d \"{}\"", project.display())
    } else {
        provider
            .args
            .iter()
            .map(|value| value.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ")
    };
    let parameters = wide(std::ffi::OsStr::new(&parameter_text));
    let directory = wide(project.as_os_str());
    let result = unsafe {
        ShellExecuteW(
            ptr::null_mut(),
            ptr::null(),
            executable.as_ptr(),
            parameters.as_ptr(),
            directory.as_ptr(),
            SW_SHOWNORMAL,
        )
    };
    if result as isize <= 32 {
        let code = result as isize;
        let kind = if matches!(code, 2 | 3) {
            io::ErrorKind::NotFound
        } else {
            io::ErrorKind::Other
        };
        Err(io::Error::new(
            kind,
            format!("Windows could not open the external terminal (ShellExecuteW code {code})."),
        ))
    } else {
        Ok(())
    }
}

pub fn describe(project_path: &str, environment: Option<&str>) -> AppResult<TerminalLaunch> {
    let canonical = std::fs::canonicalize(project_path)?;
    if !canonical.join("azure.yaml").is_file() {
        return Err(AppError::Invalid(format!(
            "{} does not contain azure.yaml.",
            canonical.display()
        )));
    }
    if let Some(environment) = environment {
        validate_environment_name(environment)?;
    }

    let directory = canonical.display().to_string();
    Ok(TerminalLaunch {
        project_path: directory.clone(),
        environment: environment.map(str::to_string),
        working_directory: shell_path(&canonical),
        command: environment
            .map(|value| {
                if value.contains(['(', ')']) {
                    format!("azd up --environment '{value}'")
                } else {
                    format!("azd up --environment {value}")
                }
            })
            .unwrap_or_else(|| "azd up".into()),
        opened: false,
        detail: None,
        historical: false,
    })
}

fn launch_with(
    mut handoff: TerminalLaunch,
    platform: &str,
    resolve: impl Fn(&str) -> AppResult<std::path::PathBuf>,
    launch: impl Fn(&TerminalProvider, &Path, &Path) -> io::Result<()>,
) -> TerminalLaunch {
    let project = Path::new(&handoff.working_directory);
    let candidates = providers(platform, project);
    let mut unavailable = Vec::new();
    for candidate in candidates {
        let executable = match resolve(candidate.program) {
            Ok(executable) => executable,
            Err(error) => {
                // Windows app-execution aliases can exist but fail canonicalization
                // with ERROR_CANT_ACCESS_FILE (1920). Try the next fixed provider;
                // never change PATH, repair aliases or run the suggested azd command.
                unavailable.push(format!("{}: {error}", candidate.program));
                continue;
            }
        };
        match launch(&candidate, &executable, project) {
            Ok(()) => {
                handoff.opened = true;
                return handoff;
            }
            Err(error) => {
                unavailable.push(format!("{}: {error}", candidate.program));
            }
        }
    }
    handoff.detail = Some(format!("No terminal opened. {}", unavailable.join("; ")));
    handoff
}

pub fn launch(handoff: TerminalLaunch) -> TerminalLaunch {
    launch_with(
        handoff,
        std::env::consts::OS,
        resolve_system_executable,
        spawn,
    )
}

pub fn open_recorded(
    database: &crate::persistence::Database,
    project_path: &str,
    environment: Option<&str>,
) -> AppResult<TerminalLaunch> {
    open_recorded_with(database, project_path, environment, launch)
}

fn open_recorded_with(
    database: &crate::persistence::Database,
    project_path: &str,
    environment: Option<&str>,
    launch: impl FnOnce(TerminalLaunch) -> TerminalLaunch,
) -> AppResult<TerminalLaunch> {
    ensure_allowed(database, project_path)?;
    let mut handoff = describe(project_path, environment)?;
    let Some(project) = database.project_by_path(project_path)? else {
        return Ok(launch(handoff));
    };
    // This context binds history to a project/environment only. It carries no
    // lifecycle operation, approval or evidence authority for template reports.
    let context = crate::evidence::OperationEvidenceContext {
        project_id: project.id,
        environment: environment.unwrap_or_default().into(),
        azure_cloud: None,
        tenant_id: None,
        subscription_id: None,
        resource_group: None,
        template_id: project.template_id.unwrap_or_default(),
        revision: project.revision,
        contract_digest: None,
    };
    let id = uuid::Uuid::new_v4().to_string();
    if let Err(error) =
        database.begin_operation(&id, "terminalHandoff", &project.path, Some(&context))
    {
        handoff.detail = Some(format!("No terminal opened because handoff history is unavailable: {error}. Copy the instructions or retry the handoff; do not prepare again."));
        return Ok(handoff);
    }
    let mut result = launch(handoff);
    let event = crate::models::OperationEvent {
        operation_id: id, kind: "terminalHandoff".into(),
        state: if result.opened { "succeeded" } else { "failed" }.into(),
        timestamp: chrono::Utc::now().to_rfc3339(), stream: "system".into(),
        level: if result.opened { "info" } else { "error" }.into(),
        message: result.detail.clone().unwrap_or_else(|| "Terminal opened. Prepared, deployment not observed. No command was executed by the app.".into()),
        progress: None, environment_binding: None,
        outcome: Some(crate::models::OperationOutcome {
            resource_state: "unchanged".into(),
            summary: if result.opened { "Terminal opened. Prepared, deployment not observed." } else { "Terminal launch failed. Prepared, deployment not observed." }.into(),
            guidance: "Copy the command and project directory, or retry just the terminal handoff. Opening a terminal never runs the command or proves deployment.".into(),
            retryable: true,
        }),
    };
    if let Err(error) =
        database.record_operation_with_context(&event, &project.path, Some(&context))
    {
        result.detail = Some(format!(
            "{} History could not be saved: {error}. Do not repeat preparation or deployment.",
            result.detail.as_deref().unwrap_or("Terminal opened.")
        ));
    }
    Ok(result)
}

pub fn saved(
    database: &crate::persistence::Database,
    project_path: &str,
    environment: Option<&str>,
) -> AppResult<Option<TerminalLaunch>> {
    ensure_allowed(database, project_path)?;
    let Some(project) = database.project_by_path(project_path)? else {
        return Ok(None);
    };
    let Some(environment) = environment else {
        return Ok(None);
    };
    let last = database
        .operation_history_for_environment(&project.path, Some(environment))?
        .into_iter()
        .find(|event| {
            event.kind == "terminalHandoff"
                && event.environment_binding.as_deref() == Some("current")
        });
    if last.is_none()
        && (project.state != "prepared" || project.environment.as_deref() != Some(environment))
    {
        return Ok(None);
    }
    let mut handoff = describe(&project.path, Some(environment))?;
    handoff.historical = true;
    if let Some(event) = last {
        handoff.opened = event.state == "succeeded";
        handoff.detail = Some(event.message);
    }
    Ok(Some(handoff))
}

fn ensure_allowed(database: &crate::persistence::Database, project_path: &str) -> AppResult<()> {
    if let Some(project) = database.project_by_path(project_path)? {
        if let Some(contract) = database.project_template_exact(&project.path)? {
            crate::graph_session::reject_unadapted_execution(&contract)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{describe, launch_with, providers};
    use std::{ffi::OsString, path::Path};

    #[test]
    fn terminal_providers_are_fixed_by_platform() {
        let project = Path::new("/safe/project");
        let windows = providers("windows", project);
        assert_eq!(windows[0].program, "wt.exe");
        assert_eq!(windows[0].args[0], OsString::from("-d"));
        assert_eq!(windows[1].program, "powershell.exe");
        assert_eq!(windows[1].args[0], OsString::from("-NoExit"));

        let macos = providers("macos", project);
        assert_eq!(macos[0].program, "open");
        assert_eq!(macos[0].args[0], OsString::from("-a"));
        assert_eq!(macos[0].args[1], OsString::from("Terminal"));

        let linux = providers("linux", project);
        assert_eq!(linux[0].program, "xdg-terminal-exec");
        assert!(linux.iter().all(|provider| !provider.program.contains(' ')));
    }

    #[test]
    fn rejects_non_projects_and_invalid_environment_names_before_launch() {
        let directory = tempfile::tempdir().unwrap();
        assert!(describe(directory.path().to_str().unwrap(), Some("dev")).is_err());

        std::fs::write(directory.path().join("azure.yaml"), "name: test").unwrap();
        assert!(describe(
            directory.path().to_str().unwrap(),
            Some("unsafe environment")
        )
        .is_err());
    }

    #[test]
    fn broken_windows_alias_falls_back_without_executing_the_command() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("azure.yaml"), "name: test").unwrap();
        let result = launch_with(
            describe(directory.path().to_str().unwrap(), Some("test")).unwrap(),
            "windows",
            |name| {
                if name == "wt.exe" {
                    Err(crate::error::AppError::Io(
                        std::io::Error::from_raw_os_error(1920),
                    ))
                } else {
                    Ok(name.into())
                }
            },
            |provider, _, project| {
                assert_eq!(provider.program, "powershell.exe");
                assert_eq!(
                    provider.args,
                    [OsString::from("-NoExit"), OsString::from("-NoLogo")]
                );
                assert!(project.join("azure.yaml").is_file());
                Ok(())
            },
        );
        assert!(result.opened);
        assert_eq!(result.command, "azd up --environment test");
    }

    #[test]
    fn failed_handoff_survives_reopen_and_retry_changes_only_handoff_history() {
        use crate::{models::ProjectSummary, persistence::Database};
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("azure.yaml"), "name: test").unwrap();
        let path = directory.path().to_str().unwrap();
        let db_path = directory.path().join("app.sqlite");
        let db = Database::open(&db_path).unwrap();
        db.upsert_project(&ProjectSummary {
            id: "fixture".into(),
            name: "fixture".into(),
            path: path.into(),
            environment: Some("test".into()),
            template_id: None,
            revision: None,
            last_operation: None,
            state: "ready".into(),
        })
        .unwrap();
        super::open_recorded_with(&db, path, Some("test"), |mut handoff| {
            handoff.detail = Some("fixture launch failure".into());
            handoff
        })
        .unwrap();
        assert_eq!(db.project_by_path(path).unwrap().unwrap().state, "ready");
        drop(db);
        let db = Database::open(&db_path).unwrap();
        let failed = super::saved(&db, path, Some("test")).unwrap().unwrap();
        assert!(!failed.opened);
        assert!(failed.historical);
        assert_eq!(failed.detail.as_deref(), Some("fixture launch failure"));
        assert!(super::saved(&db, path, Some("other")).unwrap().is_none());
        super::open_recorded_with(&db, path, Some("test"), |mut handoff| {
            handoff.opened = true;
            handoff
        })
        .unwrap();
        assert_eq!(db.project_by_path(path).unwrap().unwrap().state, "ready");
        let history = db
            .operation_history_for_environment(path, Some("test"))
            .unwrap();
        assert_eq!(history.len(), 2);
        assert!(history.iter().all(|event| event.kind == "terminalHandoff"
            && event.environment_binding.as_deref() == Some("current")
            && event.outcome.as_ref().unwrap().resource_state == "unchanged"));
        assert!(
            super::saved(&db, path, Some("test"))
                .unwrap()
                .unwrap()
                .opened
        );
        assert!(!directory
            .path()
            .join("reports/deployment-receipt.json")
            .exists());
        let context = db.evidence_operations(path).unwrap()[0]
            .context
            .clone()
            .unwrap();
        db.begin_operation(
            "interrupted-handoff",
            "terminalHandoff",
            path,
            Some(&context),
        )
        .unwrap();
        drop(db);
        let db = Database::open(&db_path).unwrap();
        let interrupted = super::saved(&db, path, Some("test")).unwrap().unwrap();
        assert!(!interrupted.opened);
        assert!(interrupted.detail.unwrap().contains("interrupted"));
        assert_eq!(
            db.operation_history_for_environment(path, Some("test"))
                .unwrap()[0]
                .outcome
                .as_ref()
                .unwrap()
                .resource_state,
            "unchanged"
        );
        assert_eq!(db.project_by_path(path).unwrap().unwrap().state, "ready");
        assert_eq!(
            db.project_by_path(path).unwrap().unwrap().last_operation,
            None
        );
    }

    #[test]
    fn terminal_outcomes_preserve_running_and_completed_lifecycle_state() {
        use crate::{
            models::{OperationEvent, ProjectSummary},
            persistence::Database,
        };
        for state in ["running", "succeeded", "failed"] {
            let directory = tempfile::tempdir().unwrap();
            std::fs::write(directory.path().join("azure.yaml"), "name: test").unwrap();
            let path = directory.path().to_str().unwrap();
            let db = Database::open(&directory.path().join("app.sqlite")).unwrap();
            db.upsert_project(&ProjectSummary {
                id: "fixture".into(),
                name: "fixture".into(),
                path: path.into(),
                environment: Some("test".into()),
                template_id: None,
                revision: None,
                last_operation: None,
                state: "deploying".into(),
            })
            .unwrap();
            db.begin_operation("lifecycle", "up", path, None).unwrap();
            if state != "running" {
                db.record_operation(
                    &OperationEvent {
                        operation_id: "lifecycle".into(),
                        kind: "up".into(),
                        state: state.into(),
                        timestamp: "2026-11-01T00:00:00Z".into(),
                        stream: "system".into(),
                        level: "info".into(),
                        message: "Lifecycle result".into(),
                        progress: None,
                        outcome: None,
                        environment_binding: None,
                    },
                    path,
                )
                .unwrap();
            }
            let before = db.project_by_path(path).unwrap().unwrap();
            for opened in [false, true] {
                super::open_recorded_with(&db, path, Some("test"), |mut handoff| {
                    handoff.opened = opened;
                    handoff
                })
                .unwrap();
                let after = db.project_by_path(path).unwrap().unwrap();
                assert_eq!(after.state, before.state);
                assert_eq!(after.last_operation, before.last_operation);
                assert_eq!(
                    db.operation_history(path)
                        .unwrap()
                        .iter()
                        .find(|event| event.operation_id == "lifecycle")
                        .unwrap()
                        .state,
                    state
                );
            }
        }
    }

    #[test]
    fn changed_contract_blocks_restoring_and_retrying_saved_instructions() {
        use crate::{
            models::{ProjectSummary, ProjectTemplateSnapshot},
            persistence::Database,
        };
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("azure.yaml"), "name: test").unwrap();
        let path = directory.path().to_str().unwrap();
        let db = Database::open(&directory.path().join("app.sqlite")).unwrap();
        let mut entry = crate::catalog::entry_by_id("deployment-check").unwrap();
        let project = ProjectSummary {
            id: "fixture".into(),
            name: "fixture".into(),
            path: path.into(),
            environment: Some("test".into()),
            template_id: Some(entry.id.clone()),
            revision: Some(entry.commit.clone()),
            last_operation: None,
            state: "prepared".into(),
        };
        db.upsert_project(&project).unwrap();
        db.save_project_template(
            &project,
            &ProjectTemplateSnapshot::new(entry.clone()).unwrap(),
        )
        .unwrap();
        super::open_recorded_with(&db, path, Some("test"), |handoff| handoff).unwrap();
        assert!(super::saved(&db, path, Some("test")).unwrap().is_some());
        let manifest = entry.manifest.as_mut().unwrap();
        let mut graph = manifest.connections[0].clone();
        graph.id = "graphPowerShell".into();
        manifest.connections.push(graph);
        db.save_project_template(&project, &ProjectTemplateSnapshot::new(entry).unwrap())
            .unwrap();
        assert!(super::saved(&db, path, Some("test")).is_err());
        assert!(
            super::open_recorded_with(&db, path, Some("test"), |_| panic!(
                "blocked contract must never launch"
            ))
            .is_err()
        );
        assert_eq!(db.operation_history(path).unwrap().len(), 1);
    }

    #[test]
    fn shell_directories_and_parenthesized_environments_remain_copyable() {
        assert_eq!(super::shell_path(Path::new(r"\\?\C:\a b")), r"C:\a b");
        assert_eq!(
            super::shell_path(Path::new(r"\\?\UNC\server\share\a b")),
            r"\\server\share\a b"
        );
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("azure.yaml"), "name: test").unwrap();
        assert_eq!(
            describe(directory.path().to_str().unwrap(), Some("test(1)"))
                .unwrap()
                .command,
            "azd up --environment 'test(1)'"
        );
    }

    #[test]
    fn all_launch_failures_retain_validated_instructions() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("azure.yaml"), "name: test").unwrap();
        let result = launch_with(
            describe(directory.path().to_str().unwrap(), Some("test")).unwrap(),
            "windows",
            |name| Ok(name.into()),
            |_, _, _| Err(std::io::Error::from_raw_os_error(5)),
        );
        assert!(!result.opened);
        assert!(result.detail.unwrap().contains("powershell.exe"));
        assert_eq!(result.command, "azd up --environment test");
        assert!(Path::new(&result.working_directory)
            .join("azure.yaml")
            .is_file());
    }
}
