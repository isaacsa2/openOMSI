# Data inventory and privacy publication checklist

This document is a technical inventory for maintainers, **not** a certification of legal compliance or a substitute for legal review. It describes the source files reviewed on 2026-11-01 and flags decisions that must be made by the operators of each service. Do not publish unverified retention periods or controller details.

## Scope and responsibility

| Function | Primary operator to assess | Data potentially involved | Code or documentation |
| --- | --- | --- | --- |
| Offline game | The person running the game | Local settings, saves, OMSI content and screenshots | `site/privacy/index.html` |
| Self-hosted multiplayer session | Host/player running the session (verify per case) | Player name, peer IP/network information, vehicle/position, messages, technical identifiers for bans | `crates/omsi-net/src/lib.rs`, `crates/omsi-lan/src/bans.rs` |
| Dedicated server | Operator of the dedicated server (verify per case) | Same multiplayer fields; local server logs, moderation history | `docs/SERVER.md`, `crates/omsi-lan/src/admin/` |
| Project server list / discovery | Operator of the listed service | Published server metadata, connection requests and network metadata | `crates/omsi-serverlist/`, `crates/omsi-lan/src/gateway.rs` |
| 'Playing now' counter | Operator of the counter service | Session token, version and broad platform type; requests may traverse third parties | `site/privacy/index.html` |
| Optional content downloads, online weather, radio, auto updates | Provider of the endpoint and the user initiating the request | Standard network request metadata, possibly device/platform and version | Download/update and weather integrations |
| Discord community and Content Creator applications | Operators of the specific form / community, and the relevant platform for its own processing | Application answers, names, email addresses, age, Discord account identifiers, messages | Forms and Discord configuration: **outside the game repository unless independently linked** |

Open source distribution does not automatically make the developers the operators of every user-hosted service. Conversely, software distribution does not remove duties attached to online features actually operated by the project. The final classification of provider/controller/processor must be checked per service and jurisdiction.

## Confirmed code details requiring review

- The LAN protocol exchanges player names, world/vehicle state and chat. A host has network addresses for connected peers.
- `omsi-net` accepts a hardware-derived identifier for admission and ban enforcement. It can be held by hosts; hashing/pseudonymisation does not by itself make persistent identifiers anonymous.
- At this baseline, routine LAN logging includes chat content and player names. Proposed fix: **fork PR #376**; do not claim text logging is removed until that fix is shipped and all logging paths are verified.
- The in-game chat has a profanity filter and rate limits, and voice is off by default. The profanity filter is not a guarantee against all harmful or illegal content.
- The core game cannot currently establish users' ages reliably. There is no general document-based verification or verified parent account in the inspected code. **Age-specific interactions and parental settings are out of scope of this patch.**
- Voice transport behaviour is documented in `docs/VOICE.md`; confirm deployment-specific relay and diagnostic logging before promising that external parties never process metadata.

## Required operator decisions (blocking final publication)

- [ ] Identify the controller / operator and a private, reliable contact point for privacy and data-subject requests for each service (not a public GitHub issue).
- [ ] Verify purposes and lawful bases for every personal-data category. Consent is not the sole possible basis.
- [ ] Confirm recipients, third-party processors/hosts, cross-border transfers and policies. Do not assume third-party relays are free of logs.
- [ ] Confirm actual retention periods (for connection logs, counter, hardware identifiers, bans, chat and form submissions), deletion triggers and access rights.
- [ ] Determine and document procedures for data access, correction, deletion and objections, with suitable exceptions for safety and legal obligations.
- [ ] Review current and historical creator-program forms separately: exact questions, who receives submissions, why each question is necessary, and which notice was presented **at the time** of submission.
- [ ] Complete per-service child-safety and illegal-content risk assessments as applicable (Brazilian ECA Digital, UK Online Safety Act, ICO Children's Code, EU GDPR and DSA where relevant); assess the legal need for age assurance from actual risks and obligations.
- [ ] Identify incident response and reporting channels; do not request identity documents or retain evidence without a defined necessity and legal basis.
- [ ] Confirm whether project-operated services have UK/EU/Brazilian jurisdictional reach before attributing legal obligations.

## Age assurance: decision for this patch

**No age gate, birth-date collection, ID verification, facial scan or parental-account system is being added in this work.** Do not conclude from this that age assurance or age-specific safeguards can never be required. A risk assessment must determine what is required for each regulated feature and service.

If age assurance becomes necessary, prefer interoperable age-band signals that disclose the minimum information required, subject to reliability and availability. Keep age signals separate from advertising, analytics and general player identifiers, and provide appropriate safeguards for unknown age.

Official starting points:
- Brazil: https://www.gov.br/anpd/pt-br/assuntos/eca-digital
- Brazil implementing decree: https://www.planalto.gov.br/ccivil_03/_ato2023-2026/2026/decreto/D12880.htm
- EU GDPR: https://eur-lex.europa.eu/eli/reg/2016/679/oj
- UK Online Safety Act: https://www.legislation.gov.uk/ukpga/2023/50/contents
- UK ICO Children's Code: https://ico.org.uk/for-organisations/uk-gdpr-guidance-and-resources/childrens-information/childrens-code-guidance-and-resources/

## Rollout review

This file and the public privacy page must be checked against the deployed services before publication. Final approval requires both project maintainers and a suitably qualified legal/privacy reviewer for each applicable jurisdiction.
