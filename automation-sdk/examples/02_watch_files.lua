script.configure { name = "Temporary notes watcher", activation = "temporary" }

watch {
  root = "D:\\Notes",
  recursive = true,
  include = { "**/*.txt", "**/*.md" },
  exclude = { "**/summary/**", "**/~*" }
}

on("fs.*", { debounce = "500ms", dispatch = "queue" }, function(event, task)
  await(fs.append_text("watch-log.txt", event.name .. "\n", { base = task.cwd }))
end)
