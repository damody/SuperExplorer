# Using folder-local Lua automation

1. Create `super_explorer.lua` directly inside the folder to automate.
2. Enter that folder in an Explorer tab. No separate global scripts directory is required.
3. Keep the tab in that exact folder while the automation should remain active.
4. Edit and save the file to reload it. A syntax error leaves the previous valid version active.
5. Leave the folder or close the last tab that owns it to unload the script.

The script is not inherited by child folders. To automate a child folder, place a separate
`super_explorer.lua` in that child folder.

Start with one of the files under `examples/`, copy it to the target directory, and rename it to
`super_explorer.lua`. Relative file output uses the event task's immutable `task.cwd`.

DeepSeek credentials must not appear in Lua. Configure the provider credential through the host
application's credential integration, then use `ai.summarize` as shown in
`examples/04_deepseek_txt.lua`.
