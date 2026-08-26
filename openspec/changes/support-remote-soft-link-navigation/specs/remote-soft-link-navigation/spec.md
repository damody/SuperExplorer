## ADDED Requirements

### Requirement: Remote symbolic-link classification
The remote runtime SHALL classify each listed ADB and SFTP entry as an ordinary file, ordinary
directory, link to file, link to directory, broken link, or circular link before emitting the entry
to Explorer UI consumers.

#### Scenario: Link targets a directory
- **WHEN** an ADB or SFTP directory contains a symbolic link whose target resolves to a directory
- **THEN** the emitted entry SHALL be classified as a directory link and as a navigable container

#### Scenario: Link targets a file
- **WHEN** an ADB or SFTP directory contains a symbolic link whose target resolves to a file
- **THEN** the emitted entry SHALL be classified as a file link and SHALL NOT be a navigable container

#### Scenario: Link target is unavailable
- **WHEN** a symbolic-link target is missing or cannot be resolved as an entry
- **THEN** the emitted entry SHALL be classified as a broken link and SHALL NOT be a navigable container

#### Scenario: Link resolution cycles
- **WHEN** resolution revisits a link path or exhausts the bounded link-hop limit
- **THEN** the emitted entry SHALL be classified as a circular link and SHALL NOT be a navigable container

### Requirement: Consistent remote link navigation
The file view, navigation pane, and breadcrumb child menus SHALL use the same provider classification
when deciding whether a remote item can be entered, and navigation through a directory link SHALL
retain the selected link path.

#### Scenario: Enter directory link from the file view
- **WHEN** the user opens a remote directory-link row with double-click or Enter
- **THEN** SuperExplorer SHALL navigate to the link-side virtual location and list the target directory

#### Scenario: Expand directory link from the navigation pane
- **WHEN** the user expands a remote directory link in the navigation pane
- **THEN** SuperExplorer SHALL expose the target directory's child containers under the link row

#### Scenario: Invalid link cannot be entered
- **WHEN** the user activates a broken or circular remote link
- **THEN** SuperExplorer SHALL keep the item selected and SHALL NOT begin directory navigation

#### Scenario: Link path remains visible
- **WHEN** navigation through a remote directory link succeeds
- **THEN** history, breadcrumbs, and the address bar SHALL retain the link-side path selected by the user

### Requirement: Distinct remote link presentation
The Type column SHALL identify links to folders, links to files, broken links, and circular links with
distinct stable labels.

#### Scenario: Link kinds are displayed
- **WHEN** a directory result contains all supported symbolic-link states
- **THEN** their Type values SHALL respectively be `Remote folder link`, `Remote file link`,
  `Broken remote link`, and `Circular remote link`

### Requirement: Bounded and cancellable link resolution
Remote link classification SHALL enforce a finite traversal limit, detect repeated paths, and honour
the directory request's cancellation without recursively enumerating target directory contents.

#### Scenario: Directory request is cancelled during link resolution
- **WHEN** cancellation is signalled while ADB or SFTP is resolving link metadata
- **THEN** provider work SHALL stop and no late directory batch SHALL update the active tab

#### Scenario: Directory transport fails
- **WHEN** the ADB command or SFTP session fails before an individual link can be definitively classified
- **THEN** the directory request SHALL fail rather than presenting all affected entries as broken links
