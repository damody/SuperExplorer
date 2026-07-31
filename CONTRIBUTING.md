# Contributing to SuperExplorer

Effective date: July 31, 2026  
Project owner: Damody  
Languages: **English** | [繁體中文](CONTRIBUTING.zh-TW.md) | [简体中文](CONTRIBUTING.zh-CN.md)

> SuperExplorer is proprietary, source-available software, not an open-source project. This guide is a commercial legal/workflow template and should be reviewed by qualified counsel before release.

## 1. Controlling text

The three language versions have the same intended meaning. If they differ, the [Simplified Chinese version](CONTRIBUTING.zh-CN.md) controls.

## 2. Ways to participate

You may report issues, propose designs, improve documentation, prepare core pull requests, or build independent plugins. Reviewing source or participating does not grant a general license to redistribute SuperExplorer. Your source access remains subject to the [EULA](EULA.md).

## 3. Core contributions and plugins are separate

A change to SuperExplorer core, repository-owned SDK material, documentation, tests, or project assets is a **Core Contribution**. An independently maintained extension using published interfaces is a **Plugin** and is governed by the [Plugin SDK License](PLUGIN-SDK-LICENSE.md). Do not submit an entire independent Plugin as core unless Damody asks you to do so.

## 4. CLA required before merge

Every Core Contribution requires prior acceptance of the [Contributor License Agreement](CLA.md). A pull request must not be merged until acceptance is attributable to the contributor's GitHub identity. Corporate contributors must have authority from their employer where required.

Until automated CLA signing is available, include this exact statement in the pull request description:

> I have read and accept CLA.md, including the controlling-language and dispute terms, for this contribution. I have authority to submit it under those terms.

Record the contributor's GitHub account, pull request URL, commit IDs, statement, and acceptance timestamp in the pull request history. Damody may require a separately signed copy for material or corporate contributions.

## 5. Before opening a pull request

1. Search existing issues, OpenSpec changes, and pull requests.
2. Discuss substantial behavioral or architectural changes before implementation.
3. Keep the change focused and avoid unrelated formatting or refactoring.
4. Add or update tests and user-facing documentation.
5. Identify all third-party code, generated content, AI-assisted content, assets, tools, and licenses.
6. Confirm that no confidential, employer-owned, unlawfully copied, or restricted material is included.

## 6. Pull request contents

Each pull request must explain the problem, solution, important design choices, validation performed, compatibility impact, and third-party material. Commits must be reviewable and must not contain credentials, personal data, build outputs, or unrelated files. Damody may request changes, split work, or close a proposal without accepting it.

## 7. Review and acceptance

Submission does not require Damody to review, merge, release, maintain, credit, or pay for a contribution. Acceptance occurs only when Damody merges or otherwise confirms it in writing. Damody may modify, combine, relicense, commercialize, or later remove an accepted Core Contribution under the CLA.

## 8. Plugin publication

Independent Plugin authors retain their original rights and may publish under the Plugin SDK License. Official Steam or other official-channel distribution requires the [Plugin Publishing Agreement](PLUGIN-PUBLISHING-AGREEMENT.md), including its revenue, maintenance, tax, and takedown terms.

## 9. Conduct and security

Be respectful and avoid harassment, unlawful content, privacy violations, or deceptive claims. Do not disclose an unpatched vulnerability publicly; report it privately through a contact method designated in the repository. Do not include exploit payloads beyond what is necessary for safe reproduction.

## 10. Governing terms and contact

The contribution workflow and related CLA use PRC law and one-arbitrator CIETAC arbitration seated in Beijing and conducted in Chinese, subject to non-waivable consumer protections, small-claims remedies, and urgent court relief described in the CLA. English evidence may be submitted unless translation is required. Questions may be submitted through the [SuperExplorer GitHub repository](https://github.com/damody/SuperExplorer).

Copyright © 2025–2026 Damody. All rights reserved.
