# SuperExplorer Proprietary EULA and History Squash Design

Date: 2026-07-31

## Objective

Replace SuperExplorer's GPL/commercial dual-license model with a proprietary, source-available commercial model. The public repository remains readable and may be used to prepare contributions and plugins, but SuperExplorer is not open-source software. Preserve all third-party license and notice files. Replace the current Git history with one root commit and force-update `origin/master` because the software has not been released.

## Ownership and Positioning

- Licensor and project owner: Damody.
- SuperExplorer is proprietary, source-available software.
- Publishing source code on GitHub does not grant an open-source license.
- Repository users may inspect the source and use GitHub-hosted forks solely for evaluation, learning, plugin development, and preparing contributions.
- Users may not distribute, publish, sell, or operate a fork as an independent or competing SuperExplorer product.
- Third-party dependencies remain governed exclusively by their own licenses and notices.

## Document Set and Languages

Create five documents in English, Traditional Chinese, and Simplified Chinese:

| Purpose | English | Traditional Chinese | Simplified Chinese |
| --- | --- | --- | --- |
| Core software license | `EULA.md` | `EULA.zh-TW.md` | `EULA.zh-CN.md` |
| Plugin SDK license | `PLUGIN-SDK-LICENSE.md` | `PLUGIN-SDK-LICENSE.zh-TW.md` | `PLUGIN-SDK-LICENSE.zh-CN.md` |
| Contribution process | `CONTRIBUTING.md` | `CONTRIBUTING.zh-TW.md` | `CONTRIBUTING.zh-CN.md` |
| Contributor license agreement | `CLA.md` | `CLA.zh-TW.md` | `CLA.zh-CN.md` |
| Official plugin publishing agreement | `PLUGIN-PUBLISHING-AGREEMENT.md` | `PLUGIN-PUBLISHING-AGREEMENT.zh-TW.md` | `PLUGIN-PUBLISHING-AGREEMENT.zh-CN.md` |

The three versions must have materially identical terms. The Simplified Chinese version controls if a translation or interpretation differs. Each document must link to its other two language versions.

Delete the project-owned `LICENSE` and `LICENSE-Commercial.txt`. Do not delete or rewrite licenses, notices, provenance records, or attribution belonging to dependencies under `vendor/`, `third_party/`, or `build/tools/`.

## Common Dispute Terms

All five document families use one dispute framework:

- Governing law: laws of the People's Republic of China, without regard to conflict-of-laws principles.
- Institution: China International Economic and Trade Arbitration Commission (CIETAC).
- Seat: Beijing, China.
- Tribunal: one arbitrator.
- Arbitration language: Chinese; English-language evidence may be submitted without translation unless the tribunal requires otherwise.
- Award: final and binding.
- Mandatory consumer protection rights and non-waivable small-claims remedies remain available.
- Damody may seek urgent or interim relief from any court with jurisdiction for copyright infringement, unauthorized distribution, confidentiality breaches, or misuse of intellectual property.

The documents are commercial templates intended for professional legal review before release. They must not claim to replace jurisdiction-specific legal advice.

## EULA Terms

The EULA applies to SuperExplorer source code, binaries, documentation, and project-owned assets.

- Permit source inspection, personal research, learning, plugin development, and the copying or modification reasonably required to prepare a contribution.
- Permit GitHub-hosted forks for review and contribution, subject to the prohibition on independent distribution, release, sale, hosting, or commercialization of the core product or modified versions.
- Grant Steam personal users a paid-up, perpetual, non-transferable license for personal use after a one-time purchase.
- Require a commercial seat for employee work use or internal company plugin development.
- Price each commercial seat at either US$5 per user per year or RMB¥30 per user per year, as displayed at checkout or on the applicable invoice. Users may not select a currency solely to obtain an unintended exchange-rate discount.
- State that taxes are additional where required, paid periods are not repriced retroactively, and renewals use the then-published price.
- Allow complimentary development licenses for approved official plugin collaborators.
- Prohibit unauthorized reproduction, redistribution, publication of modified core versions, commercial exploitation of the core, removal of rights notices, circumvention of license controls, and misleading trademark use.
- Include termination, warranty disclaimer, limitation of liability, export/sanctions compliance, third-party components, updates, severability, and mandatory consumer-rights terms.
- Explain that placing an EULA in the repository alone does not record assent for binary users; the future Steam/install/first-run flow should display the applicable version, require affirmative acceptance, and retain the accepted version and timestamp.

## Plugin SDK License Terms

- Permit use of public APIs, headers, examples, schemas, and SDK tools to create independent free or paid plugins.
- Let authors retain copyright in their original plugin code and assets.
- Permit independent plugin distribution when the plugin does not contain, replace, or redistribute SuperExplorer core code.
- Prohibit claims of official endorsement, confusing use of SuperExplorer marks, bypassing security or licensing controls, and embedding incompatible or unlawfully licensed materials.
- Require a valid commercial seat for company-internal plugin development.
- Require a separately accepted Plugin Publishing Agreement for distribution through Damody's official Steamworks account or other official storefront channel.

## Contribution and CLA Terms

`CONTRIBUTING` describes the PR workflow, separates core contributions from independently owned plugins, and states that no core PR will be merged until the contributor accepts the CLA.

The CLA is a broad license rather than a copyright assignment:

- The contributor retains ownership of the original contribution.
- The contributor grants Damody a perpetual, worldwide, irrevocable, royalty-free, transferable, and sublicensable copyright license.
- The grant permits use, reproduction, modification, integration, publication, distribution, sale, commercial exploitation, relicensing, and changing the project's license terms.
- Include a patent license for claims necessarily infringed by the contribution and a defensive termination provision.
- Require representations of originality, authority to contribute, disclosure of third-party material, and employer authorization where applicable.
- Include waiver or non-assertion of moral rights to the extent legally permitted.
- Record acceptance using an attributable GitHub identity plus the contribution workflow; future automation may add a dedicated CLA-signing record.

## Plugin Publishing Agreement Terms

- Plugin copyright remains with the author by default.
- The author grants Damody a worldwide, non-exclusive, transferable, sublicensable license to distribute, market, demonstrate, support, preserve, and make compatibility or security modifications to the plugin through Steam and official channels.
- Net revenue is split 90% to the plugin author and 10% to SuperExplorer.
- Net revenue means receipts remaining after Steam/platform fees, refunds, discounts, chargebacks, and legally required withholding taxes.
- Statements are prepared quarterly. Payment is due within 45 days after quarter-end.
- Amounts below US$100 carry forward until the threshold is reached; final undisputed balances are paid after termination even if below the threshold.
- The author supplies accurate payment and tax information and remains responsible for the author's own taxes.
- The author warrants ownership and lawful use of code, dependencies, branding, media, and other materials.
- The author is responsible for reasonable maintenance and timely security fixes.
- If the author stops maintaining the plugin, Damody may fix it, appoint another maintainer, suspend sales, or delist it. Necessary download and support rights for existing customers survive.
- Include review/rejection rights, reporting, audits limited to relevant records, refunds, takedown, indemnity, confidentiality, termination, and surviving customer-support obligations.

## Repository Metadata and Messaging

- Replace the root Cargo license expression with `LicenseRef-SuperExplorer-Proprietary` while keeping `publish = false`.
- Update the English, Traditional Chinese, and Simplified Chinese README license sections to state that SuperExplorer is proprietary and source-available, not open source.
- Link each README to the matching EULA, Plugin SDK License, contribution guide, CLA, and plugin publishing agreement.
- Preserve references to dependency licenses where they describe third-party compliance or package validation.
- Remove only claims that SuperExplorer itself is GPL-licensed, dual-licensed, or open source. Generic technical uses of the word "license" and third-party license requirements remain.

## Git History Rewrite

The user already maintains a backup; no additional bundle or tag will be created.

1. Record the current `origin/master` object ID for force-with-lease protection.
2. Complete and validate all document and metadata changes.
3. Create a new `master` history containing a single root commit whose tree matches the final working tree.
4. Use commit message `Initial proprietary source-available release`.
5. Confirm the new commit has no parent, `git rev-list --count master` equals one, and the working tree is clean.
6. Force-push `master` using an explicit lease against the previously recorded remote object ID.
7. Fetch and verify that local `master` and `origin/master` reference the same single root commit.

The rewrite cannot recall existing clones, forks, downloads, caches, or copies. GitHub may retain unreachable objects temporarily.

## Validation

- Confirm all 15 legal files exist and cross-link correctly.
- Compare the three language versions for matching section structure, numbers, prices, revenue percentages, payment schedule, dispute terms, and exceptions.
- Search project-owned files for GPL identifiers, dual-license claims, and claims that SuperExplorer is open source.
- Exclude third-party and generated dependency directories from the project-owned cleanup scan, then separately verify their required license and notice files still exist.
- Run `cargo metadata --no-deps` to validate the workspace manifest after changing the license expression.
- Confirm deleted project-owned license files are absent.
- Confirm the Git history and remote reference checks described above.

## Scope Boundaries

- This change does not implement checkout, license-key enforcement, Steamworks integration, CLA automation, tax reporting, or payment processing.
- It does not remove third-party open-source licenses or alter third-party attribution.
- It does not guarantee enforcement in every jurisdiction; mandatory law and the facts of assent and use remain relevant.
- The final legal text should be reviewed by qualified counsel before commercial release.
