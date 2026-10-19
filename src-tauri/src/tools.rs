use crate::{
    command_runner::{capture, CommandSpec, Executable},
    error::{AppError, AppResult},
    models::{
        Prerequisite, TemplateManifest, ToolInstallApprovalGrant, ToolInstallPlan, ToolStatus,
    },
};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};
use uuid::Uuid;

const INSTALL_APPROVAL_LIFETIME: Duration = Duration::from_secs(5 * 60);

#[derive(Clone)]
struct PendingToolInstall {
    id: String,
    exact_version: String,
    expires_at: Instant,
}

#[derive(Default)]
pub struct ToolInstallApprovals {
    pending: Mutex<HashMap<String, PendingToolInstall>>,
}

impl ToolInstallApprovals {
    pub fn clear(&self) -> AppResult<()> {
        self.pending
            .lock()
            .map_err(|_| {
                AppError::Invalid("Tool installation approval cache is unavailable.".into())
            })?
            .clear();
        Ok(())
    }

    pub fn issue(&self, plan: &ToolInstallPlan) -> AppResult<ToolInstallApprovalGrant> {
        validate_module_version(&plan.exact_version)?;
        let token = Uuid::new_v4().to_string();
        let mut pending = self.pending.lock().map_err(|_| {
            AppError::Invalid("Tool installation approval cache is unavailable.".into())
        })?;
        pending.retain(|_, approval| approval.expires_at > Instant::now());
        pending.insert(
            token.clone(),
            PendingToolInstall {
                id: plan.id.clone(),
                exact_version: plan.exact_version.clone(),
                expires_at: Instant::now() + INSTALL_APPROVAL_LIFETIME,
            },
        );
        Ok(ToolInstallApprovalGrant { token })
    }

    pub fn consume(&self, token: &str, id: &str, exact_version: &str) -> AppResult<()> {
        validate_module_version(exact_version)?;
        let approval = self
            .pending
            .lock()
            .map_err(|_| AppError::Invalid("Tool installation approval cache is unavailable.".into()))?
            .remove(token)
            .ok_or_else(|| {
                AppError::Invalid(
                    "Tool installation approval is missing, expired, or already used. Review the installation again."
                        .into(),
                )
            })?;
        if approval.expires_at <= Instant::now()
            || approval.id != id
            || approval.exact_version != exact_version
        {
            return Err(AppError::Invalid(
                "Tool installation approval does not match this package and version.".into(),
            ));
        }
        Ok(())
    }
}

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

#[derive(Default)]
struct ToolProbe {
    version: Option<String>,
    executable_path: Option<String>,
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
) -> ToolProbe {
    let Ok(output) = capture(CommandSpec::new(executable, args), Duration::from_secs(12)).await
    else {
        return ToolProbe::default();
    };
    let executable_path = Some(output.executable_path);
    if !output.success {
        return ToolProbe {
            version: None,
            executable_path,
        };
    }
    let value = output.stdout.trim();
    let mut version = None;
    if value.starts_with('{') {
        if let Ok(json) = serde_json::from_str::<Value>(value) {
            for key in ["version", "azure-cli", "Version"] {
                if let Some(value) = json.get(key).and_then(Value::as_str) {
                    version = Some(value.into());
                    break;
                }
            }
        }
    }
    version = version.or_else(|| {
        regex::Regex::new(r"\d+\.\d+(?:\.\d+){0,2}")
            .ok()?
            .find(value)
            .map(|value| value.as_str().to_string())
    });
    ToolProbe {
        version,
        executable_path,
    }
}

async fn powershell_module_version(name: &str) -> ToolProbe {
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
    let Ok(output) = capture(spec, Duration::from_secs(15)).await else {
        return ToolProbe::default();
    };
    ToolProbe {
        version: output
            .success
            .then(|| output.stdout.trim().to_string())
            .filter(|version| !version.is_empty()),
        executable_path: Some(output.executable_path),
    }
}

pub async fn check_all(include_azd_updates: bool) -> Vec<ToolStatus> {
    let (azd, az, pwsh, az_module, graph_module, exchange_module, azd_inventory, azd_updates) = tokio::join!(
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
        powershell_module_version("ExchangeOnlineManagement"),
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
        Some("2.60.0"),
        ToolProbe {
            version: inventory_azure_cli.or(az.version),
            executable_path: az.executable_path,
        },
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
            Some("1.23.0"),
            azd,
            "manual",
            None,
        ),
        azure_cli,
        powershell_tool(pwsh),
        tool(
            "Az.Accounts",
            "Azure PowerShell",
            "powershellModule",
            Some("3.0.0"),
            az_module,
            "psResource",
            Some("Az.Accounts"),
        ),
        tool(
            "Microsoft.Graph",
            "Microsoft Graph PowerShell",
            "powershellModule",
            Some("2.0.0"),
            graph_module,
            "psResource",
            Some("Microsoft.Graph"),
        ),
        tool(
            "exchange-online-management",
            "ExchangeOnlineManagement",
            "powershellModule",
            None,
            exchange_module,
            "manual",
            Some("ExchangeOnlineManagement"),
        ),
    ]
}

fn powershell_tool(probe: ToolProbe) -> ToolStatus {
    tool(
        "pwsh",
        "PowerShell",
        "tool",
        Some("7.6.0"),
        probe,
        "manual",
        None,
    )
}

fn version_requirement(value: &str) -> Result<semver::VersionReq, semver::Error> {
    match semver::Version::parse(value.trim_start_matches('v')) {
        Ok(version) => semver::VersionReq::parse(&format!(">={version}")),
        Err(_) => semver::VersionReq::parse(value),
    }
}

fn minimum_requirement(value: &str) -> String {
    match semver::Version::parse(value.trim_start_matches('v')) {
        Ok(version) => format!(">={version}"),
        Err(_) => value.to_string(),
    }
}

pub fn manifest_prerequisites(
    manifest: &TemplateManifest,
    environment_values: &std::collections::HashMap<String, String>,
) -> AppResult<Vec<Prerequisite>> {
    let mut prerequisites = crate::manifest::effective_prerequisites(manifest, environment_values)?;
    let Some(minimum_azd_version) = manifest.minimum_azd_version.as_deref() else {
        return Ok(prerequisites);
    };
    let azd_requirement = minimum_requirement(minimum_azd_version);
    if let Some(azd) = prerequisites
        .iter_mut()
        .find(|requirement| requirement.id == "azd")
    {
        azd.required = true;
        azd.minimum_version = Some(match azd.minimum_version.take() {
            Some(existing) => format!("{}, {azd_requirement}", minimum_requirement(&existing)),
            None => azd_requirement,
        });
    } else {
        prerequisites.push(Prerequisite {
            id: "azd".into(),
            name: "Azure Developer CLI".into(),
            kind: "tool".into(),
            minimum_version: Some(azd_requirement),
            required: true,
            install_provider: Some("manual".into()),
            install_id: None,
            documentation_url: Some(
                "https://learn.microsoft.com/azure/developer/azure-developer-cli/install-azd"
                    .into(),
            ),
            condition: None,
        });
    }
    Ok(prerequisites)
}

fn evaluate_requirement(requirement: &Prerequisite, detected: &[ToolStatus]) -> ToolStatus {
    let Some(mut status) = detected
        .iter()
        .find(|status| status.id == requirement.id)
        .cloned()
    else {
        return ToolStatus {
            id: requirement.id.clone(),
            name: requirement.name.clone(),
            kind: requirement.kind.clone(),
            minimum_version: requirement.minimum_version.clone(),
            required: requirement.required,
            state: "unknown".into(),
            installed_version: None,
            executable_path: None,
            latest_version: None,
            update_available: false,
            detail: Some(
                "This prerequisite is not registered by the native tool inventory.".into(),
            ),
            install_provider: requirement.install_provider.clone(),
            install_id: requirement.install_id.clone(),
            documentation_url: requirement.documentation_url.clone(),
            install_command: None,
            can_install: false,
        };
    };

    status.name = requirement.name.clone();
    status.kind = requirement.kind.clone();
    status.required = requirement.required;
    status.minimum_version = requirement.minimum_version.clone();
    status.install_provider = requirement
        .install_provider
        .clone()
        .or(status.install_provider);
    status.install_id = requirement.install_id.clone().or(status.install_id);
    status.documentation_url = requirement
        .documentation_url
        .clone()
        .or(status.documentation_url);

    let Some(required_version) = requirement.minimum_version.as_deref() else {
        return status;
    };
    let required = match version_requirement(required_version) {
        Ok(required) => required,
        Err(_) => {
            status.state = "unknown".into();
            status.detail =
                Some("The manifest declares an invalid prerequisite version requirement.".into());
            return status;
        }
    };
    let installed = status
        .installed_version
        .as_deref()
        .and_then(|version| semver::Version::parse(version.trim_start_matches('v')).ok());
    status.state = match installed {
        Some(installed) if required.matches(&installed) => "ready".into(),
        Some(_) => "outdated".into(),
        None => "missing".into(),
    };
    status
}

pub async fn evaluate_prerequisites(requirements: &[Prerequisite]) -> Vec<ToolStatus> {
    let detected = check_all(false).await;
    requirements
        .iter()
        .map(|requirement| evaluate_requirement(requirement, &detected))
        .collect()
}

pub async fn evaluate_manifest_prerequisites(
    manifest: &TemplateManifest,
    environment_values: &std::collections::HashMap<String, String>,
) -> AppResult<Vec<ToolStatus>> {
    Ok(evaluate_prerequisites(&manifest_prerequisites(manifest, environment_values)?).await)
}

pub async fn ensure_requirements(requirements: &[Prerequisite]) -> AppResult<()> {
    let blocking = evaluate_prerequisites(requirements)
        .await
        .into_iter()
        .filter(|status| status.required && status.state != "ready")
        .map(|status| format!("{} ({})", status.name, status.state))
        .collect::<Vec<_>>();
    if blocking.is_empty() {
        Ok(())
    } else {
        Err(AppError::Invalid(format!(
            "Required template prerequisites are not ready: {}.",
            blocking.join(", ")
        )))
    }
}

pub async fn ensure_manifest_requirements(
    manifest: &TemplateManifest,
    environment_values: &std::collections::HashMap<String, String>,
) -> AppResult<()> {
    ensure_requirements(&manifest_prerequisites(manifest, environment_values)?).await
}

fn tool(
    id: &str,
    name: &str,
    kind: &str,
    minimum: Option<&str>,
    probe: ToolProbe,
    provider: &str,
    install_id: Option<&str>,
) -> ToolStatus {
    let reported_version = probe
        .version
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let installed_version = reported_version
        .and_then(|value| semver::Version::parse(value.trim_start_matches('v')).ok());
    let required_version = minimum.and_then(|minimum| semver::Version::parse(minimum).ok());
    let state = match (reported_version, installed_version, required_version) {
        (None, _, _) => "missing",
        (Some(_), None, _) => "unknown",
        (Some(_), Some(installed), Some(required)) if installed < required => "outdated",
        (Some(_), Some(_), _) => "ready",
    };
    let can_install = approved_install(id).is_some();
    let install_command = match provider {
        "azdTool" => install_id.map(|name| format!("azd tool install {name} --no-prompt")),
        _ => None,
    };
    let documentation_url = documentation_url(id).map(Into::into).or_else(|| {
        can_install.then(|| {
            "https://learn.microsoft.com/powershell/module/microsoft.powershell.psresourceget/install-psresource?view=powershellget-3.x".into()
        })
    });
    ToolStatus {
        id: id.into(),
        name: name.into(),
        kind: kind.into(),
        minimum_version: minimum.map(Into::into),
        required: id == "azd",
        state: state.into(),
        installed_version: reported_version.map(str::to_string),
        executable_path: probe.executable_path,
        latest_version: None,
        update_available: false,
        detail: None,
        install_provider: Some(provider.into()),
        install_id: install_id.map(Into::into),
        documentation_url,
        install_command,
        can_install,
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
    fn script(&self, exact_version: &str) -> AppResult<String> {
        validate_module_version(exact_version)?;
        Ok(format!(
            "Install-PSResource {} -Version '{}' -Repository PSGallery -Scope CurrentUser -TrustRepository -Quiet -AcceptLicense",
            self.id, exact_version
        ))
    }

    fn plan(&self, exact_version: &str) -> AppResult<ToolInstallPlan> {
        let script = self.script(exact_version)?;
        Ok(ToolInstallPlan {
            id: self.id.into(),
            tool_name: self.name.into(),
            provider: "PSResourceGet".into(),
            action: format!("Install or update the {} module", self.name),
            source: "PowerShell Gallery (PSGallery)".into(),
            scope: "Current user".into(),
            destination: "Current-user PowerShell module path".into(),
            exact_version: exact_version.into(),
            command: format!(
                "pwsh -NoLogo -NoProfile -NonInteractive -Command \"{script}\""
            ),
            documentation_url: "https://learn.microsoft.com/powershell/module/microsoft.powershell.psresourceget/install-psresource?view=powershellget-3.x".into(),
        })
    }

    fn spec(&self, exact_version: &str) -> AppResult<CommandSpec> {
        Ok(CommandSpec::new(
            Executable::PowerShell,
            [
                "-NoLogo".into(),
                "-NoProfile".into(),
                "-NonInteractive".into(),
                "-Command".into(),
                self.script(exact_version)?,
            ],
        ))
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

fn validate_module_version(version: &str) -> AppResult<()> {
    semver::Version::parse(version)
        .map(|_| ())
        .map_err(|_| AppError::Invalid(format!("Invalid PowerShell module version: {version}")))
}

pub async fn resolve_install_plan(id: &str) -> AppResult<ToolInstallPlan> {
    let install = approved_install(id).ok_or_else(|| {
        AppError::Invalid(format!(
            "{id} has no vetted non-elevated installation strategy."
        ))
    })?;
    let spec = CommandSpec::new(
        Executable::PowerShell,
        [
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "$r=Find-PSResource -Name $env:AZD_GUI_MODULE -Repository PSGallery | Sort-Object Version -Descending | Select-Object -First 1; if($r){$r.Version.ToString()}",
        ],
    )
    .env("AZD_GUI_MODULE", install.id);
    let output = capture(spec, Duration::from_secs(30)).await?;
    if !output.success {
        return Err(AppError::Command(format!(
            "Could not resolve an exact PSGallery version for {}: {}",
            install.id,
            output.stderr.trim()
        )));
    }
    let version = output.stdout.trim();
    install.plan(version)
}

pub fn install_spec(id: &str, exact_version: &str) -> AppResult<CommandSpec> {
    approved_install(id)
        .ok_or_else(|| {
            AppError::Invalid(format!(
                "{id} has no vetted non-elevated installation strategy."
            ))
        })?
        .spec(exact_version)
}

#[cfg(test)]
mod tests {
    use super::{
        approved_install, evaluate_requirement, install_spec, manifest_prerequisites,
        parse_azd_tool_inventory, parse_azd_tool_updates, powershell_tool, tool,
        ToolInstallApprovals, ToolProbe,
    };
    use crate::models::{Prerequisite, TemplateManifest};
    use serde_json::json;
    use std::collections::HashMap;

    #[test]
    fn powershell_readiness_enforces_the_76_boundary() {
        for (version, expected_state) in [
            (Some("7.4.0"), "outdated"),
            (Some("7.5.99"), "outdated"),
            (Some("7.6.0-preview.1"), "outdated"),
            (Some("7.6.0"), "ready"),
            (Some("7.6.6"), "ready"),
            (Some("7.7.0"), "ready"),
            (Some("bad output"), "unknown"),
            (Some(""), "missing"),
            (None, "missing"),
        ] {
            let status = powershell_tool(ToolProbe {
                version: version.map(Into::into),
                executable_path: Some("pwsh.exe".into()),
            });
            assert_eq!(status.state, expected_state, "version: {version:?}");
            assert_eq!(status.minimum_version.as_deref(), Some("7.6.0"));
            assert_eq!(status.install_provider.as_deref(), Some("manual"));
            assert_eq!(status.executable_path.as_deref(), Some("pwsh.exe"));
            assert!(!status.can_install);
        }
    }

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
        let plan = approved_install("Microsoft.Graph")
            .unwrap()
            .plan("2.32.0")
            .unwrap();
        let spec = install_spec("Microsoft.Graph", "2.32.0").unwrap();

        assert_eq!(plan.provider, "PSResourceGet");
        assert_eq!(plan.source, "PowerShell Gallery (PSGallery)");
        assert_eq!(plan.scope, "Current user");
        assert_eq!(plan.exact_version, "2.32.0");
        assert!(plan.command.contains(spec.args.last().unwrap()));
        assert!(plan.command.contains("-Version '2.32.0'"));
        assert!(plan.command.contains("-Quiet -AcceptLicense"));
    }

    #[test]
    fn unapproved_installers_do_not_produce_plans_or_commands() {
        for id in [
            "azd",
            "az-cli",
            "pwsh",
            "exchange-online-management",
            "manifest-supplied",
        ] {
            assert!(approved_install(id).is_none());
            assert!(install_spec(id, "1.0.0").is_err());
        }
    }

    #[test]
    fn tool_install_approval_is_version_bound_and_single_use() {
        let plan = approved_install("Az.Accounts")
            .unwrap()
            .plan("5.3.1")
            .unwrap();
        let approvals = ToolInstallApprovals::default();
        let grant = approvals.issue(&plan).unwrap();

        assert!(approvals
            .consume(&grant.token, "Az.Accounts", "5.3.0")
            .is_err());
        let grant = approvals.issue(&plan).unwrap();
        approvals
            .consume(&grant.token, "Az.Accounts", "5.3.1")
            .unwrap();
        assert!(approvals
            .consume(&grant.token, "Az.Accounts", "5.3.1")
            .is_err());
    }

    #[test]
    fn user_directed_tools_keep_copyable_official_guidance() {
        let status = tool(
            "az-cli",
            "Azure CLI",
            "tool",
            Some("2.60.0"),
            ToolProbe::default(),
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

    #[test]
    fn exchange_module_inventory_is_manual_and_has_no_invented_minimum() {
        let status = tool(
            "exchange-online-management",
            "ExchangeOnlineManagement",
            "powershellModule",
            None,
            ToolProbe::default(),
            "manual",
            Some("ExchangeOnlineManagement"),
        );

        assert_eq!(status.minimum_version, None);
        assert!(!status.can_install);
        assert_eq!(status.install_provider.as_deref(), Some("manual"));
    }

    #[test]
    fn empty_or_unparseable_module_output_never_reports_ready() {
        for (version, expected_state) in [
            ("", "missing"),
            ("  ", "missing"),
            ("bad output", "unknown"),
        ] {
            let status = tool(
                "Az.Accounts",
                "Azure PowerShell",
                "powershellModule",
                Some("3.0.0"),
                ToolProbe {
                    version: Some(version.into()),
                    executable_path: Some("pwsh.exe".into()),
                },
                "psResource",
                Some("Az.Accounts"),
            );
            assert_eq!(status.state, expected_state, "version: {version:?}");
            assert_ne!(status.state, "ready");
        }
    }

    #[test]
    fn template_requirement_uses_its_own_semver_range() {
        let detected = tool(
            "azd",
            "Azure Developer CLI",
            "tool",
            Some("1.23.0"),
            ToolProbe {
                version: Some("1.23.1".into()),
                executable_path: None,
            },
            "manual",
            None,
        );
        let requirement = Prerequisite {
            id: "azd".into(),
            name: "Azure Developer CLI".into(),
            kind: "tool".into(),
            minimum_version: Some(">=1.23.7, <2.0.0".into()),
            required: true,
            install_provider: Some("manual".into()),
            install_id: None,
            documentation_url: None,
            condition: None,
        };

        let status = evaluate_requirement(&requirement, &[detected]);
        assert_eq!(status.state, "outdated");
        assert_eq!(status.minimum_version.as_deref(), Some(">=1.23.7, <2.0.0"));
    }

    #[test]
    fn unknown_template_prerequisites_fail_closed() {
        let requirement = Prerequisite {
            id: "unregistered-tool".into(),
            name: "Unregistered tool".into(),
            kind: "tool".into(),
            minimum_version: Some(">=1.0.0".into()),
            required: true,
            install_provider: None,
            install_id: None,
            documentation_url: None,
            condition: None,
        };

        let status = evaluate_requirement(&requirement, &[]);
        assert_eq!(status.state, "unknown");
        assert!(!status.can_install);
    }

    #[test]
    fn minimum_azd_version_strengthens_the_manifest_azd_prerequisite() {
        let manifest = TemplateManifest {
            schema_version: "1.0".into(),
            minimum_azd_version: Some("1.23.7".into()),
            prerequisites: vec![Prerequisite {
                id: "azd".into(),
                name: "Azure Developer CLI".into(),
                kind: "tool".into(),
                minimum_version: Some("1.23.0".into()),
                required: false,
                install_provider: Some("manual".into()),
                install_id: None,
                documentation_url: None,
                condition: None,
            }],
            ..Default::default()
        };

        let prerequisites = manifest_prerequisites(&manifest, &HashMap::new()).unwrap();
        assert_eq!(prerequisites.len(), 1);
        assert!(prerequisites[0].required);
        assert_eq!(
            prerequisites[0].minimum_version.as_deref(),
            Some(">=1.23.0, >=1.23.7")
        );
    }

    #[test]
    fn minimum_azd_version_adds_a_required_azd_prerequisite_when_absent() {
        let manifest = TemplateManifest {
            schema_version: "1.0".into(),
            minimum_azd_version: Some("1.23.7".into()),
            ..Default::default()
        };

        let prerequisites = manifest_prerequisites(&manifest, &HashMap::new()).unwrap();
        assert_eq!(prerequisites.len(), 1);
        assert_eq!(prerequisites[0].id, "azd");
        assert!(prerequisites[0].required);
        assert_eq!(
            prerequisites[0].minimum_version.as_deref(),
            Some(">=1.23.7")
        );
    }

    #[test]
    fn minimum_azd_version_remains_unconditional_when_manifest_azd_is_disabled() {
        let manifest: TemplateManifest = serde_json::from_value(json!({
            "schemaVersion": "1.0",
            "minimumAzdVersion": "1.23.7",
            "configuration": {"groups": [{
                "id": "options",
                "title": "Options",
                "fields": [{
                    "id": "enableAzdCheck",
                    "environmentVariable": "ENABLE_AZD_CHECK",
                    "label": "Enable azd check",
                    "type": "boolean",
                    "default": false
                }]
            }]},
            "prerequisites": [{
                "id": "azd",
                "name": "Azure Developer CLI",
                "kind": "tool",
                "required": true,
                "condition": {"field": "enableAzdCheck", "equals": true}
            }]
        }))
        .unwrap();

        let prerequisites = manifest_prerequisites(&manifest, &HashMap::new()).unwrap();
        assert_eq!(prerequisites.len(), 1);
        assert_eq!(prerequisites[0].id, "azd");
        assert_eq!(
            prerequisites[0].minimum_version.as_deref(),
            Some(">=1.23.7")
        );
        assert!(prerequisites[0].condition.is_none());
    }
}
