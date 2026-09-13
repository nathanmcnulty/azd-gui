use crate::{
    command_runner::{capture, CommandSpec, Executable},
    deployment::{validate_environment_name, validate_location, validate_optional_guid},
    error::{AppError, AppResult},
    models::ProjectEnvironment,
};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::{io::Write, path::Path, time::Duration};

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

fn validate_environment_value_name(value: &str) -> AppResult<()> {
    let mut characters = value.chars();
    if value.len() > 128
        || !characters
            .next()
            .is_some_and(|character| character.is_ascii_uppercase())
        || !characters.all(|character| {
            character.is_ascii_uppercase() || character.is_ascii_digit() || character == '_'
        })
    {
        return Err(AppError::Invalid(format!(
            "Invalid environment variable name: {value}"
        )));
    }
    Ok(())
}

fn environment_get_value_command(
    project: &Path,
    environment: &str,
    key: &str,
) -> AppResult<CommandSpec> {
    validate_environment_name(environment)?;
    validate_environment_value_name(key)?;
    Ok(CommandSpec::new(
        Executable::Azd,
        [
            "env",
            "get-value",
            key,
            "--environment",
            environment,
            "--no-prompt",
        ],
    )
    .cwd(project))
}

fn environment_set_file_command(
    project: &Path,
    environment: &str,
    file: &Path,
) -> AppResult<CommandSpec> {
    validate_environment_name(environment)?;
    Ok(CommandSpec::new(
        Executable::Azd,
        [
            "env".into(),
            "set".into(),
            "--file".into(),
            file.display().to_string(),
            "--environment".into(),
            environment.into(),
            "--no-prompt".into(),
        ],
    )
    .cwd(project))
}

pub async fn auth_status() -> AppResult<Value> {
    json_command(CommandSpec::new(
        Executable::Azd,
        ["auth", "status", "--output", "json"],
    ))
    .await
}

pub async fn environment_list(project: &Path) -> AppResult<Vec<ProjectEnvironment>> {
    json_command(
        CommandSpec::new(
            Executable::Azd,
            ["env", "list", "--output", "json", "--no-prompt"],
        )
        .cwd(project),
    )
    .await
}

pub async fn environment_new(
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

pub async fn environment_select(project: &Path, name: &str) -> AppResult<()> {
    command_stdout(environment_select_command(project, name)?).await?;
    Ok(())
}

pub async fn environment_get_value(
    project: &Path,
    environment: &str,
    key: &str,
) -> AppResult<String> {
    Ok(
        command_stdout(environment_get_value_command(project, environment, key)?)
            .await?
            .trim_end()
            .to_string(),
    )
}

pub async fn environment_set_values(
    project: &Path,
    environment: &str,
    values: &[(String, String)],
) -> AppResult<()> {
    if values.is_empty() || values.len() > 64 {
        return Err(AppError::Invalid(
            "Provide between 1 and 64 environment values.".into(),
        ));
    }
    let mut file = tempfile::Builder::new().prefix("azd-gui-env-").tempfile()?;
    for (name, value) in values {
        validate_environment_value_name(name)?;
        if value.len() > 32_768 {
            return Err(AppError::Invalid(format!(
                "Environment value is too large: {name}"
            )));
        }
        writeln!(file, "{}={}", name, serde_json::to_string(value)?)?;
    }
    file.flush()?;
    let output = capture(
        environment_set_file_command(project, environment, file.path())?,
        Duration::from_secs(30),
    )
    .await?;
    if !output.success {
        return Err(AppError::Command(
            "azd rejected an environment value; values were withheld from diagnostics.".into(),
        ));
    }
    Ok(())
}

pub async fn show(project: &Path, environment: Option<&str>) -> AppResult<Value> {
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

pub fn lifecycle_command(
    action: &str,
    project: &Path,
    environment: Option<&str>,
) -> AppResult<CommandSpec> {
    let requires_environment = matches!(action, "up" | "provision" | "preview" | "deploy" | "down");
    if requires_environment {
        let environment = environment.ok_or_else(|| {
            AppError::Invalid("A lifecycle action requires an explicit environment.".into())
        })?;
        validate_environment_name(environment)?;
    } else if let Some(environment) = environment {
        validate_environment_name(environment)?;
    }
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

#[cfg(test)]
mod tests {
    use super::{
        environment_get_value_command, environment_new_command, environment_select_command,
        environment_set_file_command, json_command, lifecycle_command,
    };
    use crate::command_runner::{CommandSpec, Executable};
    use crate::models::ProjectEnvironment;
    use std::path::Path;

    #[test]
    fn only_allows_known_lifecycle_commands() {
        let up = lifecycle_command("up", Path::new("C:\\"), Some("dev")).unwrap();
        assert_eq!(up.args, ["up", "--no-prompt", "--environment", "dev"]);

        let preview = lifecycle_command("preview", Path::new("C:\\"), Some("staging")).unwrap();
        assert_eq!(
            preview.args,
            [
                "provision",
                "--preview",
                "--no-prompt",
                "--environment",
                "staging"
            ]
        );

        assert!(lifecycle_command("arbitrary-shell", Path::new("C:\\"), None).is_err());
    }

    #[test]
    fn mutating_lifecycle_commands_require_a_valid_explicit_environment() {
        for action in ["preview", "up", "provision", "deploy", "down"] {
            assert!(lifecycle_command(action, Path::new("C:\\"), None).is_err());
            assert!(
                lifecycle_command(action, Path::new("C:\\"), Some("unsafe environment")).is_err()
            );
        }

        assert!(lifecycle_command("refresh", Path::new("C:\\"), None).is_ok());
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
        assert_eq!(
            environment_get_value_command(&project, "staging", "MAESTER_SCHEDULE")
                .unwrap()
                .args,
            [
                "env",
                "get-value",
                "MAESTER_SCHEDULE",
                "--environment",
                "staging",
                "--no-prompt"
            ]
        );
        let set_spec = environment_set_file_command(
            &project,
            "staging",
            &project.join("azd-gui-env-input.env"),
        )
        .unwrap();
        assert!(set_spec.args.contains(&"--file".to_string()));
        assert!(!set_spec
            .args
            .iter()
            .any(|argument| argument.contains("secret-value")));
        assert!(environment_get_value_command(&project, "staging", "unsafe-key").is_err());
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
