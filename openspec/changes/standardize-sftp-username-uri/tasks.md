## 1. SFTP URI contract

- [x] 1.1 Update the shared SFTP input parser to split `username@host`, reject unsafe user-info, and retain host-only canonical remote identity.
- [x] 1.2 Add model tests for standard input, host-only compatibility, legacy-order non-inference, and password-safe rejection.

## 2. Application integration

- [x] 2.1 Update address-login integration tests to prove standard username prefill reaches host-based navigation.

## 3. Completion checks

- [x] 3.1 Run focused model, UI, and application tests and correct failures.
- [x] 3.2 Run strict OpenSpec validation and inspect the completed artifacts for contract contradictions.
- [x] 3.3 Perform the user-perspective address-entry, login, and navigation review.
