use crate::{
    command_runner::resolve_system_executable,
    deployment::validate_environment_name,
    error::{AppError, AppResult},
};
use serde::Serialize;
use std::{ffi::OsString, io, path::Path};

#[cfg(not(windows))]
use std::process::Command;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalLaunch {
    pub working_directory: String,
    pub command: String,
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
    let executable = wide(executable_path.as_os_str());
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

pub fn open(project_path: &str, environment: Option<&str>) -> AppResult<TerminalLaunch> {
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

    let platform = std::env::consts::OS;
    let candidates = providers(platform, &canonical);
    let mut unavailable = Vec::new();
    for candidate in candidates {
        let executable = match resolve_system_executable(candidate.program) {
            Ok(executable) => executable,
            Err(AppError::MissingExecutable(_)) => {
                unavailable.push(candidate.program);
                continue;
            }
            Err(error) => return Err(error),
        };
        match spawn(&candidate, &executable, &canonical) {
            Ok(()) => {
                let command = environment
                    .map(|value| format!("azd up --environment {value}"))
                    .unwrap_or_else(|| "azd up".into());
                return Ok(TerminalLaunch {
                    working_directory: canonical.display().to_string(),
                    command,
                });
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                unavailable.push(candidate.program)
            }
            Err(error) => return Err(AppError::Io(error)),
        }
    }

    Err(AppError::MissingExecutable(format!(
        "external terminal ({})",
        unavailable.join(", ")
    )))
}

#[cfg(test)]
mod tests {
    use super::{open, providers};
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
        assert!(open(directory.path().to_str().unwrap(), Some("dev")).is_err());

        std::fs::write(directory.path().join("azure.yaml"), "name: test").unwrap();
        assert!(open(
            directory.path().to_str().unwrap(),
            Some("unsafe environment")
        )
        .is_err());
    }
}
