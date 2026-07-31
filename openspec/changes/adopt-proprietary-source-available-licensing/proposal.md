## Why

SuperExplorer is not yet released and its existing GPL/commercial dual-license metadata conflicts with the intended proprietary, source-available business model. The repository needs a coherent multilingual legal framework for users, contributors, plugin authors, and official Steam publishing before commercial distribution begins.

## What Changes

- **BREAKING**: Remove SuperExplorer's GPL-3.0-or-later and dual-license grant and replace it with proprietary, source-available terms.
- Add English, Traditional Chinese, and Simplified Chinese versions of the EULA, Plugin SDK License, contribution guide, CLA, and Plugin Publishing Agreement, with Simplified Chinese controlling interpretation.
- Define personal Steam use, low-cost annual business seats, source inspection and contribution permissions, independent plugin rights, and restrictions on redistribution of the core product.
- Define CIETAC arbitration seated in Beijing under PRC law, subject to mandatory consumer remedies and urgent intellectual-property relief.
- Define an official plugin publishing model in which authors retain copyright and receive 90% of net revenue, with quarterly statements and threshold-based payment.
- Preserve third-party licenses and notices while removing only project-owned GPL and open-source claims.
- **BREAKING**: Replace the existing `master` history with one root commit and force-update `origin/master` using an explicit lease.

## Capabilities

### New Capabilities

- `proprietary-licensing-governance`: Governs the multilingual proprietary EULA, source inspection, commercial seats, plugin SDK permissions, contributions, CLA grants, official plugin publishing, third-party exclusions, and dispute terms.
- `single-commit-release-baseline`: Governs the protected rewrite of the unreleased repository into one verified root commit and synchronized remote baseline.

### Modified Capabilities

None.

## Impact

- Removes root `LICENSE` and `LICENSE-Commercial.txt` and adds fifteen multilingual legal and contribution documents.
- Updates `Cargo.toml` and the three README files to identify SuperExplorer as proprietary and source-available rather than GPL or open source.
- Leaves dependency license, notice, provenance, and attribution files under `vendor/`, `third_party/`, and `build/tools/` intact.
- Rewrites local and remote `master` history, requiring existing clones to re-clone or reset explicitly.
- Does not implement checkout, license enforcement, Steamworks integration, CLA automation, payment processing, or tax reporting.
