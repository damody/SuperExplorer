script.configure { name = "Clipboard timer", activation = "temporary" }

schedule.every("10m", function(event, task)
  await(sleep("250ms"))
  local text = await(clipboard.read_text())
  if text and #text > 0 then
    await(fs.write_text("clipboard-note.txt", text, { base = task.cwd, mode = "atomic_replace" }))
  end
end)

schedule.cron("0 0 9 * * MON-FRI", "Asia/Taipei", function(event, task)
  await(ui.notify("Automation", "Scheduled task is active"))
end)
