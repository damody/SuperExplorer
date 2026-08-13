#![cfg(windows)]

#[path = "../mft_journal.rs"]
mod mft_journal;
#[path = "../mft_size_map.rs"]
mod mft_size_map;

use std::path::Path;

fn main() {
    let mut arguments = std::env::args_os().skip(1);
    let Some(root) = arguments.next() else {
        std::process::exit(2);
    };
    let Some(output) = arguments.next() else {
        std::process::exit(2);
    };
    let index = match mft_size_map::read_volume_index(Path::new(&root), || false) {
        Ok(index) => index,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(3);
        }
    };
    if let Err(error) = mft_size_map::write_index(Path::new(&output), &index) {
        eprintln!("{error}");
        std::process::exit(4);
    }
}
