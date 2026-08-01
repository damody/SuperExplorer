# Offline SDK CI gate

The workflow is restricted to self-hosted `windows`, `x64`, and
`hyperv-offline` labels. It invokes the runner-owned
`C:\ProgramData\SuperExplorerCI\Invoke-OfflineSdkGuest.ps1`; the repository
template is not an authority for credentials or guest readiness.

The guest must have zero NICs/routes before and after execution. It copies the
bundle through PowerShell Direct, runs the offline fixture/host/plugin gates
and egress sentinel, then emits an attestation matching
`schemas/offline-build-attestation.schema.json`. Hyper-V execution is not
performed by local tests.
