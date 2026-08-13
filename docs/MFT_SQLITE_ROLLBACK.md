# MFT SQLite upgrade and rollback

The installer stops `SuperExplorerMft`, waits for `STOPPED`, replaces the service binary, and restarts it as `LocalSystem`. It never deletes `%ProgramData%\SuperExplorer\MftIndex` during install, upgrade, repair, or uninstall.

The current service owns `<volume>.mft.sqlite3`, `-wal`, and `-shm`. It admits legacy `.semftidx`, `.semftcp`, and `.semftdelta` files read-only only when the complete canonical SQLite set is absent. Migration and cleanup require both the ten-minute deadline and an authenticated focused Super Explorer lease.

Rolling back to an older binary does not require deleting SQLite during service stop. Older builds ignore the SQLite filenames and may rebuild their own legacy cache from NTFS. SQLite and quarantine/audit records remain available for recovery or a later reinstall. An administrator who intentionally wants permanent cache removal must first stop the service and separately remove the exact `%ProgramData%\SuperExplorer\MftIndex`, `MftIndexQuarantine`, and `MftMaintenanceAudit` directories; that action is not part of normal rollback or uninstall.

Because uncommitted memory changes are deliberately discarded on stop, restart catches up from the last durable USN cursor. If the journal no longer retains that range, the current service reports rebuild-required and waits for the normal foreground gate.
