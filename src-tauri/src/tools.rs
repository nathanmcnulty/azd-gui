use crate::{
    command_runner::{capture, CommandSpec, Executable},
    models::ToolStatus,
};
use serde_json::Value;
use std::time::Duration;

async fn version(
    executable: Executable,
    args: impl IntoIterator<Item = impl Into<String>>,
) -> Option<String> {
    let output = capture(CommandSpec::new(executable, args), Duration::from_secs(12))
        .await
        .ok()?;
    if !output.success {
        return None;
    }
    let value = output.stdout.trim();
    if value.starts_with('{') {
        let json: Value = serde_json::from_str(value).ok()?;
        for key in ["version", "azure-cli", "Version"] {
            if let Some(version) = json.get(key).and_then(Value::as_str) {
                return Some(version.into());
            }
        }
    }
    regex::Regex::new(r"\d+\.\d+(?:\.\d+){0,2}")
        .ok()?
        .find(value)
        .map(|value| value.as_str().to_string())
}

async fn powershell_module_version(name: &str) -> Option<String> {
    let spec = CommandSpec::new(
        Executable::PowerShell,
        [
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "$m=Get-Module -ListAvailable -Name $env:AZD_GUI_MODULE | Sort-Object Version -Descending | Select-Object -First 1; if($m){$m.Version.ToString()}",
        ],
    )
    .env("AZD_GUI_MODULE", name);
    let output = capture(spec, Duration::from_secs(15)).await.ok()?;
    output.success.then(|| output.stdout.trim().to_string())
}

pub async fn check_all() -> Vec<ToolStatus> {
    let (azd, az, pwsh, az_module, graph_module) = tokio::join!(
        version(Executable::Azd, ["version", "--output", "json"]),
        version(Executable::AzureCli, ["version", "--output", "json"]),
        version(
            Executable::PowerShell,
            [
                "-NoLogo",
                "-NoProfile",
                "-Command",
                "$PSVersionTable.PSVersion.ToString()"
            ]
        ),
        powershell_module_version("Az.Accounts"),
        powershell_module_version("Microsoft.Graph.Authentication")
    );
    vec![
        tool(
            "azd",
            "Azure Developer CLI",
            "tool",
            "1.23.0",
            azd,
            "manual",
            false,
            None,
        ),
        tool(
            "az-cli",
            "Azure CLI",
            "tool",
            "2.60.0",
            az,
            "azdTool",
            false,
            Some("az-cli"),
        ),
        tool(
            "pwsh",
            "PowerShell",
            "tool",
            "7.4.0",
            pwsh,
            "manual",
            false,
            None,
        ),
        tool(
            "Az.Accounts",
            "Azure PowerShell",
            "powershellModule",
            "3.0.0",
            az_module,
            "psResource",
            true,
            Some("Az.Accounts"),
        ),
        tool(
            "Microsoft.Graph",
            "Microsoft Graph PowerShell",
            "powershellModule",
            "2.0.0",
            graph_module,
            "psResource",
            true,
            Some("Microsoft.Graph"),
        ),
    ]
}

#[allow(clippy::too_many_arguments)]
fn tool(
    id: &str,
    name: &str,
    kind: &str,
    minimum: &str,
    installed: Option<String>,
    provider: &str,
    can_install: bool,
    install_id: Option<&str>,
) -> ToolStatus {
    let installed_version = installed
        .as_deref()
        .and_then(|value| semver::Version::parse(value.trim_start_matches('v')).ok());
    let required_version = semver::Version::parse(minimum).ok();
    let state = installed_version
        .zip(required_version)
        .map(|(installed, required)| {
            if installed >= required {
                "ready"
            } else {
                "outdated"
            }
        })
        .unwrap_or(if installed.is_some() {
            "ready"
        } else {
            "missing"
        });
    let install_command = match provider {
        "psResource" => install_id
            .map(|name| format!("Install-PSResource {name} -Scope CurrentUser -TrustRepository")),
        "azdTool" => install_id.map(|name| format!("azd tool install {name} --no-prompt")),
        _ => None,
    };
    ToolStatus {
        id: id.into(),
        name: name.into(),
        kind: kind.into(),
        minimum_version: Some(minimum.into()),
        required: id == "azd",
        state: state.into(),
        installed_version: installed.filter(|value| !value.is_empty()),
        detail: None,
        install_provider: Some(provider.into()),
        install_id: install_id.map(Into::into),
        documentation_url: None,
        install_command,
        can_install,
    }
}

pub fn install_spec(id: &str) -> Option<CommandSpec> {
    match id {
        "Az.Accounts" => Some(
            CommandSpec::new(
                Executable::PowerShell,
                [
                    "-NoLogo",
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    "Install-PSResource Az.Accounts -Scope CurrentUser -TrustRepository -Quiet -AcceptLicense",
                ],
            ),
        ),
        "Microsoft.Graph" => Some(
            CommandSpec::new(
                Executable::PowerShell,
                [
                    "-NoLogo",
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    "Install-PSResource Microsoft.Graph -Scope CurrentUser -TrustRepository -Quiet -AcceptLicense",
                ],
            ),
        ),
        _ => None,
    }
}
