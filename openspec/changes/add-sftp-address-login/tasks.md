## 1. Address and state contracts

- [x] 1.1 Add safe direct-host/username-hint parsing and canonicalization tests
- [x] 1.2 Add the application-owned native Credential UI login seam without model-owned secrets

## 2. Secure connection coordination

- [x] 2.1 Add atomic profile upsert and Credential Manager rollback behavior
- [x] 2.2 Add host-key probe/authentication and changed-key rejection flow
- [x] 2.3 Add runtime/provider/navigation refresh after successful persistence

## 3. Login UI and navigation

- [x] 3.1 Intercept unresolved direct SFTP address submission and open the login surface
- [x] 3.2 Implement native masked login, prefill, cancel, and redacted errors
- [x] 3.3 Canonicalize the address and navigate exactly once after success

## 4. Targeted verification

- [x] 4.1 Add parser, username precedence, and secret-isolation tests
- [ ] 4.2 Add success, authentication failure, and changed-host-key coordinator tests
- [x] 4.3 Run formatting, affected-crate tests, and strict OpenSpec validation
