---@meta explorer-automation/v1

---@class AutomationTask
---@field id string
---@field cwd string Immutable directory captured when the event was queued.
---@field deadline_unix_ms? integer

---@class AutomationEvent
---@field name string
---@field version integer
---@field sequence integer
---@field timestamp_unix_ms integer
---@field source string
---@field context table
---@field data table

---@class ScriptConfig
---@field name? string
---@field activation? 'always'|'temporary'
---@field default_dispatch? 'queue'|'parallel'|'latest'|'drop'
---@field task_timeout? string|integer

script = {}
---@param config ScriptConfig
function script.configure(config) end

---@param event_filter string
---@param options_or_callback table|fun(event: AutomationEvent, task: AutomationTask)
---@param callback? fun(event: AutomationEvent, task: AutomationTask)
function on(event_filter, options_or_callback, callback) end

---@param chord string
---@param callback fun(event: AutomationEvent, task: AutomationTask)
function hotkey(chord, callback) end

---@param options table
function watch(options) end

schedule = {}
function schedule.once(delay, callback) end
function schedule.every(interval, callback) end
function schedule.cron(expression, timezone, callback) end

fs = {}; process = {}; clipboard = {}; ui = {}; ai = {}
function await(operation) return operation end
function sleep(duration) end
function spawn(callback, parent_task) end
function fs.read_text(path, options) end
function fs.read_bytes(path, options) end
function fs.write_text(path, text, options) end
function fs.append_text(path, text, options) end
function fs.write_json(path, value, options) end
function fs.write_bytes(path, bytes, options) end
function fs.remove(path, options) end
function process.run(executable, argv, options) end
function process.run_script(path, argv, options) end
function clipboard.read_text() end
function ui.notify(title, body) end
function ui.show_summary(text, options) end
function ai.summarize(options) end
