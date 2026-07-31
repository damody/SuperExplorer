script.configure {
  name = "Queued current-folder hotkey",
  activation = "always",
  default_dispatch = "queue",
  task_timeout = "90s"
}

hotkey("Ctrl+Alt+S", function(event, task)
  await(fs.write_text("hotkey.txt", "Triggered in " .. task.cwd, {
    base = task.cwd,
    mode = "atomic_replace"
  }))
end)

on("directory.entered", { dispatch = "queue", queue_capacity = 256 }, function(event, task)
  await(ui.notify("Directory", task.cwd))
end)
