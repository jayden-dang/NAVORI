# NAVORI — Architecture

> macOS-only menubar app, runs locally. **AI-first personal cockpit**: an AI assistant that understands what you are doing, knows all your personal tools (GitHub, machine, ports, agentic harnesses…) and acts on your behalf — using the **strongest cloud model, from any provider you choose** (Claude, ChatGPT, DeepSeek, Gemini, Grok, OpenRouter…).
>
> Shape: **Rust core** (logic, AI runtime, modules) + **Swift shell** (AppKit + SwiftUI) connected via **UniFFI**. **Apple Silicon only.**
> First features: **Awake (caffeine)**, **Ports**, **GitHub PRs** — which are also the AI's first set of *tools*.
> **No MVP**: every release is a complete, production-ready feature, usable immediately (see §14).
> **Audience**: a single user (the author) now; open source later, where other developers install it themselves via `brew install --cask` or a download page (requires Developer ID + notarization at that time).

> **Document map.** The product north star is [`docs/product/vision.md`](product/vision.md) (goals GOAL-1..GOAL-10). The binding architecture invariants are [`docs/architecture/INDEX.md`](architecture/INDEX.md) (ARCH-1..ARCH-18). Engineering standards (including the Definition of Done and the performance method) are in [`docs/standards/`](standards/). The glossary is [`CONTEXT.md`](../CONTEXT.md). The menubar UI is [`docs/design/2026-11-01-menubar-ui-brief.md`](design/2026-11-01-menubar-ui-brief.md).
> This file is the detailed system design narrative and rationale. When it disagrees with those files, those files win.

---

## 1. Design principles

The binding rules are the ARCH-N invariants in `docs/architecture/INDEX.md`; this table is the rationale behind them.

| # | Principle | Consequence |
|---|---|---|
| P1 | **AI-first, not an AI add-on** | The AI runtime is a core layer on par with the kernel, present from the start. Every module is born "AI-readable" (state can be described) and "AI-operable" (actions have a schema). |
| P2 | **Model-agnostic** | No dependence on a single provider. The internal message/tool/stream format is our own standard; providers are just adapters. The user picks a model per role and per conversation. |
| P3 | **Rust owns logic, Swift owns experience** | Rust: kernel, modules, AI, store, IPC, low-level system FFI. Swift: UI and the Apple frameworks tied to the app bundle / permissions (Accessibility, ScreenCaptureKit, Notifications, Login item, Hotkey). Swift contains **no** business logic. |
| P4 | **Narrow FFI boundary** | Swift talks to Rust through ~6 functions + a few listeners. No chatty calls. Data crossing the boundary is snapshots/commands, not object graphs. |
| P5 | **Everything is a Module** | A feature = an independent module actor registered with the kernel. Modules do not import each other; they communicate via the Event Bus / Commands. |
| P6 | **Actions carry a schema + risk** | A single action surface serves UI, CLI, IPC, the AI agent, and MCP. `Read` / `Write` / `Destructive` decides whether the AI must ask permission. |
| P7 | **Every change is an Event** | The event journal in SQLite = the context timeline for the AI. |
| P8 | **See everything except secrets** | The AI may see the full context (apps, windows, text, screen) **except** sensitive data (API keys, SSH keys, tokens, passwords, private keys, `.env`…). All data leaving the machine passes through **one** gate (Context Gateway + Sensitive Data Shield), **fail-closed**. Navori's own secrets live only in the Keychain. `unsafe` only in `navori-macos` and `navori-ffi`. |
| P9 | **Frugal with resources & cost** | Idle CPU ≈ 0%. Adaptive polling. Proactive AI has a budget (tokens/day) and rate limits; cheap work uses a cheap model. |

---

## 2. Runtime overview

```
┌──────────────────────────────────────── Navori.app (1 process) ─────────────────────────────────────────┐
│                                                                                                         │
│  SWIFT SHELL (main thread)                                                                              │
│   ├─ StatusBar      NSStatusItem + NSMenu/NSPanel (AppKit) hosting SwiftUI views                        │
│   ├─ Features/      Awake · Ports · GitHub · Assistant (chat, quick-ask, approvals)                     │
│   ├─ CoreStore      @Observable, receives snapshots from Rust, hops to @MainActor                       │
│   └─ Platform/      implements foreign traits: Notifier · AXObserver · ScreenCapture · LoginItem · HotKey │
│                          ▲ snapshots / stream deltas          │ dispatch / ask / approve               │
│ ═════════════════════════╪════════════ UniFFI (navori-ffi) ══╪═════════════════════════════════════════ │
│                          │                                    ▼                                        │
│  RUST CORE (tokio runtime)                                                                              │
│   ┌──────────────────────────────── Kernel ──────────────────────────────────┐                         │
│   │ ModuleRegistry · Supervisor · CommandRouter · EventBus(+journal)          │                         │
│   │ Scheduler · Store · Secrets · Config                                      │                         │
│   └───────────────────────────────────────────────────────────────────────────┘                         │
│         │                         │                                   │                                │
│   ┌─────┴──── Modules ─────┐   ┌──┴────────────── AI Runtime ─────────┴───────────────────────┐        │
│   │ awake · ports · github │   │ Context Engine ─► Context Gateway (redact/consent) ─┐         │        │
│   │ (later: calendar,      │◄──┤ Agent Runtime (loop, tools, policy, approvals)  ◄───┤         │        │
│   │  notes, clipboard,     │   │ Model Router (roles, fallback, budget)          ────┤         │        │
│   │  mail, …)              │   │ LLM Providers: anthropic · openai · openai-compat · gemini   │        │
│   └────────────────────────┘   │ Memory (facts, episodes, embeddings) · Triggers (proactive)  │        │
│                                │ MCP client (uses external tools) · MCP server (exposes Navori tools) │
│                                └───────────────────────────────────────────────────────────────┘        │
│   IPC server (Unix socket, JSON-RPC 2.0) ◄─────────────────────────────┐                                │
└────────────────────────────────────────────────────────────────────────┼────────────────────────────────┘
                                                                         │
                 `navori` CLI ◄── Claude Code hooks · Codex notify · scripts · `navori ask "…"`
                 MCP (stdio via `navori mcp`) ◄── Claude Code / Cursor / other agents use Navori's tools
                 Cloud LLM APIs ◄── egress only from LLM Providers, through the Context Gateway
```

- **One process** contains the Swift shell + Rust core. The **CLI** `navori` is a separate Rust binary inside the bundle (`Navori.app/Contents/Helpers/navori`), talking to the app over the socket.
- The Rust core is initialized **once** at app launch (`NavoriCore::start`) and owns its own tokio runtime; Swift does not know tokio exists.

### Menubar UI
Design locked (2026-11-01): [`docs/design/2026-11-01-menubar-ui-brief.md`](design/2026-11-01-menubar-ui-brief.md) — NSMenu + cascading submenus; level 0 = Ask → **Needs you** (the decision inbox shared by the user and the AI: Decision / To-do / Update) → Awake / Ports / Pull Requests, one line each. Each feature updates its own surface in the brief when it ships.

### Concurrency
- **Swift**: the main thread for AppKit; every callback from Rust hops to `@MainActor` in **one** class, `CoreStore` — nothing else touches the bindings.
- **Rust**: each module/agent session is an **actor** (tokio task + `mpsc` inbox + `watch` snapshot + `broadcast` events). `CancellationToken` for shutdown/cancel (including cancelling an AI turn that is streaming).
- **Coalescing**: snapshots sent to Swift are coalesced (≤ 30/s, only while the panel is visible); AI tokens are batched in ~33 ms frames. The main thread is never flooded.

---

## 3. Repository structure

```
NAVORI/
├── Cargo.toml                    # [workspace], [workspace.dependencies], [workspace.lints]
├── rust-toolchain.toml           # stable, edition 2024, target aarch64-apple-darwin
├── .cargo/config.toml            # alias `cargo xtask …`
│
├── crates/                       # ── foundation ──
│   ├── kernel/                   # navori-kernel: Module trait, registry, router, bus, scheduler, supervisor
│   ├── protocol/                 # navori-protocol: Envelope, Event, ActionSpec, Risk, ContextFragment
│   ├── macos/                    # navori-macos: low-level unsafe FFI (IOKit power, libproc, kqueue, sysctl)
│   ├── procs/                    # navori-procs: process graph, project/worktree resolver, HarnessCatalog
│   ├── store/                    # navori-store: SQLite WAL, per-module migrations, KV, event journal
│   ├── secrets/                  # navori-secrets: Keychain + file backend (dev)
│   ├── config/                   # navori-config: typed config.toml, hot-reload
│   ├── ipc/                      # navori-ipc: Unix socket JSON-RPC server/client
│   ├── redact/                   # navori-redact: Sensitive Data Shield — shared secret detector (context, memory, logs)
│   └── ffi/                      # navori-ffi: the ONLY UniFFI layer Swift sees; the app's composition root
│
├── ai/                           # ── AI runtime (core, not an add-on) ──
│   ├── llm/                      # navori-llm: canonical types + LlmProvider trait + adapters
│   ├── models/                   # navori-models: model catalog, capabilities, pricing, Model Router
│   ├── context/                  # navori-context: Context Engine + Context Gateway (redaction, consent)
│   ├── agent/                    # navori-agent: agent loop, tool registry, policy, approvals, sessions
│   ├── memory/                   # navori-memory: facts, episodic summaries, embeddings, VectorIndex
│   ├── triggers/                 # navori-triggers: proactive rules on the EventBus, budget
│   └── mcp/                      # navori-mcp: MCP client (external tools) + MCP server (Navori tools)
│
├── modules/                      # ── features, one crate each ──
│   ├── awake/                    # navori-mod-awake
│   ├── ports/                    # navori-mod-ports
│   └── github/                   # navori-mod-github
│
├── apps/
│   ├── cli/                      # `navori` binary (Rust): hook, awake, ports, ask, mcp, doctor
│   └── macos/                    # Swift shell
│       ├── Package.swift         #   SwiftPM; the .app is assembled + signed by `cargo xtask bundle`
│       ├── Navori/
│       │   ├── App/              #   AppDelegate, accessory policy, lifecycle
│       │   ├── StatusBar/        #   NSStatusItem, NSMenu, NSPanel controllers
│       │   ├── Features/         #   Awake/ Ports/ GitHub/ Assistant/ Settings/ (SwiftUI)
│       │   ├── Core/             #   CoreStore (@Observable), listener adapters
│       │   ├── Platform/         #   foreign-trait impls: Notifier, AX, ScreenCapture, LoginItem, HotKey
│       │   └── main.swift        #   NSApplication + accessory policy (Info.plist generated by xtask)
│       └── Generated/            #   GENERATED (gitignored): NavoriCore/ (Swift bindings) + NavoriCoreFFI.xcframework
│
├── integrations/
│   ├── claude-code/              # hooks snippet + MCP config
│   └── codex/                    # notify config
├── xtask/                        # build-core, gen-bindings, bundle, sign, check-deps
└── docs/
    ├── ARCHITECTURE.md
    └── adr/
```

### Dependency rules (checked by `cargo xtask check-deps`)

```
crates/ffi ─► ai/*, modules/*, crates/*         (the app's composition root)
apps/cli   ─► crates/ipc, crates/protocol       (thin CLI, talks only over the socket)
ai/agent   ─► ai/{llm,models,context,memory,mcp}, kernel, protocol
modules/*  ─► kernel, protocol, procs, store, secrets, config   (+ macos via traits)
procs      ─► macos
ai/context, ai/memory, crates/store (journal) ─► redact     (everything written to disk or leaving the machine goes through redact)

FORBIDDEN: module ─► module · module ─► ai/* · ai/* ─► modules/* (the AI sees modules only through ActionSpec/ContextFragment)
           any crate other than `macos`, `ffi` using `unsafe`
```

Key point: **the AI imports no module**. It only sees the list of `ActionSpec`s and `ContextFragment`s provided by the kernel → adding a new module gives the AI new tools automatically.

### Layout inside a module

```
modules/github/src/
├── lib.rs        # impl Module, manifest
├── actor.rs      # select! { inbox, ticks, bus, shutdown }
├── action.rs     # enum Action (serde + JsonSchema + descriptions written for the LLM) + Risk
├── state.rs      # Snapshot (Serialize; derives uniffi::Record behind feature "ffi")
├── context.rs    # State → ContextFragment (concise summary for the AI)
├── event.rs      # domain events (with a human-readable sentence `summary()`)
├── domain/       # pure logic, heavily unit-tested
└── infra/        # IO: http, auth, SQLite repo
```

---

## 4. Kernel: the module contract (AI-first)

```rust
pub trait Module: Send + 'static {
    const ID: &'static str;                                  // "github"
    type Action: DeserializeOwned + JsonSchema + ActionMeta + Send;
    type State:  Serialize + Clone + Send + Sync + 'static;
    type Config: DeserializeOwned + Default + Send + Sync + 'static;

    /// AI-readable: the current state, concise, with sensitive data filtered out.
    fn context(state: &Self::State) -> ContextFragment;
    /// Badge / icon hint for the menubar.
    fn tray(state: &Self::State) -> TrayContribution { TrayContribution::none() }

    async fn run(self, ctx: ModuleCtx<Self>) -> anyhow::Result<()>;
}

pub trait ActionMeta {
    fn risk(&self) -> Risk;                  // Read | Write | Destructive
}
```

Three obligations for every module — these turn a "feature" into a "capability of the AI":

| Obligation | Used by |
|---|---|
| `Action` + JSON Schema + doc-comment written for the LLM + `Risk` | UI, CLI, IPC, **Agent tools**, **MCP server** |
| `context()` → `ContextFragment` | **Context Engine** (the AI knows your "now") |
| Domain `Event` with `summary()` | Journal, **Triggers**, **Memory** (the AI knows "what happened") |

- **Typed inside, erased at the edge**: the kernel wraps a module into `ModuleHandle { id, actions: Vec<ActionSpec>, dispatch, state_json, context }`. The Swift UI uses typed values (via UniFFI); AI/IPC/MCP use JSON `Envelope`s.
- **Supervisor**: a failing module is restarted with backoff, without crashing the app.
- **Shared Scheduler**: panel open → fast; background → slow; on battery → slower; sleep → stopped; wake → refresh.
- **EventBus**: realtime `broadcast` + an `events` table (configurable retention).

---

## 5. The Rust ⇄ Swift boundary (`navori-ffi`, UniFFI 0.32)

### Rust → Swift surface (complete)

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
    async fn shutdown(&self);
}
```

### Foreign traits (Swift implements, Rust calls)

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

### Data types
- Module State/Action derive `uniffi::Record` / `uniffi::Enum` **behind the `ffi` feature** (`#[cfg_attr(feature = "ffi", derive(uniffi::Record))]`) → modules still do not depend on UniFFI when building the CLI/tests, yet Swift gets fully typed values (`GithubState`, `AwakeAction`…) without hand-written mirror types.
- `ModuleSnapshot` / `ModuleCommand` are aggregate enums, extended by one variant per new module.

### Known risks & how they are handled
| Issue | Handling |
|---|---|
| UniFFI-generated async code is not yet fully `Sendable`-conformant on Swift 6 (uniffi-rs #2448) | The bindings live in a separate `NavoriCore` package with strict concurrency relaxed there; the app only touches `CoreStore` (`@MainActor`). |
| Callbacks arrive on a Rust thread | The listener adapter always does `Task { @MainActor in … }`; Rust has already coalesced. |
| Panic across FFI | UniFFI catches it and converts it to an error; add a `panic hook` that writes `tracing` + a crash marker. |
| Data copied across the boundary | Send only coalesced snapshots, not individual items; large lists are paginated. |
| Stack traces break at the boundary | Every command/turn carries a `trace_id`; both sides log the same ID (`os.Logger` ↔ `tracing`). |

### Build pipeline (one command)

```
cargo xtask build-core [--profile dev|release]
  1. cargo build -p navori-ffi --lib            → libnavori_ffi.a (aarch64, MACOSX_DEPLOYMENT_TARGET=15.0)
  2. uniffi-bindgen-swift --swift-sources / --headers / --modulemap (module navori_ffiFFI)
  3. xcodebuild -create-xcframework             → apps/macos/Generated/NavoriCoreFFI.xcframework
cargo xtask bundle [--profile dev|release]
  4. swift build (SwiftPM, arm64) + cargo build -p navori-cli
  5. assemble the .app: MacOS/Navori, Helpers/navori, Info.plist (profile, bundle id, LSUIElement)
  6. codesign --options runtime: CLI first, app second (identity "Navori Local Signing", ad-hoc + warning if missing)
cargo xtask run      # bundle dev + (re)launch "Navori Dev.app"
cargo xtask install  # bundle release → /Applications (keeps the old copy as .previous, auto-rollback if the copy fails)
cargo xtask ci       # fmt --check, clippy -D warnings, test, check-deps, swift build
```
- **SwiftPM instead of an Xcode project** (decided when building R0): no XcodeGen install needed, everything runs from the CLI; opening `apps/macos/Package.swift` in Xcode still gives Previews/debugger. Info.plist is generated by xtask, so there is no project file to keep in sync.
- Note when writing doc comments in Rust: do not use the sequence `*/` — UniFFI copies doc comments into Swift block comments.

---

## 6. AI Runtime

### 6.1 `navori-llm` — internal standard + provider adapters

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

### 6.2 `navori-models` — catalog & Model Router

- **Model catalog**: a built-in data file (`models.toml`: context window, capabilities `tools/vision/reasoning/caching`, input/output price) + refresh from `list_models()` + user-added models. Model IDs are **never hard-coded** in logic.
- **Roles** — the user assigns a model to each role in Settings:

| Role | Used for | Suggestion |
|---|---|---|
| `chat` | conversation, multi-step agents | the strongest model the user picks |
| `fast` | classification, event summaries, proactive decisions | a cheap/fast model |
| `vision` | understanding screenshots | a model with vision |
| `embedding` | memory search | any provider with embeddings |

- Each conversation can switch models midway (history is in the canonical format, so switching provider works).
- **Fallback chain** on 429/5xx/quota exhausted (e.g. Claude → GPT → DeepSeek), configurable.
- **Usage & budget**: tokens + estimated cost per turn are recorded in SQLite; a daily limit for proactive tasks; shown in Settings.
- Each provider's API key is stored in the Keychain (`navori-secrets`), never in config.toml or logs.

### 6.3 `navori-context` — Context Engine + Context Gateway

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

### 6.4 `navori-agent` — Agent Runtime

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

### 6.5 `navori-memory`
- Everything written to memory has already passed through `navori-redact` — secrets never sit in the DB.
- **Facts/preferences** ("I use squash merge", "repo X is work") — the AI writes them via the `memory.remember` tool; the user views/deletes them in Settings.
- **Episodic**: the `fast` model summarizes the journal by hour/day → a short activity log.
- **Embeddings** for semantic search: the `VectorIndex` trait; starting with brute-force cosine in Rust (personal scale, tens of thousands of vectors is still fast), later switching to `sqlite-vec` (currently alpha) without changing the API.

### 6.6 `navori-triggers` — Proactive
- Rules on the EventBus, for example: CI fails on my PR → summarize the error log · new review request → summarize the diff · a Claude Code session ends → notify + release awake · port conflict when starting a dev server.
- Pipeline: event → rule match (cheap, no AI call) → the `fast` model decides "is this worth interrupting" → if so, the `chat` agent does the work → notification.
- Daily token **budget**, **quiet hours**, per-rule rate limit, a "don't notify me about this again" button.

### 6.7 `navori-mcp` (`rmcp`)
- **MCP client**: the user adds MCP servers (Linear, Notion, filesystem…) → their tools enter the Tool Registry, defaulting to `Risk::Write` unless the user lowers it.
- **MCP server**: `navori mcp` (stdio) → Claude Code/Cursor can call Navori's tools ("which ports are running in this worktree?", "keep the machine awake until I'm done", "which PRs are waiting on me?"). Same policy engine as the internal agent.

---

## 7. Data & storage

| Kind | Location | Technology |
|---|---|---|
| Config | `~/Library/Application Support/Navori/config.toml` | `serde` + `toml`, sections per module/AI; read at startup (bad file → run with defaults + report the error). Hot-reload via `notify`: not done yet, added when the first module needs it |
| State, cache, journal, AI sessions, memory, usage | `~/Library/Application Support/Navori/navori.db` | SQLite WAL, `rusqlite` (bundled) + `rusqlite_migration`; tables prefixed by owner (`github_*`, `ai_session_*`, `memory_*`) |
| Secrets (GitHub token, provider API keys) | Keychain, service `dev.navori` | `keyring` (apple-native). Dev: `NAVORI_SECRET_STORE=file` (0600) |
| Logs | `~/Library/Logs/Navori/` | `tracing` + `tracing-appender`; prompt content is not logged unless debug is enabled |
| Socket | `~/Library/Application Support/Navori/run/navori.sock` (fallback `/tmp/navori-<uid>/<profile>.sock` when the path exceeds 103 bytes) | directory 0700, socket 0600, peer UID == socket owner UID; the socket is also the single-instance lock |

> **A stable signature is mandatory** — even without a paid Apple Developer account. With ad-hoc signing the hash changes on every build → macOS treats it as a new app: the Keychain asks for the password again, and Accessibility / Screen Recording permissions are lost. The solution: one **self-signed code-signing certificate** (`Navori Local Signing`, created once with `cargo xtask signing-setup`, valid 10 years) used for **every** build. The designated requirement = bundle ID + certificate → stays the same across builds.
>
> The dev build uses its own bundle ID (`dev.navori.app.debug`) and its own socket/DB → it never touches the build you use day to day.

---

## 8. Shared service: `navori-procs`

Awake, Ports and the Context Engine all need to know "which project does this process belong to, and who started it":
- **ProcessTable** (implemented in `navori-macos`: libproc + `sysctl KERN_PROCARGS2`): pid, ppid, uid, exe, argv, cwd, start time.
- **ProjectResolver**: cwd → `.git` → repo root; `.git` is a file → **worktree** → `{project, worktree, branch}`.
- **HarnessCatalog**: recognizes ancestors such as `claude`, `codex`, `grok`, `gemini`, `opencode`, `cursor-agent`… (configurable) and the terminal host.
- **ProcWatcher**: `kqueue EVFILT_PROC/NOTE_EXIT` → learns of a process death immediately, no polling.

---

## 9. Feature — Awake (`navori-mod-awake`)

**Lease model**:
```rust
struct Lease { id, source: Manual | Timer | Process{pid} | Agent{harness, session_id, pid} | Command,
               mode: System | Display, expires_at: Option<_>, grace: Duration }
```
- **Reconciler**: with ≥ 1 lease → hold exactly one IOKit assertion (`PreventUserIdleSystemSleep`, + `PreventUserIdleDisplaySleep` if needed). Idempotent.
- A lease ends when: released · TTL expires · the process dies (kqueue) · the app exits (the OS reclaims the assertion itself).

**Harness integration — verified on the machine (2026-11-01)**: all three harnesses have Claude Code-style hooks (JSON via stdin, with `hook_event_name` + session id + `cwd`). So **one** `navori hook` command serves all of them.

| Harness | Hook | acquire | release after grace | release immediately |
|---|---|---|---|---|
| Claude Code | `~/.claude/settings.json` | `UserPromptSubmit` | `Stop` | `SessionEnd` |
| Grok Build (`grok` 1.0.70) | `~/.grok/hooks/navori.json` | `UserPromptSubmit` | `Stop` (only when `reason == "end_turn"`), `StopCancelled`, `StopFailure` | `SessionEnd` |
| Codex (`codex-cli` 0.166) | `~/.codex/hooks.json` | `UserPromptSubmit` | `Stop` | `SessionEnd` |
| Any command | — | `navori awake run -- <cmd>` | — | when the command exits |
| Harness without hooks | — | auto-detect via HarnessCatalog | silent for N minutes | process dies |

Important details:
- **Harness identification**: `navori hook` determines the source on its own (the `GROK_*` environment variables of the Grok runner, then walking up the process tree looking for `claude` / `grok` / `codex`) → obtains the harness's real PID for kqueue to watch. No arguments needed.
- **Duplicate-count protection**: Grok **also reads the hooks in `~/.claude/settings.json`** (Claude-compatibility mode) → the same event may arrive twice. Every lease operation is **idempotent per `(harness, session_id)`**, so duplicates do no harm.
- **"Stop" ≠ "work finished"**: a harness may still have background tasks (background bash, subagents, long-running tasks) after the turn ends. Before releasing the lease once the grace period elapses, Awake inspects the **harness's child process tree**; if a child is still running/consuming CPU, it extends the lease.
- **Installation**: Settings → Integrations has Install/Repair/Remove buttons per harness. Navori **merges** into the existing config file (backup before writing, never overwriting other hooks), and `navori doctor` checks whether the hook is still correct.
- **Safety net**: if a hook fails or does not run → kqueue (process death) + TTL still guarantee the lease cannot hang forever.

AI tools: `awake.status` (Read), `awake.hold{duration, reason}` (Write), `awake.release` (Write).
Out of scope (decided): staying awake with the lid closed.

---

## 10. Feature — Ports (`navori-mod-ports`)

- **Scan** via libproc (no `lsof`): pids of the current UID → socket fds → TCP `LISTEN` + port. Usually < 50 ms.
- **Enrich** via `navori-procs`: project · worktree · branch · harness · kind (dev server, database, docker).
- **Display**: Project → Worktree → Process → Ports.
- **Actions**: open `localhost:port` · copy · reveal in Finder · kill one / by worktree / project / all. Kill = `SIGTERM` to the process group → 3s → `SIGKILL`. Never touches other UIDs; a `protected` list is excluded from "kill all".
- Polling every 3s when the panel is open, 30s in the background.
- AI tools: `ports.list` (Read), `ports.kill{pid|worktree|project}` (**Destructive**), `ports.open` (Read).

---

## 11. Feature — GitHub PRs (`navori-mod-github`)

- **Inbox**: Needs my review · My PRs · Involved. Per PR: repo, #, title, draft, review decision, CI rollup, mergeable, labels, updated.
- **Auth — OAuth App + Device Flow** (a long-term decision, not a temporary one; see §15 for the rationale). Scopes: `repo`, `read:org`, `notifications`, `workflow`. No client secret, no redirect server — the same way `gh auth login` works. Token → Keychain.
  - Multiple accounts (personal + work) supported from day one: tokens are keyed by `account_id`.
  - Orgs with SAML SSO: guidance to authorize the token for the org inside the app; orgs that block OAuth Apps: send an approval request, or use a PAT (classic) as the fallback.
  - A `TokenProvider` trait (`OAuthDevice`, `ClassicPat`) reserves room for other methods without changing the module.
- **Data**: a single GraphQL request (3 aliased search queries) via `reqwest` + `graphql_client`; track rate-limit headers; SQLite cache → opening the app shows data immediately.
- **Fast, cheap change detection**: poll the **Notifications API** with `If-Modified-Since` and respect `X-Poll-Interval` (a 304 costs no quota) → only when there is a new notification does GraphQL run again. This is why an OAuth App is needed: per GitHub docs, the notifications endpoints only support classic-type tokens (OAuth/PAT classic), not GitHub App user tokens or fine-grained PATs.
- `domain::diff(prev, next) → Vec<GithubEvent>` → notification + journal + **Triggers**.
- AI tools: `github.inbox` (Read), `github.pr.get{diff, checks, comments}` (Read), `github.pr.summarize` (Read), `github.pr.comment` / `approve` (Write), `github.pr.merge` (**Destructive**), `github.checks.rerun` (Write).
- AI-first scenarios: "Summarize the 3 PRs waiting on me, which is the smallest?" · "Why did CI on PR #42 fail?" · a trigger that automatically summarizes CI failures.

---

## 12. Tech stack

| Area | Choice |
|---|---|
| **Rust core** | |
| Async | `tokio`, `tokio-util`, `futures` |
| Errors | `thiserror` (lib), `anyhow` (edge) |
| Serde / Schema | `serde`, `serde_json`, `toml`, `schemars` |
| Time / ID | `time` (or `jiff`), `ulid` |
| Logging | `tracing`, `tracing-subscriber`, `tracing-appender` |
| Storage | `rusqlite` (bundled), `rusqlite_migration`; (later) `sqlite-vec` |
| Secrets | `keyring` (apple-native) |
| HTTP / SSE | `reqwest` (rustls), `eventsource-stream` |
| GitHub | `graphql_client` |
| MCP | `rmcp` |
| macOS FFI | `objc2-io-kit`, `libproc`, `libc` |
| FFI bridge | `uniffi` 0.32 |
| IPC / CLI | tokio `UnixListener` + JSON-RPC 2.0, `clap` |
| **Swift shell** | |
| UI | AppKit (`NSStatusItem`, `NSMenu`, `NSPanel`) + SwiftUI, `@Observable` |
| Markdown (chat) | a SwiftUI renderer (e.g. swift-markdown-ui) |
| Hotkey | `KeyboardShortcuts` (sindresorhus) |
| System | `UserNotifications`, `ScreenCaptureKit`, `Vision` (OCR), `CoreImage` (blur), `ApplicationServices` (AX), `ServiceManagement` (SMAppService) |
| Updates | `cargo xtask install` (build from source); Sparkle 2 only once there is a Developer ID (open-source release) |
| Project | SwiftPM (`Package.swift`) + `cargo xtask bundle` |
| **Quality** | `cargo-nextest`, `insta`, `wiremock`, `proptest`; Swift Testing for `CoreStore`; `cargo-deny`; `[workspace.lints]` (`unsafe_code = "forbid"` except `macos`, `ffi`) |

---

## 13. Quality & operations

- **Testability**: system IO & network sit behind traits (`PowerManager`, `ProcessTable`, `GithubApi`, `LlmProvider`, `PlatformServices`) → tested with fakes. A scripted `FakeLlm` (returns text/tool calls per a script) tests the agent loop without spending tokens.
- **AI evals**: a set of scenarios (`xtask eval`) — "kill the port of worktree X" must call the right tool + the right arguments + ask for approval — run across many models to learn which model is usable for which role.
- **Diagnostics**: `navori status`, `navori doctor` (socket, Keychain, AX/Screen Recording permissions, each harness's hook, GitHub token, each provider's key, rate limit), `navori logs`, `navori ai usage`.
- **Performance budget**: GOAL-7 — idle 0% CPU, ≤ 1 wakeup/s, ≤ 30 MB footprint; ≤ 60 MB with all features and the menu open; cold start to menubar icon ≤ 200 ms; menu open ≤ 50 ms; submenu ≤ 16 ms; release .app ≤ 25 MB. Measured by `cargo xtask perf` (method in `docs/standards/performance.md`).
- **Durable data**: versioned SQLite migrations, **automatic DB backup before every migration**, rollback on failure; a bad config → use the previous values + report the error, no crash.
- **Release (now — single user, no paid Apple Developer account needed)**: `cargo xtask install` = release build → sign with the self-signed certificate (hardened runtime, sign `Helpers/navori` first, then the app) → `codesign --verify --strict` → safely replace `/Applications/Navori.app` (quit the old app, keep the old copy as `.previous` for rollback) → relaunch. Update = `git pull && cargo xtask install`. Login item via SMAppService, with a LaunchAgent fallback in `~/Library/LaunchAgents` if SMAppService rejects the self-signed signature.
- **Release (later, open source)**: the same pipeline; switch `signing.identity` to a Developer ID → additionally enable notarize + staple + DMG + Sparkle appcast (`cargo xtask release`). Other developers then install it themselves via `brew install --cask` or a download page (requires Developer ID + notarization at that time). No architecture change.
- **Crash**: a Rust panic hook + crash marker; on the next launch it shows "Navori just crashed" with a button to open the log.

### Definition of Done

The Definition of Done lives in `docs/standards/INDEX.md` (trimmed for a single user: onboarding is limited to macOS permission prompts; a feature needs only a short note in `docs/features/<feature>.md`).

---

## 14. Roadmap — every release is a complete feature

No v1/v2 split. Each release adds **one** complete capability per the Definition of Done; once a release is done it is usable every day immediately.

| Release | Goals | What you can use right away | Includes |
|---|---|---|---|
| **R0 — Foundation** *(groundwork, not a feature)* | GOAL-7, GOAL-8 | A stably-signed menubar app, installed/updated with one command, launching at login, Settings, the `navori` CLI + `doctor` | Rust workspace + SwiftPM; kernel, store, config, logging, redact; `navori-ffi` + `CoreStore`; IPC; `xtask signing-setup` / `install` |
| **R1 — Awake** | GOAL-2 | Keep the machine awake manually/on a timer/per command; auto-hold & auto-release following Claude Code, Grok, Codex sessions; one-button hook install | lease, IOKit, kqueue, `navori hook`, Integrations UI |
| **R2 — AI Assistant** | GOAL-1, GOAL-5, GOAL-6 | Chat in the menubar + quick-ask hotkey with **any model** (Claude, ChatGPT, DeepSeek, Gemini, Grok, OpenRouter, custom endpoint); control Awake in plain language | 4 adapters + conformance, catalog + roles + fallback + usage/budget, agent loop + policy + approvals, Context Engine/Gateway + Sensitive Data Shield, memory |
| **R3 — Ports** | GOAL-3 | See all ports by project/worktree/harness, open/kill each one or all; ask the AI to "clean up the ports of merged worktrees" | libproc, `navori-procs`, kill with approval |
| **R4 — GitHub** | GOAL-4 | PR inbox (needs review / mine / involved), CI, approve/merge, notifications; the AI summarizes PRs & CI failures | device flow, multi-account, notifications polling, GraphQL, cache |
| **R5 — Proactive** | GOAL-9, GOAL-1 | The AI speaks up on its own when it matters: CI fail, new review, agent session done, port conflict | triggers, budget, quiet hours |
| **R6 — MCP** | GOAL-10 | Claude Code/Grok/Codex use Navori's tools; Navori uses external MCP servers | `navori-mcp` client + server |

The roadmap will move to `docs/roadmap/INDEX.md` when it is planned with plan-milestones.

**Why AI is in R2 and not last**: AI is the core (P1). Placing it early lets the `Action` / `context()` / `Event` contracts be validated by a real agent before Ports and GitHub are written — those two features are born as AI tools. Screen observation (screenshot + OCR) is in R2 because the Sensitive Data Shield is a prerequisite for releasing AI.

---

## 15. Decisions locked

| # | Decision | Rationale |
|---|---|---|
| D1 | **UI**: Swift (AppKit + SwiftUI) + Rust core via UniFFI | The most native for macOS, with logic still in Rust. |
| D2 | **AI**: cloud models, any provider, the user chooses per role | AI-first, no lock-in to one vendor. |
| D3 | **GitHub**: OAuth App + Device Flow (long-term) | (a) The Notifications API — the cheapest way to detect changes — supports only classic tokens, not GitHub App user tokens. (b) Access to every repo you have permission for, with no need to install an app into each org (a GitHub App is limited to installed repos, including search results). (c) No server needed: a GitHub App is only truly superior when it has webhooks, and a local app cannot receive webhooks. Trade-off: the `repo` scope is broader and the token does not expire → keep it in the Keychain, with a revoke button. This is also the model of the `gh` CLI. |
| D4 | **CPU**: Apple Silicon only (`aarch64-apple-darwin`) | Faster builds, smaller binaries, no Intel testing. |
| D5 | **Harness**: Claude Code, Grok Build, Codex via hooks; others auto-detected | Hooks of all three verified on the machine (2026-11-01). |
| D6 | **Running with the lid closed**: not supported | Requires a privileged helper running as root — not worth it. |
| D7 | **AI observation scope**: see everything except secrets | A 4-layer Sensitive Data Shield, fail-closed, with an audit log and a pause button. |
| D8 | **App signing**: a self-signed code-signing certificate, no paid Apple Developer account yet | Single user (the author) now: the app runs only on the author's machine, built from source → Gatekeeper does not block it (no quarantine flag). Every API needed (IOKit, libproc, AX, ScreenCaptureKit, regular Keychain, notifications) requires no paid entitlement; the app is not sandboxed. Open source later: other developers install it themselves via `brew install --cask` or a download page, which requires a Developer ID + notarization at that time. |
