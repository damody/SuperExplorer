# Startup and migration evidence

`WindowsBookmarkStore::load_or_migrate` distinguishes document presence from collection length. Valid current/backup data wins, including an empty collection. Only a never-initialized store writes the legacy collection. Unavailable storage returns the unchanged legacy collection and a storage-category warning; application composition emits that warning without bookmark payload data.

The focused adapter command passed all precedence, first-launch, repeat-launch, empty-authority, corrupt-artifact, and failed-migration cases.
