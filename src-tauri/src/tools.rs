use crate::{
    command_runner::{capture, CommandSpec, Executable},
    models::{ToolInstallPlan, ToolStatus},
};
use serde::Deserialize;
use serde_json::Value;
use std::{collections::HashMap, time::Duration};

#[derive(Debug, Deserialize)]
struct AzdToolInventoryItem {
    id: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    version: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AzdToolUpdateItem {
    id: String,
    #[serde(default)]
    latest_version: String,
    #[serde(default)]
    update_available: bool,
}

fn parse_azd_tool_inventory(value: &str) -> Option<HashMap<String, AzdToolInventoryItem>> {
    let items: Vec<AzdToolInventoryItem> = serde_json::from_str(value).ok()?;
    Some(
        items
            .into_iter()
            .map(|item| (item.id.clone(), item))
            .collect(),
    )
}

fn parse_azd_tool_updates(value: &str) -> Option<HashMap<String, AzdToolUpdateItem>> {
    let items: Vec<AzdToolUpdateItem> = serde_json::from_str(value).ok()?;
    Some(
        items
            .into_iter()
            .map(|item| (item.id.clone(), item))
            .collect(),
    )
}

async fn azd_tool_inventory() -> Option<HashMap<String, AzdToolInventoryItem>> {
    let output = capture(
        CommandSpec::new(
            Executable::Azd,
            ["tool", "list", "--output", "json", "--no-prompt"],
        ),
        Duration::from_secs(30),
    )
    .await
    .ok()?;
    output
        .success
        .then(|| parse_azd_tool_inventory(output.stdout.trim()))
        .flatten()
}

async fn azd_tool_updates() -> Option<HashMap<String, AzdToolUpdateItem>> {
    let output = capture(
        CommandSpec::new(
            Executable::Azd,
            ["tool", "check", "--output", "json", "--no-prompt"],
        ),
        Duration::from_secs(30),
    )
    .await
    .ok()?;
    output
        .success
        .then(|| parse_azd_tool_updates(output.stdout.trim()))
        .flatten()
}

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

pub async fn check_all(include_azd_updates: bool) -> Vec<ToolStatus> {
    let (azd, az, pwsh, az_module, graph_module, azd_inventory, azd_updates) = tokio::join!(
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
        powershell_module_version("Microsoft.Graph.Authentication"),
        async {
            if include_azd_updates {
                azd_tool_inventory().await
            } else {
                None
            }
        },
        async {
            if include_azd_updates {
                azd_tool_updates().await
            } else {
                None
            }
        }
    );
    let azd_inventory = azd_inventory.unwrap_or_default();
    let azd_updates = azd_updates.unwrap_or_default();
    let inventory_azure_cli = azd_inventory.get("az-cli").and_then(|item| {
        (item.status.eq_ignore_ascii_case("installed") && !item.version.is_empty())
            .then(|| item.version.clone())
    });
    let mut azure_cli = tool(
        "az-cli",
        "Azure CLI",
        "tool",
        "2.60.0",
        inventory_azure_cli.or(az),
        "azdTool",
        Some("az-cli"),
    );
    if let Some(update) = azd_updates.get("az-cli") {
        azure_cli.latest_version =
            (!update.latest_version.is_empty()).then(|| update.latest_version.clone());
        azure_cli.update_available = update.update_available;
        if update.update_available {
            azure_cli.detail = Some(
                "A newer version is available. Preview the platform-specific update with azd tool before applying it in a terminal."
                    .into(),
            );
            azure_cli.install_command = Some("azd tool upgrade az-cli --dry-run".into());
        }
    }
    vec![
        tool(
            "azd",
            "Azure Developer CLI",
            "tool",
            "1.23.0",
            azd,
            "manual",
            None,
        ),
        azure_cli,
        tool("pwsh", "PowerShell", "tool", "7.4.0", pwsh, "manual", None),
        tool(
            "Az.Accounts",
            "Azure PowerShell",
            "powershellModule",
            "3.0.0",
            az_module,
            "psResource",
            Some("Az.Accounts"),
        ),
        tool(
            "Microsoft.Graph",
            "Microsoft Graph PowerShell",
            "powershellModule",
            "2.0.0",
            graph_module,
            "psResource",
            Some("Microsoft.Graph"),
        ),
    ]
}

fn tool(
    id: &str,
    name: &str,
    kind: &str,
    minimum: &str,
    installed: Option<String>,
    provider: &str,
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
    let install_plan = install_plan(id);
    let install_command = install_plan
        .as_ref()
        .map(|plan| plan.command.clone())
        .or_else(|| match provider {
            "azdTool" => install_id.map(|name| format!("azd tool install {name} --no-prompt")),
            _ => None,
        });
    let documentation_url = install_plan
        .as_ref()
        .map(|plan| plan.documentation_url.clone())
        .or_else(|| documentation_url(id).map(Into::into));
    ToolStatus {
        id: id.into(),
        name: name.into(),
        kind: kind.into(),
        minimum_version: Some(minimum.into()),
        required: id == "azd",
        state: state.into(),
        installed_version: installed.filter(|value| !value.is_empty()),
        latest_version: None,
        update_available: false,
        detail: None,
        install_provider: Some(provider.into()),
        install_id: install_id.map(Into::into),
        documentation_url,
        install_command,
        can_install: install_plan.is_some(),
    }
}

fn documentation_url(id: &str) -> Option<&'static str> {
    match id {
        "azd" => {
            Some("https://learn.microsoft.com/azure/developer/azure-developer-cli/install-azd")
        }
        "az-cli" => Some(
            "https://learn.microsoft.com/azure/developer/azure-developer-cli/development-tools",
        ),
        "pwsh" => {
            Some("https://learn.microsoft.com/powershell/scripting/install/installing-powershell")
        }
        _ => None,
    }
}

struct ApprovedModuleInstall {
    id: &'static str,
    name: &'static str,
}

impl ApprovedModuleInstall {
    fn script(&self) -> String {
        format!(
            "Install-PSResource {} -Scope CurrentUser -TrustRepository -Quiet -AcceptLicense",
            self.id
        )
    }

    fn plan(&self) -> ToolInstallPlan {
        let script = self.script();
        ToolInstallPlan {
            id: self.id.into(),
            tool_name: self.name.into(),
            provider: "PSResourceGet".into(),
            action: format!("Install or update the {} module", self.name),
            source: "PowerShell Gallery (PSGallery)".into(),
            scope: "Current user".into(),
            destination: "Current-user PowerShell module path".into(),
            command: format!(
                "pwsh -NoLogo -NoProfile -NonInteractive -Command \"{script}\""
            ),
            documentation_url: "https://learn.microsoft.com/powershell/module/microsoft.powershell.psresourceget/install-psresource?view=powershellget-3.x".into(),
        }
    }

    fn spec(&self) -> CommandSpec {
        CommandSpec::new(
            Executable::PowerShell,
            [
                "-NoLogo".into(),
                "-NoProfile".into(),
                "-NonInteractive".into(),
                "-Command".into(),
                self.script(),
            ],
        )
    }
}

fn approved_install(id: &str) -> Option<ApprovedModuleInstall> {
    match id {
        "Az.Accounts" => Some(ApprovedModuleInstall {
            id: "Az.Accounts",
            name: "Azure PowerShell",
        }),
        "Microsoft.Graph" => Some(ApprovedModuleInstall {
            id: "Microsoft.Graph",
            name: "Microsoft Graph PowerShell",
        }),
        _ => None,
    }
}

pub fn install_plan(id: &str) -> Option<ToolInstallPlan> {
    approved_install(id).map(|install| install.plan())
}

pub fn install_spec(id: &str) -> Option<CommandSpec> {
    approved_install(id).map(|install| install.spec())
}

#[cfg(test)]
mod tests {
    use super::{
        install_plan, install_spec, parse_azd_tool_inventory, parse_azd_tool_updates, tool,
    };

    #[test]
    fn parses_public_azd_tool_inventory_and_update_contracts() {
        let inventory = parse_azd_tool_inventory(
            r#"[{"id":"az-cli","name":"Azure CLI","category":"cli","priority":"recommended","status":"Installed","version":"2.85.0"}]"#,
        )
        .unwrap();
        let updates = parse_azd_tool_updates(
            r#"[{"id":"az-cli","name":"Azure CLI","installedVersion":"2.85.0","latestVersion":"2.89.0","updateAvailable":true}]"#,
        )
        .unwrap();

        assert_eq!(inventory["az-cli"].status, "Installed");
        assert_eq!(inventory["az-cli"].version, "2.85.0");
        assert_eq!(updates["az-cli"].latest_version, "2.89.0");
        assert!(updates["az-cli"].update_available);
    }

    #[test]
    fn rejects_invalid_azd_tool_json_without_affecting_fallback_detection() {
        assert!(parse_azd_tool_inventory("not json").is_none());
        assert!(parse_azd_tool_updates("{}").is_none());
    }

    #[test]
    fn approved_install_plan_matches_the_executed_script() {
        let plan = install_plan("Microsoft.Graph").unwrap();
        let spec = install_spec("Microsoft.Graph").unwrap();

        assert_eq!(plan.provider, "PSResourceGet");
        assert_eq!(plan.source, "PowerShell Gallery (PSGallery)");
        assert_eq!(plan.scope, "Current user");
        assert!(plan.command.contains(spec.args.last().unwrap()));
        assert!(plan.command.contains("-Quiet -AcceptLicense"));
    }

    #[test]
    fn unapproved_installers_do_not_produce_plans_or_commands() {
        for id in ["azd", "az-cli", "pwsh", "manifest-supplied"] {
            assert!(install_plan(id).is_none());
            assert!(install_spec(id).is_none());
        }
    }

    #[test]
    fn user_directed_tools_keep_copyable_official_guidance() {
        let status = tool(
            "az-cli",
            "Azure CLI",
            "tool",
            "2.60.0",
            None,
            "azdTool",
            Some("az-cli"),
        );

        assert!(!status.can_install);
        assert_eq!(
            status.install_command.as_deref(),
            Some("azd tool install az-cli --no-prompt")
        );
        assert!(status
            .documentation_url
            .as_deref()
            .unwrap()
            .contains("development-tools"));
    }
}
