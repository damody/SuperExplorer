# Trilingual README Design

## Goal

Add complete, synchronized project documentation in English, Traditional Chinese, and Simplified Chinese so developers can understand, build, run, and validate the project in their preferred language.

## Files

- `README.md`: canonical English README and the default repository landing page.
- `README.zh-TW.md`: complete Traditional Chinese translation.
- `README.zh-CN.md`: complete Simplified Chinese translation.

Each file will begin with links to all three language versions. The three files will use the same section order, commands, paths, and factual content.

## Content Structure

1. Project name and concise description.
2. Language selector.
3. Current feature highlights, limited to behavior supported by the codebase and existing evidence documents.
4. Platform and toolchain requirements, including Windows, Rust, MSVC, and Git submodules.
5. Clone and submodule initialization instructions.
6. Development build and run commands.
7. Release artifact command.
8. Formatting, checking, linting, and test commands.
9. High-level workspace structure.
10. Links to detailed project status, manual testing, visual testing, and evidence documents.
11. Known limitations, reflecting the current handoff/status documentation.
12. Proprietary, source-available licensing statement based on the workspace's `LicenseRef-SuperExplorer-Proprietary` metadata, with third-party terms kept separate.

## Source of Truth

README claims will be derived from `Cargo.toml`, crate manifests, scripts, and the maintained documents under `docs/`. Commands will use PowerShell syntax because this is a Windows-only project. Generated artifacts and local evidence directories will not be presented as version-controlled deliverables.

## Translation Rules

- Preserve command blocks, environment variable names, file paths, and Rust identifiers exactly across languages.
- Translate explanatory prose and headings naturally rather than word-for-word.
- Use Taiwan terminology in `README.zh-TW.md` and Mainland China terminology in `README.zh-CN.md` where terminology differs.
- Keep the English README canonical; future factual changes should be mirrored into both translations.

## Validation

- Confirm all three files exist and their language links resolve to repository-relative paths.
- Compare heading structure and fenced command blocks across all versions.
- Check every referenced repository path exists.
- Scan for incomplete placeholder markers.
- Run a lightweight Markdown consistency check using repository tools or a local script where available.

## Scope

This change adds documentation only. It does not change application code, dependencies, build configuration, generated artifacts, or existing user modifications.
