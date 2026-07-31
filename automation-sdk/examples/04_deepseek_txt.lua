script.configure { name = "DeepSeek TXT summary", activation = "always" }

on("fs.created", { debounce = "750ms", dispatch = "queue" }, function(event, task)
  local text = await(fs.read_text(event.data.path))
  local result = await(ai.summarize {
    text = text,
    model = "deepseek-v4-flash",
    system_prompt = "請使用繁體中文，用三個短句總結。",
    output = {
      path = "summary.txt",
      base = task.cwd,
      mode = "atomic_replace",
      encoding = "utf-8"
    }
  })
  await(ui.show_summary(result.text, { popup = false }))
end)
