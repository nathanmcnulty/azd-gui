//! One supported validation route. No project scripts or hooks are executed.
use crate::{
    command_runner::{CommandSpec, Executable},
    error::{AppError, AppResult},
    models::ProjectTemplateSnapshot,
};
use std::{path::Path, sync::Arc};
pub const REVISION: &str = "abc6e896620990e68360a63dd14efda66a3f75cd";
const FILES: &[(&str, &str)] = &[
    ("scripts/Test-Deployment.ps1", include_str!("../deployment-check/scripts/Test-Deployment.ps1")),
    ("scripts/Deployment.Validation.psm1", include_str!("../deployment-check/scripts/Deployment.Validation.psm1")),
    ("scripts/vendor/Azd.DeploymentValidation/Azd.DeploymentValidation.psd1", include_str!("../deployment-check/scripts/vendor/Azd.DeploymentValidation/Azd.DeploymentValidation.psd1")),
    ("scripts/vendor/Azd.DeploymentValidation/Azd.DeploymentValidation.psm1", include_str!("../deployment-check/scripts/vendor/Azd.DeploymentValidation/Azd.DeploymentValidation.psm1")),
    ("scripts/vendor/Azd.DeploymentValidation/deployment-validation.schema.json", include_str!("../deployment-check/scripts/vendor/Azd.DeploymentValidation/deployment-validation.schema.json")),
];
pub fn supported(snapshot: &ProjectTemplateSnapshot) -> bool {
    snapshot.entry.repository == "https://github.com/nathanmcnulty/azd-reference"
        && snapshot.entry.commit == REVISION
        && snapshot.entry.subdirectory.as_deref() == Some("examples/deployment-check")
}
pub fn command(
    snapshot: &ProjectTemplateSnapshot,
    project: &Path,
    environment: &str,
) -> AppResult<CommandSpec> {
    if !cfg!(windows) {
        return Err(AppError::Invalid(
            "Use the supported Windows app for validation.".into(),
        ));
    }
    if !supported(snapshot) {
        return Err(AppError::Invalid(
            "This source has no supported validation action.".into(),
        ));
    }
    crate::deployment::validate_environment_name(environment)?;
    if matches!(environment, "." | "..") {
        return Err(AppError::Invalid("Choose a named environment.".into()));
    }
    let target = crate::evidence::read_target_values(project, environment);
    let tenant = target
        .tenant_id
        .ok_or_else(|| AppError::Invalid("Save a valid tenant before validation.".into()))?;
    let subscription = target
        .subscription_id
        .ok_or_else(|| AppError::Invalid("Save a valid subscription before validation.".into()))?;
    let group = target.resource_group.ok_or_else(|| {
        AppError::Invalid("Save the deployed resource group before validation.".into())
    })?;
    crate::deployment::validate_resource_group(Some(&group))?;
    if group.is_empty() || target.azure_cloud.as_deref() != Some("AzureCloud") {
        return Err(AppError::Invalid(
            "Review the public Azure target before validation.".into(),
        ));
    }
    crate::pim_pilot::reject_aliases(project)?;
    if !project.join("azure.yaml").is_file() {
        return Err(AppError::Invalid("Project azure.yaml is missing.".into()));
    }
    let project_guard = Arc::new(crate::pim_pilot::Root::open(project)?);
    let reports = project.join("reports");
    match std::fs::create_dir(&reports) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.into()),
    }
    let reports_guard = Arc::new(crate::pim_pilot::Root::for_write(&reports)?);
    drop(project_guard);
    crate::pim_pilot::reject_aliases(&reports.join("deployment-validation.json"))?;
    let assets = Arc::new(tempfile::tempdir()?);
    for (name, content) in FILES {
        let path = assets.path().join(name);
        std::fs::create_dir_all(path.parent().expect("embedded path"))?;
        // Adapt only the root to the registered project; scripts and imports stay embedded.
        let mut content = content.replace(
            "[System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))",
            "$env:AZD_GUI_VALIDATION_PROJECT_ROOT",
        );
        if *name == "scripts/Test-Deployment.ps1" {
            content = content.replace(
                "$writtenPath =",
                &format!(
                    "{}\n$writtenPath =",
                    include_str!("project_verification_evidence.ps1")
                ),
            );
        }
        if *name == "scripts/Deployment.Validation.psm1" {
            content = content.replace("$context = $contextJson | ConvertFrom-Json", "$context = $contextJson | ConvertFrom-Json\n            if ([string] $context.environmentName -cne 'AzureCloud') { throw 'Azure CLI must target the public Azure cloud.' }");
        }
        if name.ends_with("deployment-validation.schema.json") {
            content = content.replace("\"const\": \"1.0\"", "\"const\": \"1.1\"");
        }
        std::fs::write(path, content)?;
    }
    let wrapper = assets.path().join("run.ps1");
    std::fs::write(&wrapper, include_str!("project_verification.ps1"))?;
    let mut spec = CommandSpec::new(
        Executable::PowerShell,
        [
            "-NoLogo".into(),
            "-NoProfile".into(),
            "-NonInteractive".into(),
            "-File".into(),
            wrapper.display().to_string(),
        ],
    )
    .cwd(project)
    .env(
        "AZD_GUI_VALIDATION_PROJECT_ROOT",
        project.display().to_string(),
    )
    .env("AZURE_ENV_NAME", environment)
    .env("AZURE_TENANT_ID", tenant)
    .env("AZURE_SUBSCRIPTION_ID", subscription)
    .env("AZURE_RESOURCE_GROUP", group);
    spec.assets = Some(assets);
    spec.directory_guards = vec![reports_guard];
    Ok(spec)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    fn fixture() -> (tempfile::TempDir, ProjectTemplateSnapshot) {
        let root = tempfile::tempdir().unwrap();
        let snapshot = ProjectTemplateSnapshot {
            entry: crate::catalog::entry_by_id("deployment-check").unwrap(),
            digest: "a".repeat(64),
        };
        std::fs::write(root.path().join("azure.yaml"), "name: fixture").unwrap();
        let environment = root.path().join(".azure/dev");
        std::fs::create_dir_all(&environment).unwrap();
        std::fs::write(environment.join(".env"), "AZD_GUI_AZURE_CLOUD=AzureCloud\nAZURE_TENANT_ID=11111111-1111-4111-8111-111111111111\nAZURE_SUBSCRIPTION_ID=22222222-2222-4222-8222-222222222222\nAZURE_RESOURCE_GROUP=rg-fixture\n").unwrap();
        (root, snapshot)
    }
    #[test]
    #[cfg(windows)]
    fn validation_uses_only_embedded_pinned_scripts_and_saved_target() {
        let (root, snapshot) = fixture();
        let spec = command(&snapshot, root.path(), "dev").unwrap();
        assert!(!spec.args.iter().any(|v| v.contains("Test-Deployment.ps1")));
        assert!(spec
            .environment
            .iter()
            .any(|(key, value)| key == "AZURE_RESOURCE_GROUP" && value == "rg-fixture"));
        let assets = spec.assets.unwrap();
        let script =
            std::fs::read_to_string(assets.path().join("scripts/Test-Deployment.ps1")).unwrap();
        assert!(script.contains("$env:AZD_GUI_VALIDATION_PROJECT_ROOT"));
        assert!(script.contains("azdManagementEvidence"));
        assert!(!script.contains("& az login"));
        for change in ["revision", "repo", "path"] {
            let mut altered = snapshot.clone();
            match change {
                "revision" => altered.entry.commit = "f".repeat(40),
                "repo" => altered.entry.repository.push_str("-other"),
                _ => altered.entry.subdirectory = None,
            }
            assert!(command(&altered, root.path(), "dev").is_err());
        }
        assert!(command(&snapshot, root.path(), "../dev").is_err());
        std::fs::write(
            root.path().join(".azure/dev/.env"),
            "AZURE_RESOURCE_GROUP=rg-other",
        )
        .unwrap();
        assert!(command(&snapshot, root.path(), "dev").is_err());
    }
    #[test]
    fn validation_vendor_bytes_remain_pinned() {
        use sha2::{Digest, Sha256};
        let source: serde_json::Value =
            serde_json::from_str(include_str!("../deployment-check/source.json")).unwrap();
        assert_eq!(source["commit"], REVISION);
        for (name, content) in FILES {
            assert_eq!(
                source["files"][name],
                format!("{:x}", Sha256::digest(content.as_bytes())),
                "{name}"
            );
        }
    }
    #[test]
    fn pim_catalog_never_authorizes_generic_graph_hooks() {
        let snapshot = ProjectTemplateSnapshot {
            entry: crate::catalog::entry_by_id("pim-pilot").unwrap(),
            digest: "b".repeat(64),
        };
        assert!(crate::graph_session::reject_unadapted_execution(&snapshot).is_err());
        assert!(!supported(&snapshot));
    }
    #[tokio::test]
    #[cfg(windows)]
    #[ignore = "Runs PowerShell 7 and deterministic Azure CLI fixture; use tools/Test-AdminJourneys.ps1"]
    async fn validation_worker_runs_real_pinned_checks_without_project_code_or_cloud() {
        let (root, snapshot) = fixture();
        let cli = root.path().join("fixture-bin");
        std::fs::create_dir(&cli).unwrap();
        #[cfg(windows)] std::fs::write(cli.join("az.cmd"), "@echo off\r\nif \"%1\"==\"account\" (echo {\"id\":\"22222222-2222-4222-8222-222222222222\",\"tenantId\":\"11111111-1111-4111-8111-111111111111\",\"environmentName\":\"AzureCloud\"})\r\nexit /b 0\r\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let path = cli.join("az");
            std::fs::write(&path, "#!/bin/sh\nif [ \"$1\" = account ]; then echo '{\"id\":\"22222222-2222-4222-8222-222222222222\",\"tenantId\":\"11111111-1111-4111-8111-111111111111\",\"environmentName\":\"AzureCloud\"}'; fi\n").unwrap();
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        std::fs::create_dir(root.path().join("scripts")).unwrap();
        std::fs::write(
            root.path().join("scripts/Test-Deployment.ps1"),
            "throw 'Project code must never execute'",
        )
        .unwrap();
        let inherited = std::env::var_os("PATH").unwrap();
        let path = std::env::join_paths(
            std::iter::once(cli.clone()).chain(std::env::split_paths(&inherited)),
        )
        .unwrap();
        for failure in [false, true] {
            if failure {
                #[cfg(windows)]
                std::fs::write(cli.join("az.cmd"), "@echo off\r\nexit /b 1\r\n").unwrap();
                #[cfg(unix)]
                std::fs::write(cli.join("az"), "#!/bin/sh\nexit 1\n").unwrap();
            }
            let spec = command(&snapshot, root.path(), "dev")
                .unwrap()
                .env("PATH", path.to_string_lossy())
                .env("AZD_GUI_PROJECT_ID", "fixture")
                .env("AZD_GUI_ENVIRONMENT", "dev")
                .env("AZD_GUI_TEMPLATE_ID", "deployment-check")
                .env("AZD_GUI_SOURCE_REVISION", REVISION)
                .env("AZD_GUI_CONTRACT_DIGEST", "a".repeat(64))
                .env("AZD_GUI_AZURE_CLOUD", "AzureCloud")
                .env(
                    "AZD_GUI_OPERATION_ID",
                    "33333333-3333-4333-8333-333333333333",
                );
            let output = crate::command_runner::capture(spec, std::time::Duration::from_secs(30))
                .await
                .unwrap();
            assert_eq!(
                output.success, !failure,
                "{} {}",
                output.stdout, output.stderr
            );
            let report =
                crate::validation_report::read_with_context(root.path(), Some("dev"), None);
            assert!(
                matches!(
                    report.state,
                    crate::validation_report::ValidationReportState::Available
                ),
                "{report:?}"
            );
            assert_eq!(report.report.unwrap().checks.len(), 3);
        }
    }
}
