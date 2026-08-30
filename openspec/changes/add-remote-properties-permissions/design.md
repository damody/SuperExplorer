# Design

## Decision

Use one GPUI dialog backed by a typed `SetUnixMode` file operation. The model accepts only permission and special bits. The remote service resolves the provider; ADB uses validated argument-array execution and SFTP uses `SETSTAT`. The local Shell provider returns unsupported if the remote-only operation crosses that boundary.

## Data flow

Context menu → selected remote `FileEntry` snapshot → permission draft → `FileOperationRequest` → remote worker/provider → operation terminal → directory reconciliation.

## Failure and security

Missing mode metadata disables the dialog. Modes containing file-type bits are rejected. Paths remain structured descriptors until the provider boundary. ADB path quoting uses the existing hardened helper; SFTP transmits a metadata object with only `permissions` populated. Errors remain user-safe and technical details stay in diagnostics.

## Testing

Cover command projection, draft bit toggles, request generation, mode validation, ADB quoting, all-target compilation, and strict specification validation.
