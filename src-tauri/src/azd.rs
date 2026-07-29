use crate::{
    command_runner::{capture, CommandSpec, Executable},
    error::{AppError, AppResult},
};
use async_trait::async_trait;
use serde_json::Value;
use std::{path::Path, time::Duration};

#[async_trait]
#[allow(dead_code)]
pub trait AzdClient {
    async fn version(&self) -> AppResult<Value>;
    async fn auth_status(&self) -> AppResult<Value>;
    async fn environment_list(&self, project: &Path) -> AppResult<Value>;
    async fn show(&self, project: &Path, environment: Option<&str>) -> AppResult<Value>;
    fn lifecycle_command(
        &self,
        action: &str,
        project: &Path,
        environment: Option<&str>,
    ) -> AppResult<CommandSpec>;
}

#[derive(Default)]
pub struct SystemAzdClient;

async fn json_command(spec: CommandSpec) -> AppResult<Value> {
    let output = capture(spec, Duration::from_secs(20)).await?;
    if !output.success {
        return Err(AppError::Command(output.stderr));
    }
    serde_json::from_str(&output.stdout).map_err(AppError::from)
}

#[async_trait]
impl AzdClient for SystemAzdClient {
    async fn version(&self) -> AppResult<Value> {
        json_command(CommandSpec::new(
            Executable::Azd,
            ["version", "--output", "json"],
        ))
        .await
    }

    async fn auth_status(&self) -> AppResult<Value> {
        json_command(CommandSpec::new(
            Executable::Azd,
            ["auth", "status", "--output", "json"],
        ))
        .await
    }

    async fn environment_list(&self, project: &Path) -> AppResult<Value> {
        json_command(
            CommandSpec::new(
                Executable::Azd,
                ["env", "list", "--output", "json", "--no-prompt"],
            )
            .cwd(project),
        )
        .await
    }

    async fn show(&self, project: &Path, environment: Option<&str>) -> AppResult<Value> {
        let mut args: Vec<String> = vec![
            "show".into(),
            "--output".into(),
            "json".into(),
            "--no-prompt".into(),
        ];
        if let Some(environment) = environment {
            args.extend(["--environment".into(), environment.into()]);
        }
        json_command(CommandSpec::new(Executable::Azd, args).cwd(project)).await
    }

    fn lifecycle_command(
        &self,
        action: &str,
        project: &Path,
        environment: Option<&str>,
    ) -> AppResult<CommandSpec> {
        let mut args: Vec<String> = match action {
            "up" => vec!["up".into()],
            "provision" => vec!["provision".into()],
            "preview" => vec!["provision".into(), "--preview".into()],
            "deploy" => vec!["deploy".into(), "--all".into()],
            "refresh" => vec!["env".into(), "refresh".into()],
            "show" => vec!["show".into(), "--output".into(), "json".into()],
            "down" => vec!["down".into(), "--force".into()],
            _ => {
                return Err(AppError::Invalid(format!(
                    "Unsupported azd action: {action}"
                )))
            }
        };
        args.push("--no-prompt".into());
        if let Some(environment) = environment {
            args.extend(["--environment".into(), environment.into()]);
        }
        Ok(CommandSpec::new(Executable::Azd, args).cwd(project))
    }
}

#[cfg(test)]
mod tests {
    use super::{json_command, AzdClient, SystemAzdClient};
    use crate::command_runner::{CommandSpec, Executable};
    use std::path::Path;

    #[test]
    fn only_allows_known_lifecycle_commands() {
        let client = SystemAzdClient;
        assert!(client
            .lifecycle_command("up", Path::new("C:\\"), Some("dev"))
            .is_ok());
        assert!(client
            .lifecycle_command("arbitrary-shell", Path::new("C:\\"), None)
            .is_err());
    }

    #[tokio::test]
    async fn rejects_invalid_json_from_a_fake_cli() {
        #[cfg(windows)]
        let spec = CommandSpec::new(
            Executable::Test("cmd"),
            ["/D", "/S", "/C", "echo this-is-not-json"],
        );
        #[cfg(not(windows))]
        let spec = CommandSpec::new(Executable::Test("sh"), ["-c", "printf 'this-is-not-json'"]);

        assert!(json_command(spec).await.is_err());
    }
}
