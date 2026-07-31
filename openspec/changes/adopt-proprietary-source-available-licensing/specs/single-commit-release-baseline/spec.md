## ADDED Requirements

### Requirement: Verified single root commit
The final `master` branch SHALL contain exactly one commit, that commit SHALL have no parent, its tree SHALL contain the complete validated proprietary licensing migration, and its message SHALL be `Initial proprietary source-available release`.

#### Scenario: Local history verification
- **WHEN** the history rewrite completes locally
- **THEN** `git rev-list --count master` reports one, the commit has no parent, and the working tree is clean

#### Scenario: Final tree contents
- **WHEN** the root commit tree is inspected
- **THEN** it contains all validated project files and legal documents, omits the two legacy project license files, and retains required third-party notices

### Requirement: Protected remote rewrite
The remote `master` rewrite SHALL use an explicit force-with-lease expectation equal to the remote object ID recorded before implementation. The process MUST stop without overwriting the remote if that expectation no longer matches.

#### Scenario: Unchanged remote
- **WHEN** the recorded remote object ID still matches `origin/master`
- **THEN** the protected force push updates the remote to the new root commit

#### Scenario: Concurrent remote update
- **WHEN** `origin/master` changes after its object ID was recorded
- **THEN** the protected force push fails and the process inspects the remote change instead of forcing over it

### Requirement: Remote baseline verification
After a successful push, the repository SHALL fetch the remote and SHALL verify that local `master` and `origin/master` resolve to the same single root commit.

#### Scenario: Successful synchronization
- **WHEN** the protected force push succeeds and the remote is fetched
- **THEN** local and remote object IDs match and the reachable `master` history contains one commit

### Requirement: No repository-local legacy-history reference
The rewrite SHALL NOT create a backup tag, backup branch, or bundle inside the repository because the user already maintains an external backup.

#### Scenario: Reachability after rewrite
- **WHEN** local refs and tags are enumerated after synchronization
- **THEN** no newly created backup ref retains the superseded history
