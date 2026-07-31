## 1. Establish Licensing Documents

- [x] 1.1 Create the English, Traditional Chinese, and Simplified Chinese EULA files with matching sections, source-available permissions, personal and business terms, restrictions, exceptions, and cross-language links.
- [x] 1.2 Create the three Plugin SDK License files with independent free/paid plugin rights, core-code restrictions, commercial-seat treatment, branding limits, and official-channel publishing requirement.
- [x] 1.3 Create the three CONTRIBUTING files with core/plugin workflow separation, PR requirements, and attributable CLA acceptance gate.
- [x] 1.4 Create the three CLA files with contributor ownership, broad copyright and patent grants, representations, third-party disclosure, employer authority, and moral-rights treatment.
- [x] 1.5 Create the three Plugin Publishing Agreement files with ownership, distribution and continuity rights, 90/10 net-revenue split, quarterly reporting, 45-day payment, US$100 threshold, maintenance, takedown, tax, and termination terms.

## 2. Replace Project License Metadata

- [x] 2.1 Delete only the project-owned `LICENSE` and `LICENSE-Commercial.txt` files.
- [x] 2.2 Change the root Cargo workspace license metadata to `LicenseRef-SuperExplorer-Proprietary` without changing third-party manifests.
- [x] 2.3 Replace the license sections in all three READMEs with matching proprietary/source-available wording and links to the corresponding legal and contribution documents.
- [x] 2.4 Scan project-owned content and update any remaining active GPL, dual-license, or SuperExplorer open-source claims while preserving historical technical references and third-party license requirements.

## 3. Validate Legal and Repository Content

- [x] 3.1 Verify all fifteen files exist, cross-link correctly, use matching section order, and identify Simplified Chinese as controlling.
- [x] 3.2 Compare all translations for exact commercial-seat prices, 90/10 revenue split, quarterly/45-day/US$100 settlement terms, CIETAC Beijing terms, and mandatory-law exceptions.
- [x] 3.3 Verify required license, notice, provenance, and attribution files under `vendor/`, `third_party/`, and `build/tools/` remain unchanged and outside the SuperExplorer EULA.
- [x] 3.4 Run project-owned GPL/open-source scans, `git diff --check`, OpenSpec validation, and `cargo metadata --no-deps`; correct every failure before rewriting history.

## 4. Create and Publish the Single-Commit Baseline

- [x] 4.1 Fetch `origin/master` and record its exact object ID for the protected push lease.
- [x] 4.2 Build a temporary orphan branch from the validated final index and commit the complete tree as `Initial proprietary source-available release`.
- [x] 4.3 Repoint `master` without creating a backup tag, branch, or in-repository bundle; verify one parentless commit and a clean worktree.
- [x] 4.4 Force-push `master` using the recorded explicit lease and stop for inspection if the lease does not match.
- [x] 4.5 Fetch the remote and verify local `master` and `origin/master` match and each exposes exactly one reachable commit.
