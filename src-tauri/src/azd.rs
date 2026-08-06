use crate::{
    command_runner::{capture, CommandSpec, Executable},
    deployment::{validate_environment_name, validate_location, validate_optional_guid},
    error::{AppError, AppResult},
    models::ProjectEnvironment,
};
use async_trait::async_trait;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::{path::Path, time::Duration};

#[async_trait]
#[allow(dead_code)]
pub trait AzdClient {
    async fn version(&self) -> AppResult<Value>;
    async fn auth_status(&self) -> AppResult<Value>;
    async fn environment_list(&self, project: &Path) -> AppResult<Vec<ProjectEnvironment>>;
    async fn environment_new(
        &self,
        project: &Path,
        name: &str,
        subscription: Option<&str>,
        location: Option<&str>,
    ) -> AppResult<()>;
    async fn environment_select(&self, project: &Path, name: &str) -> AppResult<()>;
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

async fn command_stdout(spec: CommandSpec) -> AppResult<String> {
    let output = capture(spec, Duration::from_secs(20)).await?;
    if !output.success {
        return Err(AppError::Command(output.stderr));
    }
    Ok(output.stdout)
}

async fn json_command<T: DeserializeOwned>(spec: CommandSpec) -> AppResult<T> {
    serde_json::from_str(&command_stdout(spec).await?).map_err(AppError::from)
}

fn environment_new_command(
    project: &Path,
    name: &str,
    subscription: Option<&str>,
    location: Option<&str>,
) -> AppResult<CommandSpec> {
    validate_environment_name(name)?;
    validate_optional_guid("Subscription ID", subscription)?;
    if let Some(location) = location.filter(|value| !value.is_empty()) {
        validate_location(location)?;
    }
    let mut args: Vec<String> = vec![
        "env".into(),
        "new".into(),
        name.into(),
        "--no-prompt".into(),
    ];
    if let Some(subscription) = subscription.filter(|value| !value.is_empty()) {
        args.extend(["--subscription".into(), subscription.into()]);
    }
    if let Some(location) = location.filter(|value| !value.is_empty()) {
        args.extend(["--location".into(), location.into()]);
    }
    Ok(CommandSpec::new(Executable::Azd, args).cwd(project))
}

fn environment_select_command(project: &Path, name: &str) -> AppResult<CommandSpec> {
    validate_environment_name(name)?;
    Ok(CommandSpec::new(Executable::Azd, ["env", "select", name, "--no-prompt"]).cwd(project))
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

    async fn environment_list(&self, project: &Path) -> AppResult<Vec<ProjectEnvironment>> {
        json_command(
            CommandSpec::new(
                Executable::Azd,
                ["env", "list", "--output", "json", "--no-prompt"],
            )
            .cwd(project),
        )
        .await
    }

    async fn environment_new(
        &self,
        project: &Path,
        name: &str,
        subscription: Option<&str>,
        location: Option<&str>,
    ) -> AppResult<()> {
        command_stdout(environment_new_command(
            project,
            name,
            subscription,
            location,
        )?)
        .await?;
        Ok(())
    }

    async fn environment_select(&self, project: &Path, name: &str) -> AppResult<()> {
        command_stdout(environment_select_command(project, name)?).await?;
        Ok(())
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
    use super::{
        environment_new_command, environment_select_command, json_command, AzdClient,
        SystemAzdClient,
    };
    use crate::command_runner::{CommandSpec, Executable};
    use crate::models::ProjectEnvironment;
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

    #[test]
    fn builds_only_typed_environment_commands() {
        let project = std::env::temp_dir();
        let subscription = "11111111-1111-1111-1111-111111111111";
        let new_spec = environment_new_command(
            &project,
            "staging.us_(1)",
            Some(subscription),
            Some("westus2"),
        )
        .unwrap();
        assert_eq!(
            new_spec.args,
            [
                "env",
                "new",
                "staging.us_(1)",
                "--no-prompt",
                "--subscription",
                subscription,
                "--location",
                "westus2"
            ]
        );
        assert_eq!(
            environment_select_command(&project, "staging")
                .unwrap()
                .args,
            ["env", "select", "staging", "--no-prompt"]
        );
        assert!(environment_select_command(&project, "unsafe environment").is_err());
    }

    #[test]
    fn parses_the_public_environment_list_contract() {
        let environments: Vec<ProjectEnvironment> = serde_json::from_str(
            r#"[{"Name":"dev","DotEnvPath":"C:\\project\\.azure\\dev\\.env","HasLocal":true,"HasRemote":false,"IsDefault":true}]"#,
        )
        .unwrap();
        assert_eq!(environments.len(), 1);
        assert_eq!(environments[0].name, "dev");
        assert!(environments[0].has_local);
        assert!(environments[0].is_default);
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

        assert!(json_command::<serde_json::Value>(spec).await.is_err());
    }
}
