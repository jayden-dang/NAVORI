# Navori — Architecture overview

Navori is an AI-first personal cockpit for macOS (Apple Silicon): one menubar app whose logic lives in a Rust **Core** (kernel, modules, AI runtime) and whose UI is a Swift **Shell** (AppKit + SwiftUI), connected through UniFFI. It is built for its author, installable by other developers through Homebrew, runs locally, works with any cloud model the user chooses, and keeps a hardcore performance budget. First capabilities: Awake, Ports and GitHub pull requests — which are also the AI's first tools.

This file is the **entry point**: it maps where each part of the architecture is documented, keeps the principles' rationale and the index of locked decisions, and holds the **design seeds** for features not yet specified. When this file disagrees with a document listed below, that document wins.

## Document map

| Topic | Document |
|---|---|
| Vision, goals (GOAL-N), non-goals, scope | [`product/vision.md`](product/vision.md) |
| Milestones (MILE-N), roadmap items (ROAD-N), focus | [`roadmap/INDEX.md`](roadmap/INDEX.md) |
| Decisions of record (ADRs) | [`adr/`](adr/) |
| Architecture invariants (ARCH-N) | [`architecture/INDEX.md`](architecture/INDEX.md) |
| Components and how they relate | [`architecture/system.md`](architecture/system.md) |
| Processes, threads, lifecycle, timing | [`architecture/runtime.md`](architecture/runtime.md) |
| Stored data and consistency rules | [`architecture/data.md`](architecture/data.md) |
| External systems, contracts, failure modes | [`architecture/integrations.md`](architecture/integrations.md) |
| Where code goes | [`codebase/map.md`](codebase/map.md) |
| Code units and their public surface | [`codebase/modules.md`](codebase/modules.md) |
| Allowed and forbidden dependencies, third-party stack | [`codebase/dependencies.md`](codebase/dependencies.md) |
| Trust boundaries (TB-N), threats (THR-N), mitigations | [`security/threat-model.md`](security/threat-model.md) |
| Build, sign, install, rollback | [`ops/deployment.md`](ops/deployment.md) |
| Engineering standards, Definition of Done, performance method | [`standards/`](standards/INDEX.md) |
| Menubar UI (locked brief) | [`design/2026-11-01-menubar-ui-brief.md`](design/2026-11-01-menubar-ui-brief.md) |
| Glossary | [`../CONTEXT.md`](../CONTEXT.md) |
| Agent instructions, commands | [`../AGENTS.md`](../AGENTS.md), [`agents/project.md`](agents/project.md) |

## Design principles (rationale)

The binding rules are the ARCH-N invariants; this table records why they exist.

| # | Principle | Consequence |
|---|---|---|
| P1 | **AI-first, not an AI add-on** | The AI runtime is a core layer on par with the kernel, present from the start. Every module is born "AI-readable" (state can be described) and "AI-operable" (actions have a schema). |
| P2 | **Model-agnostic** | No dependence on a single provider. The internal message/tool/stream format is our own standard; providers are just adapters. The user picks a model per role and per conversation. |
| P3 | **Rust owns logic, Swift owns experience** | Rust: kernel, modules, AI, store, IPC, low-level system FFI. Swift: UI and the Apple frameworks tied to the app bundle / permissions (Accessibility, ScreenCaptureKit, Notifications, Login item, Hotkey). Swift contains **no** business logic. Exception: computer use reads and drives other apps through Accessibility, ScreenCaptureKit and SkyLight from `navori-macos`; Swift still owns the permission prompts (ADR 0009). |
| P4 | **Narrow FFI boundary** | Swift talks to Rust through ~6 functions + a few listeners. No chatty calls. Data crossing the boundary is snapshots/commands, not object graphs. |
| P5 | **Everything is a Module** | A feature = an independent module actor registered with the kernel. Modules do not import each other; they communicate via the Event Bus / Commands. |
| P6 | **Actions carry a schema + risk** | A single action surface serves UI, CLI, IPC, Navori AI, and MCP. `Read` / `Write` / `Destructive` decides whether the AI must ask permission. |
| P7 | **Every change is an Event** | The event journal in SQLite = the context timeline for the AI. |
| P8 | **See everything except secrets** | The AI may see the full context (apps, windows, text, screen) **except** sensitive data (API keys, SSH keys, tokens, passwords, private keys, `.env`…). All data leaving the machine passes through **one** gate (Context Gateway + Sensitive Data Shield), **fail-closed**. Navori's own secrets live only in the Keychain. `unsafe` only in `navori-macos` and `navori-ffi`. |
| P9 | **Frugal with resources & cost** | Idle CPU ≈ 0%. Adaptive polling. Proactive AI has a budget (tokens/day) and rate limits; cheap work uses a cheap model. |

## Roadmap

Milestones, their order, members and commitments live in [`roadmap/INDEX.md`](roadmap/INDEX.md) (MILE-N, ROAD-N); release names R0–R6 map to MILE-1 … MILE-7. Each release adds **one** complete capability per the Definition of Done; once done it is usable every day.

**Why AI is in R2 and not last**: AI is the core (P1). Placing it early lets the `Action` / `context()` / `Event` contracts be validated by Navori AI before Ports and GitHub are written — those two features are born as AI tools. Screen observation (screenshot + OCR) is in R2 because the Sensitive Data Shield is a prerequisite for releasing AI.

## Decisions locked

Hard-to-reverse decisions live as ADRs in `docs/adr/`; the rest are recorded where they bind.

| # | Decision | Recorded in |
|---|---|---|
| D1 | **UI**: Swift (AppKit + SwiftUI) Shell + Rust Core via UniFFI, in one process | [ADR 0001](adr/0001-rust-core-swift-shell-in-process.md) |
| D2 | **AI**: cloud models, any Provider, the user chooses a model per Model role | GOAL-5 in `docs/product/vision.md`; design in the AI runtime seed below |
| D3 | **GitHub**: OAuth App + Device Flow | [ADR 0002](adr/0002-github-oauth-app-device-flow.md) |
| D4 | **CPU**: Apple Silicon only (`aarch64-apple-darwin`) | Non-goals in `docs/product/vision.md` (cheap to reverse: build config only) |
| D5 | **Harness**: Claude Code, Grok Build, Codex via hooks; others auto-detected | The Awake seed below (hooks verified on the machine 2026-11-01) |
| D6 | **Running with the lid closed**: on the power adapter while a Lease is held, without root (amended 2026-11-06; was "not supported") | Non-goals in `docs/product/vision.md`; ROAD-41 |
| D7 | **AI observation scope**: everything except secrets | [ADR 0003](adr/0003-ai-sees-everything-except-secrets.md) |
| D8 | **App signing**: self-signed identity, no paid Apple Developer account yet | [ADR 0004](adr/0004-self-signed-code-signing.md) |
| D9 | **Menubar menu**: hand-built cascade on NSPanel with NSGlassEffectView (macOS 26), not NSMenu | [ADR 0006](adr/0006-menubar-menu-on-nspanel.md) |

## Design seeds (pending feature specs)

Design notes for capabilities that are planned but not yet specified. When a feature enters `design-solution`, its `docs/specs/<date>-<feature>/design.md` lifts the matching seed (and re-checks it against the invariants and the documents above), and the seed is deleted here. Seeds are inputs, not decisions of record.

### Seed: kernel contract extensions (R1–R2)

```rust
pub trait Module: Send + Sized + 'static {
    const ID: &'static str;                                  // "github"
    type Action: ModuleAction;
    type Config: ModuleConfig;
    /// Snapshot published to the UI / AI. Must be cheap to clone.
    type State: Serialize + Clone + Send + Sync + 'static;

    fn initial_state(&self) -> Self::State;

    /// What this Module asks of the status icon, read from its published State.
    /// Pure and cheap; the default asks for nothing.
    fn tray(_state: &Self::State) -> TrayContribution { TrayContribution::default() }

    fn run(&mut self, ctx: &mut ModuleCtx<Self>) -> impl Future<Output = anyhow::Result<()>> + Send;
}

pub trait ModuleAction: DeserializeOwned + JsonSchema + Send + 'static {
    fn risk(&self) -> Risk;                  // Read | Write | Destructive
    /// One entry per variant, in display order.
    fn catalog() -> Vec<ActionDoc>;
}

pub trait ModuleConfig: DeserializeOwned + Default + Send + Sync + 'static {
    fn check(&self) -> Result<(), &'static str> {
        Ok(())
    }
}
```

Three obligations for every module — these turn a "feature" into a "capability of the AI":

| Obligation | Used by |
|---|---|
| `Action` + JSON Schema + doc-comment written for the LLM + `Risk` | UI, CLI, IPC, **Navori AI tools**, **MCP server** |
| `context()` → `ContextFragment` | **Context Engine** (the AI knows your "now") |
| Domain `Event` with `summary()` | Journal, **Triggers**, **Memory** (the AI knows "what happened") |

`tray()` is on the trait (ICON): a Module says which glyph and whether attention it wants, and the Kernel composes one status icon state when the status is read (`docs/features/status-icon.md`). For MILE-3, `context()` stays an obligation. It is not on the trait yet.

- **Typed inside, erased at the edge**: the kernel wraps a module into `ModuleHandle { id, actions: Vec<ActionSpec>, dispatch, state_json, context }`. The Swift UI uses typed values (via UniFFI); AI/IPC/MCP use JSON `Envelope`s.
- **Supervisor**: a failing module is restarted with backoff, without crashing the app.
- **EventBus**: realtime `broadcast` + an `events` table (configurable retention).

### Seed: Rust ⇄ Swift surface for AI and platform services (R2)

#### Rust → Swift surface (complete)

```rust
#[uniffi::export]
impl NavoriCore {
    #[uniffi::constructor]
    fn start(cfg: CoreConfig, platform: Arc<dyn PlatformServices>) -> Result<Arc<Self>, CoreError>;

    fn subscribe(&self, listener: Arc<dyn SnapshotListener>);           // snapshots of all modules (enum ModuleSnapshot)
    async fn dispatch(&self, cmd: ModuleCommand) -> Result<ActionOutcome, CoreError>;   // typed command

    // AI
    fn ask(&self, req: AskRequest, sink: Arc<dyn AssistantStream>) -> Arc<TurnHandle>;  // stream + cancel()
    async fn resolve_approval(&self, id: String, decision: ApprovalDecision);

    fn set_panel_visible(&self, visible: bool);                          // throttles polling / coalescing
    fn system_did_wake(&self);                                           // one wake pulse after the Mac wakes
    async fn shutdown(&self);
}
```

#### Foreign traits (Swift implements, Rust calls)

```rust
#[uniffi::export(with_foreign)]
pub trait PlatformServices: Send + Sync {
    fn notify(&self, n: Notification);                         // UNUserNotificationCenter
    fn frontmost(&self) -> Option<FrontmostApp>;               // NSWorkspace + AX: app, window title, selected text, browser URL
    fn secure_input_active(&self) -> bool;                     // IsSecureEventInputEnabled — a password is being typed somewhere
    async fn capture_screen(&self, req: CaptureRequest) -> Result<Capture, PlatformError>;
        // ScreenCaptureKit, SCContentFilter pre-excludes denylisted app windows; plus OCR (Vision) text + bounding boxes
    async fn redact_image(&self, img: Vec<u8>, regions: Vec<Rect>) -> Result<Vec<u8>, PlatformError>; // blur (CoreImage)
    fn permissions(&self) -> PermissionStatus;                 // AX, Screen Recording, Notifications
}
pub trait SnapshotListener: Send + Sync { fn on_snapshot(&self, s: ModuleSnapshot); }
pub trait AssistantStream: Send + Sync { fn on_event(&self, e: AssistantEvent); }  // delta, tool call, approval request, done, error
```

#### Data types
- The Awake record (`AwakeView`) is declared in `navori-ffi` and sits on `CoreSnapshot`. `modules/awake` does not depend on UniFFI; FFI converts through the Module's Rust type. Swift gets a typed view without a hand-written mirror.
- `ModuleSnapshot` / `ModuleCommand` are aggregate enums, extended by one variant per new module.

#### Known risks & how they are handled
| Issue | Handling |
|---|---|
| UniFFI-generated async code is not yet fully `Sendable`-conformant on Swift 6 (uniffi-rs #2448) | The bindings live in a separate `NavoriCore` package with strict concurrency relaxed there; the app only touches `CoreStore` (`@MainActor`). |
| Callbacks arrive on a Rust thread | The listener adapter always does `Task { @MainActor in … }`; Rust has already coalesced. |
| Panic across FFI | UniFFI catches it and converts it to an error; add a `panic hook` that writes `tracing` + a crash marker. |
| Data copied across the boundary | Send only coalesced snapshots, not individual items; large lists are paginated. |
| Stack traces break at the boundary | Every command/turn carries a `trace_id`; both sides log the same ID (`os.Logger` ↔ `tracing`). |

### Seed: AI runtime (R2, R5, R6)

#### 6.1 `navori-llm` — internal standard + provider adapters

**Canonical format** (no provider's format is taken as the standard):

```rust
pub struct ChatRequest {
    pub model: ModelRef,                 // "anthropic/<model-id>", "openai/<model-id>", "deepseek/<model-id>"
    pub system: Vec<ContentPart>,
    pub messages: Vec<Message>,          // role + Vec<ContentPart>
    pub tools: Vec<ToolDef>,             // name, description, JSON Schema
    pub tool_choice: ToolChoice,
    pub reasoning: Option<ReasoningCfg>, // effort/budget — the adapter maps it to the provider's own parameters
    pub max_output: Option<u32>,
    pub cache_hints: Vec<CacheHint>,     // prompt-cache points (adapters that do not support them ignore them)
}
pub enum ContentPart { Text, Image, ToolUse, ToolResult, Reasoning /* opaque, kept as-is to be sent back */ }
pub enum StreamEvent { TextDelta, ReasoningDelta, ToolCallStart, ToolCallArgsDelta, ToolCallEnd, Usage, Stop(StopReason), Error }

#[async_trait]
pub trait LlmProvider: Send + Sync {
    fn id(&self) -> ProviderId;
    async fn list_models(&self) -> Result<Vec<ModelInfo>>;
    async fn stream(&self, req: ChatRequest, cancel: CancellationToken)
        -> Result<BoxStream<'static, Result<StreamEvent>>>;
    async fn embed(&self, _: EmbedRequest) -> Result<EmbedResponse> { Err(Unsupported) }
}
```

**Adapters by "protocol family"** — 4 adapters cover nearly every provider:

| Adapter | Provider |
|---|---|
| `anthropic` (Messages API) | Claude |
| `openai` (Responses API) | ChatGPT / OpenAI |
| `openai_compat` (Chat Completions) | DeepSeek, xAI Grok, OpenRouter, Groq, Mistral, Together, Ollama / LM Studio, **any custom endpoint** (base_url + key) |
| `gemini` | Google Gemini |

- Adapters are hand-written on `reqwest` + `eventsource-stream` rather than depending on a multi-provider library: AI-first means full control over each provider's specific features (prompt caching, reasoning, tool streaming, citations) — generic libraries usually lag behind. (`genai` may be used temporarily behind the trait to bootstrap quickly.)
- A shared **conformance test suite** for all adapters: the same set of scenarios (text, parallel tool calls, tool result, image, cancel mid-stream, 429/5xx errors) run against recorded SSE fixtures (`wiremock` + `insta`), and run live when a key is available (`cargo xtask llm-conformance`).

#### 6.2 `navori-models` — catalog & Model Router

- **Model catalog**: a built-in data file (`models.toml`: context window, capabilities `tools/vision/reasoning/caching`, input/output price) + refresh from `list_models()` + user-added models. Model IDs are **never hard-coded** in logic.
- **Roles** — the user assigns a model to each role in Settings:

| Role | Used for | Suggestion |
|---|---|---|
| `chat` | conversation, multi-step tool use | the strongest model the user picks |
| `fast` | classification, event summaries, proactive decisions | a cheap/fast model |
| `vision` | understanding screenshots | a model with vision |
| `embedding` | memory search | any provider with embeddings |

- Each conversation can switch models midway (history is in the canonical format, so switching provider works).
- **Fallback chain** on 429/5xx/quota exhausted (e.g. Claude → GPT → DeepSeek), configurable.
- **Usage & budget**: tokens + estimated cost per turn are recorded in SQLite; a daily limit for proactive tasks; shown in Settings.
- Each provider's API key is stored in the Keychain (`navori-secrets`), never in config.toml or logs.

#### 6.3 `navori-context` — Context Engine + Context Gateway

The **Context Engine** assembles the "current situation" on demand:
- `ContextFragment`s from every module (pending PRs, awake leases, running ports…).
- System signals through `PlatformServices` (Swift): foreground app, window title, selected text, browser URL, idle time, **screenshot** (on by default).
- The current project: from `navori-procs` (cwd of the active terminal/editor/harness → repo/worktree/branch).
- A recent timeline from the event journal; relevant memory from `navori-memory`.
- Merged within the **token budget** of the model in use, prioritized by relevance.

The **Context Gateway** — the *only* gate through which data may leave the machine. Policy: **see everything except secrets**. Inside it is the **Sensitive Data Shield** (`navori-redact`) with 4 layers, applied *before* sending to the cloud **and** before writing to the journal/memory:

| Layer | Mechanism |
|---|---|
| 1. Block at the source | **App denylist** (1Password, Bitwarden, Passwords, Keychain Access, banking apps — configurable) → no title/text/screenshot is taken; their windows are **excluded from the screenshot at capture time** (`SCContentFilter` excluding apps). **Secure Input** active (a password is being typed anywhere) → skip text + screenshot at that moment. Private/incognito windows → skipped. |
| 2. Block by path | The AI/tools **never read**: `~/.ssh/**`, `~/.gnupg/**`, `~/.aws/credentials`, `~/.config/gh/hosts.yml`, `~/.netrc`, `~/.docker/config.json`, `**/.env*`, `*.pem`, `*.key`, `*.p12`, `~/Library/Keychains/**`, harness credential files (`~/.claude/.credentials.json`, `~/.grok/auth.json`, `~/.codex/auth.json`…). The list is extensible. |
| 3. Detection in text content | A secret rule set (gitleaks-style): AWS, GitHub (`ghp_`, `gho_`, `github_pat_`), OpenAI, Anthropic, Slack, Stripe, Google, JWT, `PRIVATE KEY` blocks, connection strings with passwords, `KEY=…/TOKEN=…/PASSWORD=…`, high-entropy strings near sensitive keywords → replaced with `[REDACTED:github_token]`. |
| 4. Detection in images | Swift runs on-device OCR (Vision) → Rust runs the same layer-3 rule set on the OCR text → Swift **blurs** regions containing secrets (CoreImage) → only then is it sent. |

- **Fail-closed**: an OCR failure, a detector failure, or an app that cannot be identified → that portion is **not sent** (the rest is).
- **Audit log**: every request records the *kind* of data sent + provider + number of items redacted (never the content) → "what did the AI see about me today?".
- **Testing**: a fake-secret corpus (fixtures) + a "no token slips through" test run in CI; a regression test per rule.
- A switch to pause observation ("Pause AI vision" 15 minutes / 1 hour) right on the menubar.

#### 6.4 `navori-agent` — Agent Runtime

```
user/trigger ─► assemble context ─► Model Router ─► stream ─┬─► text → Swift
                                                             └─► tool call ─► Policy ─┬─ Read → run immediately
                                                                                      ├─ Write → per allowlist / ask
                                                                                      └─ Destructive → always ask (approval UI)
                                                                     ▼
                                                         CommandRouter / MCP client / built-in tool
                                                                     ▼
                                                              tool result ─► loop (step limit)
```

- **Tool registry** = the `ActionSpec`s of every module (automatic) + external MCP tools + built-ins (`memory.search`, `memory.remember`, `context.snapshot`, `web.fetch`, `open_url`).
- **Policy engine**: decides by `Risk` + a user-granted allowlist ("always allow `ports.kill` for project X"). Approval is an `AssistantEvent::ApprovalRequired` → Swift shows Approve/Deny buttons → `resolve_approval`.
- **Session**: stored in SQLite (canonical messages, tool calls, usage); cancellable midway; limits on steps & tokens per turn.
- **Surfaces**: chat in the menubar panel · **quick-ask** via a global hotkey (Spotlight-style NSPanel, carrying the current context) · `navori ask "…"` from the terminal · notifications with action buttons.

#### 6.5 `navori-memory`
- Everything written to memory has already passed through `navori-redact` — secrets never sit in the DB.
- **Facts/preferences** ("I use squash merge", "repo X is work") — the AI writes them via the `memory.remember` tool; the user views/deletes them in Settings.
- **Episodic**: the `fast` model summarizes the journal by hour/day → a short activity log.
- **Embeddings** for semantic search: the `VectorIndex` trait; starting with brute-force cosine in Rust (personal scale, tens of thousands of vectors is still fast), later switching to `sqlite-vec` (currently alpha) without changing the API.

#### 6.6 `navori-triggers` — Proactive
- Rules on the EventBus, for example: CI fails on my PR → summarize the error log · new review request → summarize the diff · a Claude Code session ends → notify + release awake · port conflict when starting a dev server.
- Pipeline: event → rule match (cheap, no AI call) → the `fast` model decides "is this worth interrupting" → if so, Navori AI does the work on the `chat` model → notification.
- Daily token **budget**, **quiet hours**, per-rule rate limit, a "don't notify me about this again" button.

#### 6.7 `navori-mcp` (`rmcp`)
- **MCP client**: the user adds MCP servers (Linear, Notion, filesystem…) → their tools enter the Tool Registry, defaulting to `Risk::Write` unless the user lowers it.
- **MCP server**: `navori mcp` (stdio) → Claude Code/Cursor can call Navori's tools ("which ports are running in this worktree?", "keep the machine awake until I'm done", "which PRs are waiting on me?"). Same policy engine as Navori AI.

### Seed: process model — `navori-procs` (R1, R3)

Awake, Ports and the Context Engine all need to know "which project does this process belong to, and who started it":
- **Snapshot** (from `navori-procs`, over `ProcessSource` in `navori-macos`; listed via `sysctl(KERN_PROC_ALL)`): pid, ppid, uid, exe, argv, cwd, start time.
- **resolve_location**: cwd → `.git` → repo root; `.git` is a file → **worktree** → `{project, worktree, branch}`.
- **harness_of**: recognizes Claude Code, Codex, and Grok, and the terminal host.
- **ProcWatcher**: `kqueue EVFILT_PROC/NOTE_EXIT` → learns of a process death immediately, no polling.

### Seed: Awake (R1)

Shipped behavior, commands, and events live in `docs/features/awake.md` and `docs/specs/2026-11-02-awake-leases/`. This seed is not a second action catalog.

- **Reconciler**: with ≥ 1 lease → hold at most one IOKit assertion per kind (`PreventUserIdleSystemSleep`, and `PreventUserIdleDisplaySleep` if a Lease asks for the display). Idempotent.
- A lease ends when: released · TTL expires · the process dies (kqueue). The assertion follows the process (the OS reclaims it when the app exits). Rows for process-owned Leases survive a restart, and on reopen the assertion returns if the process is still the same.

**Harness integration — verified on the machine (2026-11-01)**: all three harnesses have Claude Code-style hooks (JSON via stdin, with `hook_event_name` + session id + `cwd`). So **one** `navori hook` command serves all of them.

| Harness | Hook | acquire | release after grace | release immediately |
|---|---|---|---|---|
| Claude Code | `~/.claude/settings.json` | `UserPromptSubmit` | `Stop`, `StopFailure` | `SessionEnd` |
| Grok Build (`grok` 1.0.70) | `~/.grok/hooks/navori.json` | `UserPromptSubmit` | `Stop` (only when `reason == "end_turn"`), `StopCancelled`, `StopFailure` | `SessionEnd` |
| Codex (`codex-cli` 0.166) | `~/.codex/hooks.json` | `UserPromptSubmit` | `Stop`, `Interrupt` | `SessionEnd` |
| Any command | — | `navori awake run -- <cmd>` | — | when the command exits |
| Harness without hooks | — | auto-detect via harness_of | silent for N minutes | process dies |

Important details:
- **Harness identification**: `navori hook` sends its pid and ppid over IPC, and the Core walks the process tree while the request is open. Resolve the Harness on `UserPromptSubmit` and `Stop`, never on `SessionEnd` (1–1.5 s budgets; 2026-11-01 harness-hook research note). No arguments needed.
- **Duplicate-count protection**: Grok **also reads the hooks in `~/.claude/settings.json`** (Claude-compatibility mode) → the same event may arrive twice. Every lease operation is **idempotent per `(harness, session_id)`**, so duplicates do no harm.
- **"Stop" ≠ "work finished"**: a harness may still have background tasks (background bash, subagents, long-running tasks) after the turn ends. Before releasing the lease once the grace period elapses, Awake inspects the **harness's child process tree**; if a child is still running/consuming CPU, it extends the lease.
- **Installation**: Settings → Integrations has Install/Repair/Remove buttons per harness. Navori **merges** into the existing config file (backup before writing, never overwriting other hooks), and `navori doctor` checks whether the hook is still correct.
- **Safety net**: if a hook fails or does not run → kqueue (process death) + TTL still guarantee the lease cannot hang forever.

Out of scope (decided): staying awake with the lid closed on battery, and any root or helper process for it. On the power adapter, while a Lease is held, the lid stays closed without sleep (ROAD-41, amended 2026-11-06).

### Seed: Ports (R3)

- **Scan** (no `lsof`): pids of the current UID → socket fds → TCP `LISTEN` + port. Usually < 50 ms.
- **Enrich** via `navori-procs`: project · worktree · branch · harness · kind (dev server, database, docker).
- **Display**: Project → Worktree → Process → Ports.
- **Actions**: open `localhost:port` · copy · reveal in Finder · kill one / by worktree / project / all. Kill = `SIGTERM` to the process group → 3s → `SIGKILL`. Never touches other UIDs; a `protected` list is excluded from "kill all".
- Polling every 3s when the panel is open, 30s in the background.
- AI tools: `ports.list` (Read), `ports.kill{pid|worktree|project}` (**Destructive**), `ports.open` (Read).
  - *Amended 2026-11-04: the request shape is `ports.kill{selector, targets}`. The selector is `pid`, `worktree{project, worktree}` or `project` and only checks that the targets belong where the caller thinks; `targets` are the exact processes (pid and start time) and containers (id) to stop, and a stop never adds one. Navori re-reads the machine before it signals anything.*

### Seed: GitHub pull requests (R4)

- **Inbox**: Needs my review · My PRs · Involved. Per PR: repo, #, title, draft, review decision, CI rollup, mergeable, labels, updated.
- **Auth — OAuth App + Device Flow** (a long-term decision, not a temporary one; see ADR 0002 in `docs/adr/`). Scopes: `repo`, `read:org`, `notifications`, `workflow`. No client secret, no redirect server — the same way `gh auth login` works. Token → Keychain.
  - One account for now (multiple accounts are deferred, ROAD-26): tokens are still keyed by `account_id`, so adding a second account needs no migration.
  - Orgs with SAML SSO: guidance to authorize the token for the org inside the app; orgs that block OAuth Apps: send an approval request, or use a PAT (classic) as the fallback.
  - A `TokenProvider` trait (`OAuthDevice`, `ClassicPat`) reserves room for other methods without changing the module.
- **Data**: a single GraphQL request (3 aliased search queries) via `reqwest` + `graphql_client`; track rate-limit headers; SQLite cache → opening the app shows data immediately.
- **Fast, cheap change detection**: poll the **Notifications API** with `If-Modified-Since` and respect `X-Poll-Interval` (a 304 costs no quota) → only when there is a new notification does GraphQL run again. This is why an OAuth App is needed: per GitHub docs, the notifications endpoints only support classic-type tokens (OAuth/PAT classic), not GitHub App user tokens or fine-grained PATs.
- `domain::diff(prev, next) → Vec<GithubEvent>` → notification + journal + **Triggers**.
- AI tools: `github.inbox` (Read), `github.pr.get{diff, checks, comments}` (Read), `github.pr.summarize` (Read), `github.pr.comment` / `approve` (Write), `github.pr.merge` (**Destructive**), `github.checks.rerun` (Write).
- AI-first scenarios: "Summarize the 3 PRs waiting on me, which is the smallest?" · "Why did CI on PR #42 fail?" · a trigger that automatically summarizes CI failures.
