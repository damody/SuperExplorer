use std::path::Path;
fn main() {
    let a: Vec<_> = std::env::args().collect();
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
