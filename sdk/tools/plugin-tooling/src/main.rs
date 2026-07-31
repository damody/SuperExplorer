use std::path::Path;
fn main() {
    let a: Vec<_> = std::env::args().collect();
    if a.len() != 3 || a[1] != "validate" {
        std::process::exit(2)
    }
    let r = superexplorer_plugin_tooling::validate(Path::new(&a[2]));
    println!("{}", serde_json::to_string(&r).unwrap());
    if !r.valid {
        std::process::exit(1)
    }
}
