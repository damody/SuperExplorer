## ADDED Requirements

### Requirement: Provider-neutral asynchronous AI boundary
The system SHALL expose asynchronous summary and chat operations behind a provider-neutral interface with cancellation, timeout, streaming, and typed errors.

#### Scenario: AI task is cancelled
- **WHEN** its script task is cancelled while a request is active
- **THEN** the request stops and the task receives a cancellation result without blocking GPUI

### Requirement: DeepSeek V4 Flash provider
The initial provider SHALL call the OpenAI-compatible DeepSeek API at `https://api.deepseek.com` using model `deepseek-v4-flash`.

#### Scenario: DeepSeek summary succeeds
- **WHEN** Lua submits valid text with configured credentials
- **THEN** the provider returns the summary text and usage metadata through the AI result

### Requirement: Credential and diagnostic privacy
The system SHALL store the DeepSeek API key in Windows Credential Manager and SHALL exclude keys, prompts, and responses from settings, panic reports, and persistent diagnostics.

#### Scenario: Diagnostics are emitted
- **WHEN** an AI request completes or fails
- **THEN** diagnostics contain only safe metadata such as provider, model, sizes, duration, correlation ID, and result kind

### Requirement: Bounded retry behavior
The provider SHALL retry retryable 429 and 5xx responses with jittered backoff at most two times and SHALL return permanent authentication or validation failures immediately.

#### Scenario: Repeated rate limit
- **WHEN** all initial and retry attempts receive 429
- **THEN** the task receives a rate-limit error after no more than two retries

### Requirement: Direct atomic TXT output
Lua SHALL be able to request a summary and atomically save the returned text to a task-relative or explicit UTF-8 TXT path while also receiving the text and resolved output path.

#### Scenario: AI succeeds but output fails
- **WHEN** DeepSeek returns text but the selected file cannot be atomically written
- **THEN** the combined operation reports a file-output failure and does not report full completion or leave a partial TXT

### Requirement: Configurable summary presentation
The UI SHALL present summaries in either a dockable side panel or a small popup according to user settings, while Lua calls remain free to consume results without forced presentation.

#### Scenario: Popup mode is selected
- **WHEN** a UI summary action completes with popup mode configured
- **THEN** the result appears in a non-blocking popup with copy and error/retry affordances
