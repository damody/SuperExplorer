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
