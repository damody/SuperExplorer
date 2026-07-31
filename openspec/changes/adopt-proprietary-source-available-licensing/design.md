## Context

SuperExplorer currently declares `GPL-3.0-or-later OR LicenseRef-Commercial` in its root license and Cargo workspace metadata, and its three READMEs describe a GPL/commercial dual-license model. The intended release is instead proprietary and source-available: the repository remains readable, contributors can prepare core changes under a CLA, plugin authors can build independent commercial plugins, and Damody can sell the application and official plugins through Steam. All 258 original commits were authored under one identity, the product has not been released, the working tree began clean, and the user has an existing backup.

The repository vendors third-party code whose licenses and notices remain legally distinct from the SuperExplorer license. Those materials must be preserved even though SuperExplorer's own GPL grant is removed.

## Goals / Non-Goals

**Goals:**

- Establish a coherent proprietary, source-available legal framework in English, Traditional Chinese, and Simplified Chinese.
- Allow source review, contribution preparation, and independent plugin development without granting a general right to redistribute the core product.
- Define business-seat pricing, CLA rights, official plugin revenue sharing, maintenance responsibilities, and CIETAC dispute terms without unresolved placeholders.
- Remove project-owned GPL and open-source claims while retaining third-party license compliance.
- Rewrite `master` into one root commit and synchronize the remote using an explicit force-with-lease guard.

**Non-Goals:**

- Implement checkout, license keys, entitlement enforcement, Steamworks APIs, CLA automation, payout processing, tax reporting, or acceptance telemetry.
- Change third-party licenses, provenance, notices, or attribution.
- Guarantee enforceability in every jurisdiction or replace review by qualified counsel.
- Revoke copies already obtained through clones, forks, downloads, or caches.

## Decisions

### Use five document families with three files each

Each document family has an English base file plus `.zh-TW.md` and `.zh-CN.md` translations. Separate files render cleanly on GitHub, permit direct links from matching README languages, and make review diffs manageable. A single multilingual file was rejected because it would be long, harder to navigate, and easier to update inconsistently.

Simplified Chinese controls interpretation because the agreement uses PRC law, Chinese-language arbitration, and a Beijing seat. Every document cross-links its translations and repeats the controlling-language rule.

### Use a proprietary source-available grant rather than no express license

The EULA expressly permits source inspection, personal study, plugin development, and modifications necessary to prepare a contribution. It permits GitHub-hosted forks only for those purposes and prohibits independent release, redistribution, sale, or operation of a modified core product. Relying only on default copyright was rejected because it would leave contributors and plugin authors uncertain about cloning, modification, and SDK use.

Personal Steam use is a perpetual non-transferable license after one purchase. Employee work use and company-internal plugin development require annual seats priced at US$5 or RMB¥30 per user, using the checkout or invoice currency. Third-party materials are expressly excluded from the EULA and remain under their own terms.

### Separate core, SDK, contribution, and publishing rights

The Plugin SDK License permits independent free or paid plugins while preventing redistribution of core code and misleading official branding. Core contributions require the CLA, which preserves contributor ownership but grants Damody broad irrevocable copyright, patent, sublicensing, commercialization, and relicensing rights. Official Steam distribution requires the separate Plugin Publishing Agreement so store operations and revenue obligations do not leak into the general SDK grant.

A copyright assignment was rejected as unnecessarily burdensome. The broad CLA license supplies the commercial and relicensing rights required while letting contributors retain ownership.

### Fix official plugin economics in the publishing agreement

Authors receive 90% and SuperExplorer receives 10% of net revenue after platform fees, refunds, discounts, chargebacks, and required withholding. Statements are quarterly, payment is due within 45 days after quarter-end, and balances below US$100 carry forward. The agreement also grants the distribution and compatibility-maintenance rights needed to support existing purchasers if an author stops maintaining a plugin.

### Use one CIETAC dispute framework with mandatory-law exceptions

All document families use PRC law and CIETAC arbitration seated in Beijing before one arbitrator, conducted in Chinese. English evidence is accepted unless translation is required. Mandatory consumer remedies and small-claims rights are preserved, and courts with jurisdiction remain available for urgent intellectual-property or confidentiality relief. Region-specific dispute clauses were rejected because they create version selection and cross-border consistency problems.

### Rewrite history only after content validation

The legal files, README changes, Cargo metadata, and scans are completed and validated before history is replaced. A new root commit is built from the verified final tree and named `Initial proprietary source-available release`. The remote update uses an explicit expected old `origin/master` object ID so concurrent remote changes cause the push to fail instead of being overwritten.

No backup tag or bundle is created because the user already has a backup; retaining a tag would also keep the old history reachable. The old branch is not deleted until the new root tree is ready.

## Risks / Trade-offs

- [AI-drafted legal text may not be fully enforceable] → Label the documents as commercial templates, preserve mandatory-law exceptions, and require qualified counsel review before release.
- [Localized documents may diverge] → Keep identical section numbering and validate prices, percentages, deadlines, governing terms, and cross-links across all three languages.
- [Public source may still be copied despite restrictions] → State permissions and prohibitions clearly; do not imply that a Git rewrite can recall existing copies.
- [A force push could overwrite concurrent remote work] → Record the remote object ID and use an explicit force-with-lease comparison.
- [A Cargo `LicenseRef` expression may be rejected by tooling] → Keep the workspace unpublished and validate with `cargo metadata --no-deps`; adjust only if the actual parser rejects it.
- [Consumer arbitration may be restricted] → Preserve non-waivable consumer and small-claims remedies instead of presenting arbitration as absolute.
- [A mainland China award needs a separate Taiwan recognition procedure] → Avoid claims of automatic enforcement and state only the contractual forum and governing law.

## Migration Plan

1. Record and fetch the current remote state.
2. Add and cross-link all fifteen documents, delete only the two project-owned legacy license files, and update Cargo and README metadata.
3. Scan project-owned content for GPL, dual-license, and open-source claims while separately checking that third-party notices remain.
4. Validate document structure, numbers, links, and Cargo metadata.
5. Create a temporary orphan branch from the final index, commit the complete tree, and repoint `master` to the new root commit without retaining a tag to the old history.
6. Verify the single-parentless-commit invariant and a clean worktree.
7. Force-push with an explicit lease, fetch, and confirm local and remote refs match.

If validation fails before the push, retain the current branch and correct the content. If the protected push fails, fetch and inspect the remote change rather than overriding it. After a successful remote rewrite, recovery uses the user's external backup because no repository-local rollback ref is retained.

## Open Questions

None. The document set, controlling language, pricing, revenue share, settlement timing, dispute framework, backup decision, and force-push authorization are approved.
