use std::path::Path;
fn main() {
    let a: Vec<_> = std::env::args().collect();
    if let [_, command, root, dll] = a.as_slice()
        && command == "synthesize-package-manifest"
    {
        match superexplorer_plugin_tooling::synthesize_package_manifest(
            Path::new(root),
            Path::new(dll),
        ) {
            Ok(manifest) => println!("{manifest}"),
            Err(error) => {
                eprintln!("package manifest synthesis failed: {error}");
                std::process::exit(1);
            }
        }
        return;
    }
    if let [_, command, root, dll, output] = a.as_slice()
        && command == "stage-package"
    {
        match superexplorer_plugin_tooling::stage_package(
            Path::new(root),
            Path::new(dll),
            Path::new(output),
        ) {
            Ok(()) => return,
            Err(error) => {
                eprintln!("package staging failed: {error}");
                std::process::exit(1);
            }
        }
    }
    if let [_, command, root, bundle_id, abi_schema] = a.as_slice()
        && command == "materialize-folder-size-template"
    {
        let abi_schema = abi_schema.parse::<u32>().unwrap_or(0);
        match superexplorer_plugin_tooling::materialize_folder_size_template(
            Path::new(root),
            bundle_id,
            abi_schema,
        ) {
            Ok(report) => println!("{}", serde_json::to_string(&report).unwrap()),
            Err(error) => {
                eprintln!("folder-size template materialization failed: {error}");
                std::process::exit(1);
            }
        }
        return;
    }
    let r = match a.as_slice() {
        [_, command, path] if command == "validate" => {
            superexplorer_plugin_tooling::validate(Path::new(path))
        }
        [_, command, path] if command == "inspect-dll" => {
            superexplorer_plugin_tooling::inspect_dll(Path::new(path))
        }
        _ => std::process::exit(2),
    };
    println!("{}", serde_json::to_string(&r).unwrap());
    if !r.valid {
        std::process::exit(1)
    }
}
