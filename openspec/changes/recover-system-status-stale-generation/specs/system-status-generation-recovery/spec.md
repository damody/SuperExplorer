## ADDED Requirements

### Requirement: Stale status generation is recovered once
SuperDesktop SHALL resynchronize over the same command-client transport and retry a system-status command exactly once when the receiving host rejects its first attempt as `StaleGeneration` before platform dispatch.

#### Scenario: Volume command crosses one host restart
- **WHEN** the displayed snapshot belongs to an old status host and the user changes volume
- **THEN** SuperDesktop fetches the new host snapshot and sends the unchanged target volume once with the new generation

#### Scenario: Mute command crosses one host restart
- **WHEN** a mute command receives `StaleGeneration` on its first attempt
- **THEN** SuperDesktop resynchronizes and completes the mute action without requiring another user gesture

#### Scenario: Observer and command clients use different hosts
- **WHEN** the UI reconciler contains a numerically newer observer-host generation than the host receiving commands
- **THEN** SuperDesktop uses the command host's own snapshot generation for the retry even if the reconciler declines that separate lineage

#### Scenario: Second generation race
- **WHEN** the retry also receives `StaleGeneration`
- **THEN** SuperDesktop stops retrying and reports the final failure to the console

### Requirement: Recovery preserves command identity safety
SuperDesktop SHALL use a unique correlation ID and fresh deadline for every attempt, and the status host MUST reject generation mismatch before invoking any platform command.

#### Scenario: Rejected stale attempt has no side effect
- **WHEN** a request's expected host generation does not match the current host
- **THEN** the host returns `StaleGeneration` without invoking Core Audio, input, language, or Wi-Fi adapters

#### Scenario: Retry request identity
- **WHEN** SuperDesktop retries after resynchronization
- **THEN** the retry correlation ID differs from the rejected attempt and its deadline is calculated from retry time

### Requirement: Status UI converges from authoritative observation
SuperDesktop SHALL refresh and apply an authoritative status snapshot after a terminal command response and SHALL NOT invent an optimistic volume or mute value.

#### Scenario: Volume write succeeds
- **WHEN** Core Audio observes the requested volume within tolerance
- **THEN** the host returns an observed terminal and the UI refreshes from the subsequent snapshot

#### Scenario: Resynchronization fails
- **WHEN** the fresh snapshot cannot be obtained or does not establish the current host generation
- **THEN** SuperDesktop does not replay the command and prints a bounded failure to the console

### Requirement: Recovered races are diagnostics rather than errors
SuperDesktop SHALL trace a successfully recovered stale-generation race and SHALL NOT emit it through the error reporter.

#### Scenario: Successful recovery logging
- **WHEN** attempt one is stale and attempt two completes successfully
- **THEN** the trace records stale detection, resynchronization, retry, and recovery without a `SuperDesktop error [status:command]` line
