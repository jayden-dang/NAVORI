# Navori

Navori is an AI-first personal cockpit for macOS (Apple Silicon): a menubar app whose
logic lives in a Rust **Core** and whose UI is a Swift **Shell**. Features plug into the
Core as **Modules**; the UI, the `navori` CLI, Navori AI and Harnesses over MCP all drive them through the same
**Actions**, and everything that happens is recorded as **Events** in the **Journal**.
Vision: `docs/product/vision.md` · Invariants: `docs/architecture/INDEX.md` ·
System narrative: `docs/ARCHITECTURE.md`.

This glossary holds product concepts only. Implementation names (CoreStore, Snapshot,
EventBus, ContextFragment, composition root…) are explained in code doc comments.
Add or change terms through `define-domain`.

## Language

### Runtime

**Core**:
The Rust side of the app — kernel, modules, store and IPC server — linked into the app via
UniFFI (`navori-ffi`). The single source of truth for state.
_Avoid_: backend, daemon, server

**Shell**:
The Swift (AppKit + SwiftUI) menubar app in `apps/macos`. Renders Core state and sends
Actions; holds no business logic.
_Avoid_: frontend, client

**Kernel**:
The registry, router and supervisor of Modules inside the Core.
_Avoid_: runtime, engine

**Module**:
One capability (Awake, Ports, GitHub…) running as a supervised actor in the Kernel, with its
own Actions and state. A crashed Module is restarted with backoff and keeps its inbox.
_Avoid_: plugin, service

**Action**:
A typed request a Module accepts: a name, a description written for people and LLMs, a JSON
schema, and a **Risk**.
_Avoid_: command, RPC (for the domain concept)

**Risk**:
How dangerous an Action is: `read` (observes only), `write` (recoverable change) or
`destructive` (irreversible or disruptive — always needs explicit human approval).
_Avoid_: severity, permission level

**Envelope**:
An untyped Action request `{ module, action }` sent from the UI, CLI, IPC, AI or MCP.
_Avoid_: message, payload

**Event**:
Something that happened, emitted by a Module or the Core (e.g. `core.started`,
`module.crashed`), with a severity and a one-sentence summary for people.
_Avoid_: log line, notification

**Journal**:
The durable Event history in the SQLite store, written only after the Sensitive Data Shield,
pruned by retention (default 30 days).
_Avoid_: log, history

**Owner**:
A storage namespace (`core`, `github`…) that versions its own tables through migrations.
_Avoid_: schema, tenant

**Profile**:
Which install is meant: `release` (`Navori.app`, daily use) or `dev` (`Navori Dev.app`,
development). Profiles never share data, logs or sockets.
_Avoid_: environment, build type

### AI

**Navori AI**:
Navori's own built-in assistant: it answers the user, reads context through the Context
Gateway and acts only by calling Actions.
_Avoid_: the agent, assistant (alone), bot

**Provider**:
A cloud LLM service the user configures (Anthropic, OpenAI, DeepSeek, Google, xAI,
OpenRouter or a custom OpenAI-compatible endpoint), reached through an adapter.
_Avoid_: vendor, backend, LLM API

**Model role**:
A job Navori AI needs a model for — `chat`, `fast`, `vision`, `embedding` — to which the user
assigns one model from any Provider.
_Avoid_: role (alone — collides with a message's user/assistant role), tier

### Needs you

**Needs you**:
The inbox of decisions between the user and Navori AI, holding Decisions, To-dos and
Updates. Not a list of pull requests.
_Avoid_: notifications, inbox of PRs, alerts

**Decision**:
A Needs you item where Navori AI asks and the user decides, answered with an inline button.
_Avoid_: prompt, confirmation dialog

**To-do**:
A Needs you item that is the user's own task, created by the user or suggested by the AI;
completed by ticking it.
_Avoid_: task, reminder

**Update**:
A Needs you item for information only; it needs no action.
_Avoid_: alert, notification

### Agents & machine

**Harness**:
An agentic CLI that Navori observes: Claude Code, Codex, Grok Build.
_Avoid_: agent (alone — say Harness, Agent session or Navori AI), bot

**Agent session**:
One running session of a Harness in a project or worktree.
_Avoid_: run, job

**Lease**:
A request to keep the Mac awake from one source (manual, timer, Agent session…). The Mac stays
awake while at least one Lease is held.
_Avoid_: hold, keep-awake request, assertion (that is the IOKit mechanism)

**Tracking agents**:
The on/off setting that decides whether **Agent session** **Leases** keep the Mac awake. While it
is off, Navori still follows each session, and turning it on holds for them at once.
_Avoid_: pause, sleep mode

**Integration**:
Navori's entries installed into another tool's config — today a Harness's hooks, later its
MCP server entry — installed, repaired and removed from Settings → Integrations.
_Avoid_: plugin, extension, connector

**Connected account**:
The user's own account on an external service (GitHub, Gmail, Slack…) that Navori may reach
with the user's authorization and that the user can revoke.
_Avoid_: connector, connection, integration, linked account

**Port**:
A TCP port that one of the user's own processes listens on, shown with the process, its
project, worktree and branch.
_Avoid_: socket, endpoint

**Listener**:
One process holding one or more **Ports**; the unit that Ports lists and groups.
_Avoid_: server, service

**Placement**:
Where a **Listener** runs: its project, worktree and branch, or the missing directory it was
started in, and the `root` that holds them (the checkout directory a project row shows). It is the
Ports Module's form of `Location` in `navori-procs`, adding the missing path.
_Avoid_: location (that is the `navori-procs` type)

**Running**:
The menubar row, and the view behind it, that joins what is running: **Agent sessions** from Awake
and **Ports** from the Ports Module, one row per project and one for ports with no project.
_Avoid_: activity, dashboard

**Stop**:
Ending the **Listeners** (and Docker containers) behind named **Ports**: ask the process to exit,
wait, then end it if it is still the same process. A stop acts only on the exact targets it is
given, and never on Navori, its ancestors, its helper or Docker's backend.
_Avoid_: kill (that is the Action's name), close

### Privacy

**Sensitive Data Shield**:
The layer (`navori-redact`) that detects and masks secrets before anything is stored or sent.
_Avoid_: filter, sanitizer

**Context Gateway**:
The single path by which data reaches an LLM, with redaction and an audit log.
_Avoid_: proxy, AI client

## Relationships

- The **Core** hosts one **Kernel**, which hosts many **Modules**; the **Shell**, the CLI, Navori AI and MCP clients send them **Envelopes**
- A **Module** emits **Events**; every **Event** passes the **Sensitive Data Shield**, is written to the **Journal** and broadcast
- Each **Profile** has its own **Journal**
- An **Agent session** of a **Harness** may hold a **Lease**
- Navori AI raises **Decisions**, **To-dos** and **Updates** in **Needs you**; a **Destructive** **Action** proposed by the AI always becomes a **Decision** first
- Data reaches an LLM only through the **Context Gateway**

## Flagged ambiguities

- "agent" alone was used for both an Agent session and any AI caller of Actions — resolved: never use it alone; say **Agent session**, **Harness** or **Navori AI**.
