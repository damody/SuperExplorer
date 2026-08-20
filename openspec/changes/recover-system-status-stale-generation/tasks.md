## 1. Command recovery contract

- [x] 1.1 Extract bounded system-status request construction with unique correlations and fresh deadlines
- [x] 1.2 Classify stale terminals before reconciler insertion or console reporting
- [x] 1.3 Resynchronize from an authoritative snapshot and retry the unchanged command once
- [x] 1.4 Stop after a second stale terminal or failed resynchronization and report the final error
- [x] 1.5 Preserve final snapshot refresh and input-method Start-focus restoration

## 2. Deterministic regression coverage

- [x] 2.1 Add ordinary-success and non-stale failure tests
- [x] 2.2 Add stale-snapshot-success sequence tests for volume and mute
- [x] 2.3 Add second-stale, invalid-response, and resync-failure tests
- [x] 2.4 Prove unique correlation IDs, fresh deadlines, and the two-attempt maximum
- [x] 2.5 Prove the status host rejects generation mismatch before platform dispatch

## 3. Automated quality gates

- [x] 3.1 Run focused superdesktop-app, system-status-host, protocol, and platform-win tests
- [x] 3.2 Run the complete SuperDesktop workspace test suite
- [x] 3.3 Run workspace Clippy with warnings denied
- [x] 3.4 Run the locked offline release build

## 4. Headful volume verification

- [x] 4.1 Add a bounded UTIT case that records and restores the original endpoint volume and mute state
- [x] 4.2 Force a status-host restart between snapshot and pointer/keyboard volume commands
- [x] 4.3 Verify endpoint and flyout convergence with recovery traces and no status command error
- [x] 4.4 Run and validate the headful report with screenshots and console evidence

## 5. Packaging and closure

- [ ] 5.1 Build the no-launch installer and bind its hash to the updated SuperDesktop executable
- [ ] 5.2 Record every task result, command, artifact, and hash in the evidence index
- [ ] 5.3 Run strict OpenSpec validation with all tasks complete
- [ ] 5.4 Commit SuperDesktop, update the parent gitlink, and commit final integration
