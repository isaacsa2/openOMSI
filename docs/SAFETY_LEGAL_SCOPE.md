# Multiplayer safety and legal-scope review (working template)

**Status: requires operator and legal sign-off.** This checklist is neither legal advice nor a statement that openOMSI is compliant or non-compliant in any jurisdiction. The service's operator must provide evidence, not just check a box.

## 1. Identify each service before assigning duties

Review and sign off **separately** for:

1. Offline game executable and distributed source code.
2. Player-hosted LAN sessions (user runs the host and controls access).
3. Player-operated dedicated servers and any separate hosting companies.
4. Project-operated discovery, status, listing, relays, update and counter endpoints.
5. Project-run official community servers (if any), Discord communities, and application forms.

For each, record: operator/contact, who decides access, country/jurisdiction connections, data categories, processors, audience, exposure to minors, moderation controls, and the evidence for each answer. Do **not** assume the project is the provider of every player-hosted game. Do **not** assume any project-operated online service is exempt solely because the executable is open source.

## 2. Risk and safeguard matrix

| Hazard or obligation | Where it may arise | Existing or proposed safeguards | Gap / action to validate |
| --- | --- | --- | --- |
| Harassment, spam, abuse in text chat | LAN and dedicated | Client filter, rate limits, kick/ban; fork PR #376 proposes host enforcement | Confirm host-side checks and ability to report abusive behaviour |
| Unwanted contact via text | LAN and dedicated | Fork PR #378 proposes session-local per-player text mute | Confirm client UI, translated strings, reconnection semantics |
| Voice harassment | LAN and dedicated | Voice off by default, personal mute, moderator voice mute | Validate deployment behaviour and dispute/report route |
| Threats, child grooming, harmful content | Any user-to-user communication; mods and custom content | Moderation commands and profanity filtering | Profanity filtering does not reliably identify these risks; define reporting, response/escalation and relevant child protections |
| Mod and repaint content | User-hosted mod transfers | File/path/hash/size validation | Moderation of offensive **content** is distinct from malware/file validation |
| Personal-data exposure | Logs, counter, bans, server list, creator forms | Existing privacy page; fork PR #377 inventories flows | Verify notice, lawful basis, access controls, recipients, actual retention and rights request channel |
| Moderation abuse and mistaken bans | Player and project communities | Admin commands with reasons/limits | Define independent complaint review, appeal, evidence retention and audit access |
| Children using online chat | Any game service likely accessed by minors | Voice disabled by default, some communication controls | Complete service-specific assessment; identify additional legally required defaults/supervision/age signals, if applicable |
| Responsibility for community servers | User-operated and project-endorsed lists | Separate host role and server controls | Define eligibility criteria, incident contact and what the project controls |

## 3. Report and appeal workflow design (not implemented)

Before enabling a report backend, establish which **specific server operator** receives reports, who can read the minimum necessary evidence, how long it is held, when it is removed, how the accused user can appeal and what to do for urgent harm. Avoid recording ordinary voice conversations or keeping all text indefinitely.

Recommended functional requirements for a future independent PR:
- A client can report a player, session or inappropriate mod, selecting a category and optionally including a short description.
- The UI identifies the operator who will receive the report. Independent server reports do not silently go to the openOMSI developers.
- The server validates report sender, session context, payload size and rate limits; prevent report spam and impersonation.
- Evidence is minimised and access-controlled; logging must not expose private report payloads.
- Operators can resolve reports with a recorded rationale and give a safe response path.
- There is a separate appeal route and documented retention/deletion rules.
- Audit/test abuse cases before enabling reports in a release.

## 4. Age assurance decision

**Out of scope:** no collection of birth dates, identity documents, selfies, facial data, government identifiers, central player accounts, parental settings, or age-based chat gating in these current PRs.

Do not conclude that age assurance is categorically unnecessary. Determine applicability per service and risk, then use the least intrusive robust solution supported by the regulator; an unsupported DOB textbox cannot stand in for reliable verification where required.

- Brazil: ANPD says not every video game needs age checking; ECA Digital imposes protections for children and restrictions for certain types of content and interactions.
- UK: determine whether a project-operated feature is an in-scope user-to-user service and whether child access/risk duties and highly effective age assurance apply. ICO permits applying relevant children's privacy protections universally where age is uncertain, **but this is not a general substitute for all online safety duties**.
- EU: determine GDPR controller obligations and whether any specific platform functions fall within DSA duties; do not assume all private game chat is a public online platform.

## 5. Regulatory and documentation sources

- Brazil ANPD ECA Digital: https://www.gov.br/anpd/pt-br/assuntos/eca-digital
- Brazil Lei 15.211/2025: https://www.planalto.gov.br/ccivil_03/_ato2023-2026/2025/lei/l15211.htm
- Brazil Lei 14.852/2024: https://www.planalto.gov.br/ccivil_03/_ato2023-2026/2024/lei/l14852.htm
- EU GDPR: https://eur-lex.europa.eu/eli/reg/2016/679/oj
- EU Digital Services Act: https://eur-lex.europa.eu/eli/reg/2022/2065/oj
- UK Online Safety Act 2023, sections 35 and 226: https://www.legislation.gov.uk/ukpga/2023/50/contents
- Ofcom gaming guidance: https://www.ofcom.org.uk/online-safety/illegal-and-harmful-content/the-online-safety-act-and-gaming-know-the-risks-know-the-rules-know-how-to-comply
- UK ICO Children's Code: https://ico.org.uk/for-organisations/uk-gdpr-guidance-and-resources/childrens-information/childrens-code-guidance-and-resources/

## 6. Approval record (to complete)

| Service | Operator | Jurisdiction assessment | Risk owner | Legal/privacy reviewer | Controls verified | Decision/date |
| --- | --- | --- | --- | --- | --- | --- |
| Offline game | TBD | TBD | TBD | TBD | TBD | TBD |
| Project-operated discovery/listing | TBD | TBD | TBD | TBD | TBD | TBD |
| Player-hosted LAN | Individual operators | Per service | TBD | TBD | TBD | TBD |
| Dedicated/community servers | Per operator | Per service | TBD | TBD | TBD | TBD |
| Creator form | Form owner to identify | TBD | TBD | TBD | TBD | TBD |

Do not merge a public claim of complete compliance on the basis of this template. It records questions to be answered and evidence to be produced.
