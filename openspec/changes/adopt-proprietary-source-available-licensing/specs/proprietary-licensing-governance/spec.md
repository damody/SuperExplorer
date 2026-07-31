## ADDED Requirements

### Requirement: Multilingual legal document set
The repository SHALL contain English, Traditional Chinese, and Simplified Chinese versions of the EULA, Plugin SDK License, contribution guide, CLA, and Plugin Publishing Agreement. The documents SHALL contain materially identical terms, SHALL cross-link their language variants, and SHALL state that Simplified Chinese controls any translation or interpretation conflict.

#### Scenario: Complete language matrix
- **WHEN** the final repository tree is inspected
- **THEN** all fifteen required files exist with matching section structure and language links

#### Scenario: Translation conflict rule
- **WHEN** any legal document is read in any supported language
- **THEN** it identifies the Simplified Chinese version as the controlling text

### Requirement: Proprietary source-available EULA
The EULA SHALL identify SuperExplorer as proprietary and source-available, SHALL permit inspection, personal study, plugin development, and modifications needed to prepare contributions, and SHALL prohibit unauthorized redistribution, publication, sale, hosting, or commercial operation of the core product or modified versions.

#### Scenario: Contribution-oriented fork
- **WHEN** a user creates a GitHub-hosted fork solely to inspect the source or prepare a contribution
- **THEN** the EULA permits that activity subject to its non-distribution restrictions

#### Scenario: Independent modified release
- **WHEN** a user attempts to distribute or sell a modified SuperExplorer core build without separate permission
- **THEN** the EULA identifies the activity as prohibited

### Requirement: Personal and business license terms
The EULA SHALL grant a paid-up perpetual non-transferable personal-use license following a Steam personal purchase and SHALL require annual commercial seats for employee work use and company-internal plugin development. Each commercial seat SHALL be priced at US$5 or RMB¥30 per user per year according to the checkout or invoice currency, exclusive of required taxes.

#### Scenario: Personal Steam purchaser
- **WHEN** an individual completes a one-time Steam personal purchase and complies with the EULA
- **THEN** the individual receives a perpetual non-transferable personal-use license

#### Scenario: Company employee use
- **WHEN** a company uses SuperExplorer for employee work or internal plugin development
- **THEN** each applicable user requires a current annual commercial seat at the displayed or invoiced currency price

### Requirement: Independent plugin SDK rights
The Plugin SDK License SHALL permit authors to use public APIs, headers, examples, schemas, and SDK tools to create and independently distribute free or paid plugins while retaining copyright in their original work. It SHALL prohibit redistribution of SuperExplorer core code, bypass of security or licensing controls, and misleading official branding.

#### Scenario: Independently sold plugin
- **WHEN** an author builds a plugin using only permitted SDK materials and original or lawfully licensed content
- **THEN** the author may distribute the plugin independently without assigning plugin copyright to Damody

#### Scenario: Official Steam distribution
- **WHEN** an author requests distribution through Damody's Steamworks account or another official channel
- **THEN** the author must separately accept the Plugin Publishing Agreement

### Requirement: Core contribution governance
The contribution guide SHALL require CLA acceptance before any core pull request is merged. The CLA SHALL preserve contributor ownership while granting Damody perpetual, worldwide, irrevocable, royalty-free, transferable, and sublicensable copyright and necessary patent rights sufficient to modify, integrate, publish, distribute, sell, commercially exploit, relicense, and change project license terms.

#### Scenario: Core pull request without CLA
- **WHEN** a contributor submits a core change without attributable CLA acceptance
- **THEN** the contribution guide requires the pull request to remain unmerged

#### Scenario: Authorized contribution
- **WHEN** a contributor accepts the CLA and submits material the contributor is authorized to provide
- **THEN** Damody receives the specified commercial, sublicensing, and relicensing rights while the contributor retains ownership

### Requirement: Official plugin publishing economics and continuity
The Plugin Publishing Agreement SHALL allocate 90% of net revenue to the author and 10% to SuperExplorer after platform fees, refunds, discounts, chargebacks, and required withholding. It SHALL require quarterly statements, payment within 45 days after quarter-end, and carry-forward of balances below US$100, while preserving payment of a final undisputed balance after termination.

#### Scenario: Quarterly payment above threshold
- **WHEN** an author's payable balance for a quarter is at least US$100
- **THEN** the agreement requires payment within 45 days after quarter-end

#### Scenario: Author stops maintenance
- **WHEN** an author no longer provides reasonable maintenance or security fixes
- **THEN** the agreement permits Damody to repair, reassign maintenance, suspend sales, or delist while preserving necessary existing-customer download and support rights

### Requirement: Common dispute framework and mandatory rights
All five document families SHALL use PRC law and CIETAC arbitration seated in Beijing before one arbitrator in Chinese, with English evidence permitted unless translation is required. They SHALL preserve non-waivable consumer protections and small-claims remedies and SHALL permit urgent court relief for intellectual-property, unauthorized-distribution, and confidentiality violations.

#### Scenario: Contractual commercial dispute
- **WHEN** a dispute covered by an agreement cannot be resolved informally and no mandatory-law exception applies
- **THEN** the agreement directs it to one-arbitrator CIETAC proceedings seated in Beijing under PRC law

#### Scenario: Mandatory consumer remedy
- **WHEN** applicable consumer law makes a local remedy non-waivable
- **THEN** the agreement does not purport to eliminate that remedy

### Requirement: Project and third-party license separation
The repository SHALL remove the project-owned GPL and commercial dual-license files and claims, SHALL identify SuperExplorer as proprietary and source-available in Cargo metadata and all three READMEs, and MUST retain third-party licenses, notices, provenance, and attribution under dependency directories.

#### Scenario: Project-owned licensing scan
- **WHEN** project-owned source and documentation are scanned after migration
- **THEN** no active GPL grant, dual-license claim, or claim that SuperExplorer is open source remains

#### Scenario: Third-party compliance scan
- **WHEN** dependency directories are inspected after migration
- **THEN** their pre-existing license, notice, provenance, and attribution files remain present and outside the SuperExplorer EULA grant
