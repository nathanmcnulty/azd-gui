fn main() {
    let manifest = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    for file in [
        "AzdGui.GraphAdapter.csproj",
        "OwnedGraphSession.cs",
        "packages.lock.json",
        "NuGet.Config",
        "global.json",
    ] {
        println!("cargo:rerun-if-changed=graph-adapter/{file}");
    }
    let assets = output.join("graph-adapter");
    let result = std::process::Command::new("dotnet")
        .current_dir(manifest.join("graph-adapter"))
        .args([
            "publish",
            "AzdGui.GraphAdapter.csproj",
            "--configuration",
            "Release",
            "--nologo",
            "--output",
        ])
        .arg(&assets)
        .arg("-p:RestoreLockedMode=true")
        .arg(format!(
            "-p:BaseIntermediateOutputPath={}/",
            output.join("graph-adapter-obj").display()
        ))
        .env("DOTNET_CLI_TELEMETRY_OPTOUT", "1")
        .env("DOTNET_NOLOGO", "1")
        .output()
        .expect(".NET 8 SDK is required to build the owned Graph adapter");
    if !result.status.success() {
        panic!(
            "Owned Graph adapter build failed: {}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
    }
    let mut files = std::fs::read_dir(&assets)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "dll"))
        .collect::<Vec<_>>();
    files.sort();
    assert!(files
        .iter()
        .any(|path| path.file_name().unwrap() == "AzdGui.GraphAdapter.dll"));
    let mut generated = "const GRAPH_ADAPTER_ASSETS: &[(&str, &[u8])] = &[\n".to_string();
    for path in files {
        generated.push_str(&format!(
            "({:?}, include_bytes!({:?})),\n",
            path.file_name().unwrap().to_string_lossy(),
            path.to_string_lossy()
        ));
    }
    generated.push_str("];\n");
    std::fs::write(output.join("graph_adapter_assets.rs"), generated).unwrap();
    tauri_build::build()
}
