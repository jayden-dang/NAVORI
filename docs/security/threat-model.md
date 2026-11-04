# Threat model

Status: Approved
Date: 2026-11-01

## Purpose and boundary

The trust boundaries of Navori, the threats that cross them, and how each is mitigated — for a single-user, local-first macOS app that observes almost everything the user does and can act on the machine, on GitHub and on the user's Connected accounts through an AI agent. The asset that matters most is the user's secrets and machine state; the second is the integrity of actions taken on the user's behalf.

In scope: the app, its CLI, its local storage, and every path data or commands take in or out (`docs/architecture/integrations.md`). Out of scope: macOS itself, the security of the cloud providers, GitHub and the services behind Connected accounts, physical access to an unlocked Mac, and anything already running as the user with full privileges (see accepted risks). Mitigations marked *(planned, Rn)* arrive with that release and must exist before it ships.

## Trust boundaries

- **TB-1** The machine edge: any data that leaves the Mac for a cloud LLM provider or GitHub.
- **TB-2** The local IPC socket between Navori and every other process on the Mac.
- **TB-3** The AI agent's output, which is untrusted text that may request Actions on the machine and on GitHub.
- **TB-4** Observed content entering the AI's context: screen, window titles, selected text, files, PR and issue text, hook payloads and tool results.
- **TB-5** External MCP servers whose tools Navori's agent may call.
- **TB-6** Navori's data at rest on disk: database, Journal, backups, logs, config and session marker.
- **TB-7** Configuration files of other tools that Navori writes into, such as the harness hook files.
- **TB-8** The services behind Connected accounts: user data leaving the Mac for them, and content arriving from them that other people wrote.

## Threats

- **THR-1** A secret (API key, token, password, private key, `.env` value) reaches an LLM provider inside context sent across TB-1.
- **THR-2** Instructions hidden in observed content (TB-4) steer Navori AI into a harmful Action, such as stopping processes, merging a PR or exfiltrating data.
- **THR-3** A secret is written in plain text to Navori's data at rest (TB-6) — Journal, memory, logs, backups or panic text.
- **THR-4** Another local user or process drives Navori over the socket (TB-2) to read its Events or trigger Actions.
- **THR-5** Navori stops the wrong process — another user's, a system service or a protected database — on a user or agent request.
- **THR-6** A forged or duplicated hook payload keeps the Mac awake indefinitely or releases a Lease too early.
- **THR-7** Navori's own credentials (GitHub token, provider API keys) are stolen from disk, config or logs.
- **THR-8** A malicious or compromised external MCP server (TB-5) returns poisoned tool results or advertises a harmful tool.
- **THR-9** Installing or repairing hooks corrupts or removes the user's existing configuration in another tool (TB-7).
- **THR-10** Context leaves the machine through a path other than the Context Gateway, bypassing redaction and the audit log.
- **THR-11** A stranger's mail or message arriving through TB-8 carries hidden instructions that reach Navori AI with no action by the user, steering it to read other private content or act on the user's behalf.
- **THR-12** Navori AI sends mail or a message to another person as the user — wrong recipient, wrong content or exfiltrated data — which cannot be taken back.

## Mitigations (prose)

- **THR-1 at TB-1** — the Sensitive Data Shield runs four layers before anything crosses TB-1 *(planned, R2; the text detector and path denylist are built)*: denylisted apps and private windows are never captured and are cut out of screenshots at capture time; nothing is captured while Secure Input is active; credential paths are never read; secret patterns are masked in text and blurred in images after on-device OCR. It fails closed: if OCR, detection or app identification fails, that part is not sent. The Ports marker read (names only, wiped, never copied out of the reader) adds no environment content to anything sent. An audit log records what kinds of data went to which provider, and a menubar switch pauses observation.
- **THR-2 at TB-3/TB-4** — every Action carries a Risk; `Destructive` Actions always become a Decision in Needs you and never run without the user's explicit approval (ARCH-5); `Write` Actions run only under a user-granted allowlist or after asking *(planned, R2)*. Observed content is passed to the model as data, not as instructions, and Navori AI has a step limit per turn.
- **THR-2 and Ports** — `ports.list` output (container names, compose working-directory labels, branch names, paths) is observed content (TB-4) that can steer Navori AI (THR-2), so the AI-facing path must treat it as untrusted.
- **THR-3 at TB-6** — Events, the Journal, memory and panic text pass the Shield before they are written (ARCH-6, built for Events, the Journal and panic text); logs never contain prompt content or secrets unless debug logging is on. The Ports marker read matches three exact environment names inside the argument scratch, returns only which one matched, and zeroes the scratch; no environment value or unmatched name is copied, logged or published. The Ports Docker read is a local read-only client of the Docker socket: it sends only the request that lists running containers, drops every field but id, name, published ports and the compose working-directory label, zeroes its response buffer, and its errors carry fixed text, so a container's working directory, labels and commands reach no log, Event, Journal or panic text.
- **THR-4 at TB-2** — the socket lives in a `0700` directory, the socket file is `0600`, and every connection's peer UID must equal the owner's (built); nothing listens on the network (ARCH-11).
- **THR-5** — process actions only target the current user's UID, a configurable protected list is excluded from "stop all", and stopping is an approved `Destructive` Action *(planned, R3)*.
- **THR-6** — Lease operations are idempotent per harness session; every Lease ends when the harness process exits (kqueue) or its TTL expires, even if a hook never fires *(planned, R1)*.
- **THR-7** — Navori's secrets live only in the macOS Keychain, never in config, the database or logs (ARCH-8, planned R2/R4); token-shaped strings are also caught by the Shield's patterns.
- **THR-8 at TB-5** — tools from external MCP servers default to `Risk::Write` and go through the same approval policy; their descriptions and results are treated as untrusted observed content (THR-2) *(planned, R6)*.
- **THR-9 at TB-7** — hook installation backs up the target file first and merges only Navori's entries, never removing others; `navori doctor` verifies them *(planned, R1)*.
- **THR-10 at TB-1** — data may leave only through the Context Gateway or a module's declared API client (ARCH-7); `ai/context` and `ai/memory` must depend on `navori-redact`, and feature Modules cannot import the AI crates (`docs/codebase/dependencies.md`).
- **THR-11 at TB-8** — content from a Connected account is untrusted observed content: it passes the Shield and reaches the model as data under the THR-2 rules, and a turn started by incoming content cannot run a `Write` or `Destructive` Action without asking, even when that Action is allowlisted *(planned, before the first such service ships)*.
- **THR-12 at TB-8** — an Action that sends content to another person is `Destructive`, so it always becomes a Decision showing the recipient and the content (ARCH-5). An MCP tool defaults to `Write` (THR-8), so the user raises its sending tools to `Destructive` when adding the server *(planned, before the first such service ships)*.
- **Connected account credentials** — they follow THR-7 (ARCH-8), request the narrowest scope the service offers, and can be revoked from Settings *(planned, before the first such service ships)*.
- **Supply chain** — `Cargo.lock` is committed, dependencies are declared once and updated deliberately (`cargo xtask outdated`), and the release is signed with a stable local identity.

**Accepted risks** (single user, local-first):
- Processes already running as the user are trusted on TB-2; malware with the user's privileges can do anything Navori can, with or without Navori.
- The user's chosen LLM providers receive redacted context by design; their retention and training policies are the provider's, chosen by the user.
- Data at rest is protected by macOS account security and disk encryption (FileVault), not by Navori-level encryption.

## Provisional IDs

IDs that appear only in `.skills/system-docs/security/threat-model/proposal.md` are provisional, not definitions. TB-N and THR-N in this Approved file are immutable; retire one by strikethrough with a reason.
