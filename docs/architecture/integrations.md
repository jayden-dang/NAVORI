# Architecture domain: Integrations

Status: Approved
Date: 2026-11-01

## Purpose and boundary

Every system outside the Navori process that Navori talks to, listens to or writes into: which direction data flows, the contract Navori relies on, and what happens when that system misbehaves. Data may leave the machine only through the LLM Context Gateway or a module's declared API client (ARCH-7); everything else here is local to the Mac.

Not covered here: the internal Rust ⇄ Swift boundary (`system.md`, `runtime.md`), stored data (`data.md`), threat analysis (`docs/security/`), per-feature behaviour (each feature's design).

## External systems

**Built (R0)**

| System | Direction | Contract |
|---|---|---|
| `navori` CLI and local scripts | in | JSON-RPC 2.0, one JSON object per line, over the user-private Unix socket; methods `core.status`, `core.actions`, `core.dispatch`, `core.events`, `core.doctor`; requests ≤ 1 MiB; peer UID must match. |
| macOS Login Items (`SMAppService`) | out | `SMAppService.mainApp` register/unregister; fallback: a user LaunchAgent `~/Library/LaunchAgents/<bundle-id>.login.plist` that opens the app at login. |
| IOKit power management | out | `IOPMAssertionCreateWithName` with `PreventUserIdleSystemSleep` (+ `PreventUserIdleDisplaySleep` when a Lease asks for the display); at most one assertion per kind; released by the OS if the process dies. |

**Planned**

| System | Direction | Contract |
|---|---|---|
| Harness hooks — Claude Code, Codex, Grok Build (R1) | in (events) · out (hook install) | Each harness runs `navori hook` on lifecycle events with JSON on stdin (`hook_event_name`, session id, `cwd`). Files: `~/.claude/settings.json`, `~/.codex/hooks.json`, `~/.grok/hooks/navori.json`. Events: `UserPromptSubmit` → acquire a Lease; `Stop` (Grok: only `reason == "end_turn"`; also `StopCancelled`, `StopFailure`) → release after grace; `SessionEnd` → release now. Navori merges its entries into these files (backup first, never removing other hooks). |
| BSD process APIs — libproc, `sysctl`, kqueue, signals (R1, R3) | in · out | Read the process table (pid, ppid, uid, exe, argv, cwd), listening TCP sockets and process-exit events (`EVFILT_PROC/NOTE_EXIT`); stop processes with `SIGTERM` to the process group, then `SIGKILL` after 3 s. Only processes of the current UID. |
| Cloud LLM providers (R2) | out | Four adapter families over HTTPS + SSE: `anthropic` (Messages API), `openai` (Responses API), `openai_compat` (Chat Completions — DeepSeek, xAI, OpenRouter, Groq, Mistral, Ollama/LM Studio, custom endpoints), `gemini`. One API key per provider, in the Keychain. Every request passes the Context Gateway (redaction, audit). |
| macOS frameworks via PlatformServices (R2) | in · out | `UserNotifications` (out), `NSWorkspace` + Accessibility (frontmost app, window title, selected text), `ScreenCaptureKit` (screen capture excluding denylisted apps), `Vision` (on-device OCR), `CoreImage` (blur secrets). Each needs its macOS permission, requested on first use. |
| GitHub (R4) | both | OAuth App with device flow (no client secret, no redirect server); scopes `repo`, `read:org`, `notifications`, `workflow`; token in the Keychain. Change detection: Notifications API with `If-Modified-Since`, honouring `X-Poll-Interval` (a 304 costs no quota; classic tokens only). Data: one GraphQL request with aliased searches (review requested, mine, involved); REST for approve, merge and re-run. |
| MCP (R6) | both | Server: `navori mcp` over stdio exposes module Actions as tools to Claude Code, Codex and Grok, under the same Risk policy. Client: user-added MCP servers contribute tools, defaulting to `Risk::Write`. |
| Services behind Connected accounts — Gmail, Slack, Telegram… (not scheduled, ADR 0005) | both | Decided per service in its own design: the MCP client (R6) first, a native Module with a declared API client only when the service must be counted in the menubar or trigger proactively. Credentials in the Keychain, keyed by service and account, with room for a refresh token and expiry. Incoming content is untrusted (THR-11); sending to another person is `Destructive` (THR-12). |
| Docker (R3) | in | Ports owned by `com.docker.backend` are labelled `docker` from the process table; container attribution through the Docker API is decided in the R3 design. |

## Failure modes

- **Another Navori already running (Built):** a live socket for the same Profile refuses start-up with a clear error; a stale socket file from a crash is replaced.
- **SMAppService refuses the self-signed app (Built):** launch at login falls back to the LaunchAgent; Settings says which one is active.
- **A hook is missing, broken or never fires (R1):** the Lease still ends — the harness process exiting (kqueue) or the Lease TTL releases it; `navori doctor` and Settings → Integrations report the hook and offer Repair.
- **The same hook event arrives twice (R1):** Grok also reads Claude's hooks; every Lease operation is idempotent per `(harness, session_id)`.
- **`Stop` while background work continues (R1):** before releasing after the grace period, Awake checks the harness's child processes and extends the Lease if work is still running.
- **IOKit or process APIs fail (R1, R3):** the error is shown on the Awake or Ports row and in `doctor`; Navori never touches processes of another UID or protected processes in "stop all".
- **LLM provider error, rate limit or quota (R2):** the Model Router falls back along the configured chain; a turn can be cancelled mid-stream; if redaction or OCR fails, that part of the context is not sent (fail-closed).
- **GitHub offline or rate-limited (R4):** the menu shows cached data with its age ("offline · 5 min ago") and retries after the server's poll interval; signed out → a single "Sign in to GitHub…" item; an org with SAML SSO or OAuth-app restrictions → in-app guidance, or a classic PAT as the fallback.
- **An external MCP server is unavailable (R6):** handling is decided in the R6 design.

## Not the invariant spine

Integrations narrative does **not** redefine ARCH-N invariants. It cites the ones that govern external contact (ARCH-5, ARCH-7, ARCH-8, ARCH-11, ARCH-17, ARCH-18); when this file and `docs/architecture/INDEX.md` disagree, the spine wins.
