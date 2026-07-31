//! Discovery of optional Windows Shell companion applications.

use std::{collections::HashSet, path::PathBuf};

const TORTOISE_GIT_RELATIVE_PATH: &str = r"TortoiseGit\bin\TortoiseGitProc.exe";

/// Returns whether a usable `TortoiseGit` installation exists in a standard Program Files root.
///
/// Discovery is side-effect free: it neither launches `TortoiseGit` nor scans outside the bounded
/// candidate roots supplied by Windows environment variables.
pub fn tortoise_git_is_installed() -> bool {
    tortoise_git_executable_from_roots(
        ["ProgramW6432", "ProgramFiles", "ProgramFiles(x86)"]
            .into_iter()
            .filter_map(std::env::var_os)
            .map(PathBuf::from),
    )
    .is_some()
}

fn tortoise_git_executable_from_roots(roots: impl IntoIterator<Item = PathBuf>) -> Option<PathBuf> {
    let mut visited = HashSet::new();
    roots.into_iter().find_map(|root| {
        let key = root.as_os_str().to_string_lossy().to_lowercase();
        if !visited.insert(key) {
            return None;
        }
        let candidate = root.join(TORTOISE_GIT_RELATIVE_PATH);
        candidate.is_file().then_some(candidate)
    })
}

#[cfg(test)]
mod tests {
    use super::tortoise_git_executable_from_roots;

    #[test]
    fn bounded_candidate_discovery_requires_the_real_executable_path() {
        let fixture = tempfile::tempdir().expect("fixture");
        let missing = fixture.path().join("missing");
        let installed = fixture.path().join("installed");
        let executable = installed.join(r"TortoiseGit\bin\TortoiseGitProc.exe");
        std::fs::create_dir_all(executable.parent().expect("binary parent"))
            .expect("TortoiseGit directories");
        std::fs::write(&executable, b"fixture").expect("TortoiseGit executable fixture");

        assert_eq!(
            tortoise_git_executable_from_roots([missing, installed.clone(), installed,]),
            Some(executable),
            "duplicate Program Files roots must not change discovery"
        );
        assert!(tortoise_git_executable_from_roots(Vec::new()).is_none());
    }
}
