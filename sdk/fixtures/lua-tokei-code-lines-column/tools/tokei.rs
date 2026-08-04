use std::{env, path::PathBuf};
use tokei::{Config, Languages};

fn main() {
    let paths = env::args_os()
        .skip(1)
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    if paths.is_empty() {
        eprintln!("at least one path is required");
        std::process::exit(2);
    }
    let rows = paths.iter().map(|path| {
        let mut languages = Languages::new();
        languages.get_statistics(&[path], &[], &Config::default());
        let (mut code, mut comments, mut blanks) = (0, 0, 0);
        for (_, stats) in languages { code += stats.code; comments += stats.comments; blanks += stats.blanks; }
        serde_json::json!({"path": path.to_string_lossy(), "code": code, "comments": comments, "blanks": blanks, "total": code + comments + blanks})
    }).collect::<Vec<_>>();
    match serde_json::to_string(&rows) {
        Ok(json) => println!("{json}"),
        Err(_) => std::process::exit(3),
    }
}
