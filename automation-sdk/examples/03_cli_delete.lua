script.configure { name = "CLI and confirmed cleanup", activation = "temporary" }

on("hotkey.triggered", function(event, task)
  local result = await(process.run("tools\\formatter.exe", { "--input", "note.txt" }, {
    cwd = task.cwd,
    timeout = "30s"
  }))
  await(process.run_script("tools\\postprocess.ps1", { "-Input", "note.txt" }, {
    cwd = task.cwd,
    timeout = "2m"
  }))
  -- This is the only host action that always opens a confirmation prompt.
  await(fs.remove("old-summary.txt", { base = task.cwd, recycle = true }))
end)
