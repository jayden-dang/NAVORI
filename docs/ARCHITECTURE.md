# NAVORI — Architecture

> macOS-only menubar app, chạy local. **AI-first personal cockpit**: một trợ lý AI hiểu bạn đang làm gì, nắm toàn bộ công cụ cá nhân (GitHub, máy, ports, harness agentic…) và hành động thay bạn — dùng **model cloud mạnh nhất, bất kỳ provider nào bạn chọn** (Claude, ChatGPT, DeepSeek, Gemini, Grok, OpenRouter…).
>
> Hình thái: **Rust core** (logic, AI runtime, modules) + **Swift shell** (AppKit + SwiftUI) nối qua **UniFFI**. **Chỉ Apple Silicon.**
> Feature đầu tiên: **Awake (caffeine)**, **Ports**, **GitHub PRs** — đồng thời là bộ *tool* đầu tiên của AI.
> **Không có bản MVP**: mỗi release là một feature hoàn chỉnh, production-ready, dùng được ngay (xem §14).

---

## 1. Nguyên tắc thiết kế

| # | Nguyên tắc | Hệ quả |
|---|---|---|
| P1 | **AI-first, không phải AI-addon** | AI runtime là tầng lõi ngang hàng kernel, có từ sớm. Mọi module sinh ra đã phải "AI-readable" (state mô tả được) và "AI-operable" (action có schema). |
| P2 | **Model-agnostic** | Không phụ thuộc một provider. Định dạng message/tool/stream nội bộ là chuẩn riêng; provider chỉ là adapter. User chọn model cho từng vai trò, từng cuộc hội thoại. |
| P3 | **Rust sở hữu logic, Swift sở hữu trải nghiệm** | Rust: kernel, modules, AI, store, IPC, FFI hệ thống cấp thấp. Swift: UI và các framework Apple gắn với app bundle / quyền (Accessibility, ScreenCaptureKit, Notifications, Login item, Hotkey). Swift **không** chứa logic nghiệp vụ. |
| P4 | **Ranh giới FFI hẹp** | Swift nói với Rust qua ~6 hàm + vài listener. Không gọi lắt nhắt. Dữ liệu đi qua là snapshot/command, không phải object graph. |
| P5 | **Mọi thứ là Module** | Feature = module actor độc lập, đăng ký vào kernel. Module không import nhau; giao tiếp qua Event Bus / Command. |
| P6 | **Action có schema + rủi ro** | Một bề mặt action duy nhất phục vụ UI, CLI, IPC, AI agent, MCP. `Read` / `Write` / `Destructive` quyết định AI có cần xin phép không. |
| P7 | **Mọi thay đổi là Event** | Event journal trong SQLite = timeline ngữ cảnh cho AI. |
| P8 | **Thấy hết, trừ bí mật** | AI được thấy toàn bộ ngữ cảnh (app, cửa sổ, text, màn hình) **trừ** dữ liệu nhạy cảm (API key, SSH key, token, password, private key, `.env`…). Mọi dữ liệu rời máy đi qua **một** cổng (Context Gateway + Sensitive Data Shield), **fail-closed**. Secrets của Navori chỉ trong Keychain. `unsafe` chỉ trong `navori-macos` và `navori-ffi`. |
| P9 | **Tiết kiệm tài nguyên & chi phí** | Idle CPU ≈ 0%. Polling thích ứng. AI proactive có budget (token/ngày) và rate-limit; việc rẻ dùng model rẻ. |

---

## 2. Tổng quan runtime

```
┌──────────────────────────────────────── Navori.app (1 process) ─────────────────────────────────────────┐
│                                                                                                         │
│  SWIFT SHELL (main thread)                                                                              │
│   ├─ StatusBar      NSStatusItem + NSMenu/NSPanel (AppKit) chứa view SwiftUI                            │
│   ├─ Features/      Awake · Ports · GitHub · Assistant (chat, quick-ask, approvals)                     │
│   ├─ CoreStore      @Observable, nhận snapshot từ Rust, hop về @MainActor                               │
│   └─ Platform/      implement foreign traits: Notifier · AXObserver · ScreenCapture · LoginItem · HotKey │
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
│   │ (sau: calendar, notes, │◄──┤ Agent Runtime (loop, tools, policy, approvals)  ◄───┤         │        │
│   │  clipboard, mail, …)   │   │ Model Router (roles, fallback, budget)          ────┤         │        │
│   └────────────────────────┘   │ LLM Providers: anthropic · openai · openai-compat · gemini   │        │
│                                │ Memory (facts, episodes, embeddings) · Triggers (proactive)  │        │
│                                │ MCP client (dùng tool ngoài) · MCP server (lộ tool Navori)   │        │
│                                └───────────────────────────────────────────────────────────────┘        │
│   IPC server (Unix socket, JSON-RPC 2.0) ◄─────────────────────────────┐                                │
└────────────────────────────────────────────────────────────────────────┼────────────────────────────────┘
                                                                         │
                 `navori` CLI ◄── Claude Code hooks · Codex notify · scripts · `navori ask "…"`
                 MCP (stdio qua `navori mcp`) ◄── Claude Code / Cursor / agent khác dùng tool của Navori
                 Cloud LLM APIs ◄── chỉ đi ra từ LLM Providers, qua Context Gateway
```

- **Một process** chứa Swift shell + Rust core. **CLI** `navori` là binary Rust riêng, nằm trong bundle (`Navori.app/Contents/Helpers/navori`), nói với app qua socket.
- Rust core khởi tạo **một lần** khi app launch (`NavoriCore::start`) và sở hữu tokio runtime riêng; Swift không biết tokio tồn tại.

### Concurrency
- **Swift**: main thread cho AppKit; mọi callback từ Rust được hop về `@MainActor` trong **một** lớp `CoreStore` — không chỗ nào khác chạm vào binding.
- **Rust**: mỗi module/agent session là một **actor** (tokio task + `mpsc` inbox + `watch` snapshot + `broadcast` event). `CancellationToken` cho shutdown/cancel (bao gồm hủy một lượt AI đang stream).
- **Coalescing**: snapshot gửi sang Swift được gộp (≤ 30 lần/s, chỉ khi panel visible); token AI gộp theo khung ~33 ms. Không flood main thread.

---

## 3. Cấu trúc repository

```
NAVORI/
├── Cargo.toml                    # [workspace], [workspace.dependencies], [workspace.lints]
├── rust-toolchain.toml           # stable, edition 2024, target aarch64-apple-darwin
├── .cargo/config.toml            # alias `cargo xtask …`
│
├── crates/                       # ── nền tảng ──
│   ├── kernel/                   # navori-kernel: Module trait, registry, router, bus, scheduler, supervisor
│   ├── protocol/                 # navori-protocol: Envelope, Event, ActionSpec, Risk, ContextFragment
│   ├── macos/                    # navori-macos: unsafe FFI cấp thấp (IOKit power, libproc, kqueue, sysctl)
│   ├── procs/                    # navori-procs: process graph, project/worktree resolver, HarnessCatalog
│   ├── store/                    # navori-store: SQLite WAL, migrations per module, KV, event journal
│   ├── secrets/                  # navori-secrets: Keychain + file backend (dev)
│   ├── config/                   # navori-config: config.toml typed, hot-reload
│   ├── ipc/                      # navori-ipc: Unix socket JSON-RPC server/client
│   ├── redact/                   # navori-redact: Sensitive Data Shield — detector bí mật dùng chung (context, memory, logs)
│   └── ffi/                      # navori-ffi: lớp UniFFI DUY NHẤT Swift nhìn thấy; composition root của app
│
├── ai/                           # ── AI runtime (lõi, không phải add-on) ──
│   ├── llm/                      # navori-llm: canonical types + LlmProvider trait + adapters
│   ├── models/                   # navori-models: model catalog, capabilities, pricing, Model Router
│   ├── context/                  # navori-context: Context Engine + Context Gateway (redaction, consent)
│   ├── agent/                    # navori-agent: agent loop, tool registry, policy, approvals, sessions
│   ├── memory/                   # navori-memory: facts, episodic summaries, embeddings, VectorIndex
│   ├── triggers/                 # navori-triggers: proactive rules trên EventBus, budget
│   └── mcp/                      # navori-mcp: MCP client (tool ngoài) + MCP server (tool Navori)
│
├── modules/                      # ── feature, mỗi cái một crate ──
│   ├── awake/                    # navori-mod-awake
│   ├── ports/                    # navori-mod-ports
│   └── github/                   # navori-mod-github
│
├── apps/
│   ├── cli/                      # `navori` binary (Rust): hook, awake, ports, ask, mcp, doctor
│   └── macos/                    # Swift shell
│       ├── Package.swift         #   SwiftPM; .app được lắp ráp + ký bởi `cargo xtask bundle`
│       ├── Navori/
│       │   ├── App/              #   AppDelegate, accessory policy, lifecycle
│       │   ├── StatusBar/        #   NSStatusItem, NSMenu, NSPanel controllers
│       │   ├── Features/         #   Awake/ Ports/ GitHub/ Assistant/ Settings/ (SwiftUI)
│       │   ├── Core/             #   CoreStore (@Observable), listener adapters
│       │   ├── Platform/         #   foreign-trait impls: Notifier, AX, ScreenCapture, LoginItem, HotKey
│       │   └── main.swift        #   NSApplication + accessory policy (Info.plist do xtask sinh)
│       └── Generated/            #   GENERATED (gitignored): NavoriCore/ (Swift bindings) + NavoriCoreFFI.xcframework
│
├── integrations/
│   ├── claude-code/              # hooks snippet + cấu hình MCP
│   └── codex/                    # notify config
├── xtask/                        # build-core, gen-bindings, bundle, sign, check-deps
└── docs/
    ├── ARCHITECTURE.md
    └── adr/
```

### Quy tắc phụ thuộc (kiểm bằng `cargo xtask check-deps`)

```
crates/ffi ─► ai/*, modules/*, crates/*         (composition root của app)
apps/cli   ─► crates/ipc, crates/protocol       (CLI mỏng, chỉ nói qua socket)
ai/agent   ─► ai/{llm,models,context,memory,mcp}, kernel, protocol
modules/*  ─► kernel, protocol, procs, store, secrets, config   (+ macos qua trait)
procs      ─► macos
ai/context, ai/memory, crates/store (journal) ─► redact     (mọi thứ ghi xuống đĩa hoặc rời máy đều qua redact)

CẤM: module ─► module · module ─► ai/* · ai/* ─► modules/* (AI chỉ thấy module qua ActionSpec/ContextFragment)
     bất kỳ crate nào ngoài `macos`, `ffi` dùng `unsafe`
```

Điểm mấu chốt: **AI không import module nào**. Nó chỉ thấy danh sách `ActionSpec` và `ContextFragment` do kernel cung cấp → thêm module mới là AI tự có tool mới.

### Layout bên trong một module

```
modules/github/src/
├── lib.rs        # impl Module, manifest
├── actor.rs      # select! { inbox, ticks, bus, shutdown }
├── action.rs     # enum Action (serde + JsonSchema + mô tả viết cho LLM) + Risk
├── state.rs      # Snapshot (Serialize; derive uniffi::Record sau feature "ffi")
├── context.rs    # State → ContextFragment (tóm tắt ngắn gọn cho AI)
├── event.rs      # domain events (có `summary()` dạng câu người đọc được)
├── domain/       # logic thuần, unit-test dày
└── infra/        # IO: http, auth, SQLite repo
```

---

## 4. Kernel: hợp đồng module (AI-first)

```rust
pub trait Module: Send + 'static {
    const ID: &'static str;                                  // "github"
    type Action: DeserializeOwned + JsonSchema + ActionMeta + Send;
    type State:  Serialize + Clone + Send + Sync + 'static;
    type Config: DeserializeOwned + Default + Send + Sync + 'static;

    /// AI-readable: trạng thái hiện tại, ngắn gọn, đã lọc dữ liệu nhạy cảm.
    fn context(state: &Self::State) -> ContextFragment;
    /// Badge / icon hint cho menubar.
    fn tray(state: &Self::State) -> TrayContribution { TrayContribution::none() }

    async fn run(self, ctx: ModuleCtx<Self>) -> anyhow::Result<()>;
}

pub trait ActionMeta {
    fn risk(&self) -> Risk;                  // Read | Write | Destructive
}
```

Ba nghĩa vụ của mọi module — đây là thứ biến "feature" thành "năng lực của AI":

| Nghĩa vụ | Dùng bởi |
|---|---|
| `Action` + JSON Schema + doc-comment viết cho LLM + `Risk` | UI, CLI, IPC, **Agent tools**, **MCP server** |
| `context()` → `ContextFragment` | **Context Engine** (AI biết "hiện tại" của bạn) |
| Domain `Event` có `summary()` | Journal, **Triggers**, **Memory** (AI biết "đã xảy ra gì") |

- **Typed bên trong, erased ở biên**: kernel bọc module thành `ModuleHandle { id, actions: Vec<ActionSpec>, dispatch, state_json, context }`. UI Swift dùng kiểu typed (qua UniFFI); AI/IPC/MCP dùng `Envelope` JSON.
- **Supervisor**: module lỗi → restart với backoff, không sập app.
- **Scheduler** dùng chung: panel mở → nhanh; nền → chậm; chạy pin → chậm hơn; sleep → dừng; wake → refresh.
- **EventBus**: `broadcast` realtime + bảng `events` (retention cấu hình được).

---

## 5. Ranh giới Rust ⇄ Swift (`navori-ffi`, UniFFI 0.32)

### Bề mặt Rust → Swift (toàn bộ)

```rust
#[uniffi::export]
impl NavoriCore {
    #[uniffi::constructor]
    fn start(cfg: CoreConfig, platform: Arc<dyn PlatformServices>) -> Result<Arc<Self>, CoreError>;

    fn subscribe(&self, listener: Arc<dyn SnapshotListener>);           // snapshot mọi module (enum ModuleSnapshot)
    async fn dispatch(&self, cmd: ModuleCommand) -> Result<ActionOutcome, CoreError>;   // typed command

    // AI
    fn ask(&self, req: AskRequest, sink: Arc<dyn AssistantStream>) -> Arc<TurnHandle>;  // stream + cancel()
    async fn resolve_approval(&self, id: String, decision: ApprovalDecision);

    fn set_panel_visible(&self, visible: bool);                          // điều tiết polling / coalescing
    async fn shutdown(&self);
}
```

### Foreign traits (Swift implement, Rust gọi)

```rust
#[uniffi::export(with_foreign)]
pub trait PlatformServices: Send + Sync {
    fn notify(&self, n: Notification);                         // UNUserNotificationCenter
    fn frontmost(&self) -> Option<FrontmostApp>;               // NSWorkspace + AX: app, window title, selected text, URL trình duyệt
    fn secure_input_active(&self) -> bool;                     // IsSecureEventInputEnabled — đang gõ password ở đâu đó
    async fn capture_screen(&self, req: CaptureRequest) -> Result<Capture, PlatformError>;
        // ScreenCaptureKit, SCContentFilter loại sẵn cửa sổ app trong denylist; kèm OCR (Vision) text + bounding boxes
    async fn redact_image(&self, img: Vec<u8>, regions: Vec<Rect>) -> Result<Vec<u8>, PlatformError>; // blur (CoreImage)
    fn permissions(&self) -> PermissionStatus;                 // AX, Screen Recording, Notifications
}
pub trait SnapshotListener: Send + Sync { fn on_snapshot(&self, s: ModuleSnapshot); }
pub trait AssistantStream: Send + Sync { fn on_event(&self, e: AssistantEvent); }  // delta, tool call, approval request, done, error
```

### Kiểu dữ liệu
- State/Action của module derive `uniffi::Record` / `uniffi::Enum` **sau feature `ffi`** (`#[cfg_attr(feature = "ffi", derive(uniffi::Record))]`) → module vẫn không phụ thuộc UniFFI khi build CLI/test, nhưng Swift có kiểu typed đầy đủ (`GithubState`, `AwakeAction`…) mà không phải viết mirror type.
- `ModuleSnapshot` / `ModuleCommand` là enum tổng hợp, mở rộng một biến thể cho mỗi module mới.

### Rủi ro đã biết & cách xử lý
| Vấn đề | Xử lý |
|---|---|
| Code async sinh bởi UniFFI chưa conform `Sendable` hoàn toàn trên Swift 6 (uniffi-rs #2448) | Bindings nằm trong package `NavoriCore` riêng, nới strict concurrency ở đó; app chỉ chạm vào `CoreStore` (`@MainActor`). |
| Callback đến trên thread của Rust | Listener adapter luôn `Task { @MainActor in … }`; Rust đã coalesce trước. |
| Panic qua FFI | UniFFI bắt và chuyển thành error; thêm `panic hook` ghi `tracing` + crash marker. |
| Dữ liệu copy qua ranh giới | Chỉ gửi snapshot đã gộp, không gửi từng item; danh sách lớn phân trang. |
| Stack trace đứt ở ranh giới | Mỗi command/turn mang `trace_id`, log cả hai phía cùng ID (`os.Logger` ↔ `tracing`). |

### Build pipeline (một lệnh)

```
cargo xtask build-core [--profile dev|release]
  1. cargo build -p navori-ffi --lib            → libnavori_ffi.a (aarch64, MACOSX_DEPLOYMENT_TARGET=15.0)
  2. uniffi-bindgen-swift --swift-sources / --headers / --modulemap (module navori_ffiFFI)
  3. xcodebuild -create-xcframework             → apps/macos/Generated/NavoriCoreFFI.xcframework
cargo xtask bundle [--profile dev|release]
  4. swift build (SwiftPM, arm64) + cargo build -p navori-cli
  5. lắp .app: MacOS/Navori, Helpers/navori, Info.plist (profile, bundle id, LSUIElement)
  6. codesign --options runtime: CLI trước, app sau (identity "Navori Local Signing", thiếu thì ad-hoc + cảnh báo)
cargo xtask run      # bundle dev + (re)launch "Navori Dev.app"
cargo xtask install  # bundle release → /Applications (giữ bản cũ .previous, tự rollback nếu copy lỗi)
cargo xtask ci       # fmt --check, clippy -D warnings, test, check-deps, swift build
```
- **SwiftPM thay cho Xcode project** (quyết định khi dựng R0): không cần cài XcodeGen, mọi thứ chạy từ CLI; mở `apps/macos/Package.swift` bằng Xcode vẫn có Previews/debugger. Info.plist do xtask sinh nên không có file project cần đồng bộ.
- Lưu ý khi viết doc comment trong Rust: không dùng chuỗi `*/` — UniFFI chép doc comment vào block comment Swift.

---

## 6. AI Runtime

### 6.1 `navori-llm` — chuẩn nội bộ + provider adapters

**Định dạng canonical** (không lấy format của provider nào làm chuẩn):

```rust
pub struct ChatRequest {
    pub model: ModelRef,                 // "anthropic/<model-id>", "openai/<model-id>", "deepseek/<model-id>"
    pub system: Vec<ContentPart>,
    pub messages: Vec<Message>,          // role + Vec<ContentPart>
    pub tools: Vec<ToolDef>,             // name, description, JSON Schema
    pub tool_choice: ToolChoice,
    pub reasoning: Option<ReasoningCfg>, // effort/budget — adapter map sang tham số riêng của provider
    pub max_output: Option<u32>,
    pub cache_hints: Vec<CacheHint>,     // điểm cache prompt (adapter nào không hỗ trợ thì bỏ qua)
}
pub enum ContentPart { Text, Image, ToolUse, ToolResult, Reasoning /* opaque, giữ nguyên để gửi lại */ }
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

**Adapter theo "họ giao thức"** — 4 adapter phủ gần như mọi provider:

| Adapter | Provider |
|---|---|
| `anthropic` (Messages API) | Claude |
| `openai` (Responses API) | ChatGPT / OpenAI |
| `openai_compat` (Chat Completions) | DeepSeek, xAI Grok, OpenRouter, Groq, Mistral, Together, Ollama / LM Studio, **mọi endpoint tùy chỉnh** (base_url + key) |
| `gemini` | Google Gemini |

- Tự viết adapter bằng `reqwest` + `eventsource-stream` thay vì phụ thuộc thư viện đa-provider: AI-first nghĩa là cần kiểm soát đầy đủ tính năng riêng của từng provider (prompt caching, reasoning, tool streaming, citations) — thư viện chung thường đi sau. (`genai` có thể dùng tạm phía sau trait nếu muốn bootstrap nhanh.)
- **Conformance test suite** chung cho mọi adapter: cùng một bộ kịch bản (text, tool call song song, tool result, image, cancel giữa stream, lỗi 429/5xx) chạy với fixture SSE ghi sẵn (`wiremock` + `insta`), và chạy live khi có key (`cargo xtask llm-conformance`).

### 6.2 `navori-models` — catalog & Model Router

- **Model catalog**: file dữ liệu tích hợp sẵn (`models.toml`: context window, capabilities `tools/vision/reasoning/caching`, giá input/output) + refresh từ `list_models()` + model user tự thêm. Model ID **không bao giờ hard-code** trong logic.
- **Vai trò (roles)** — user gán model cho từng vai trò trong Settings:

| Role | Dùng cho | Gợi ý |
|---|---|---|
| `chat` | hội thoại, agent nhiều bước | model mạnh nhất user chọn |
| `fast` | phân loại, tóm tắt event, quyết định proactive | model rẻ/nhanh |
| `vision` | hiểu screenshot | model có vision |
| `embedding` | memory search | bất kỳ provider có embeddings |

- Mỗi cuộc hội thoại có thể đổi model giữa chừng (lịch sử ở định dạng canonical nên chuyển provider được).
- **Fallback chain** khi 429/5xx/hết quota (vd. Claude → GPT → DeepSeek), cấu hình được.
- **Usage & budget**: ghi token + chi phí ước tính mỗi lượt vào SQLite; giới hạn theo ngày cho tác vụ proactive; hiển thị trong Settings.
- API key mỗi provider lưu Keychain (`navori-secrets`), không bao giờ vào config.toml hay log.

### 6.3 `navori-context` — Context Engine + Context Gateway

**Context Engine** dựng "tình huống hiện tại" khi cần:
- `ContextFragment` từ mọi module (PR đang chờ, awake lease, ports đang chạy…).
- Tín hiệu hệ thống qua `PlatformServices` (Swift): app foreground, window title, selected text, URL trình duyệt, idle time, **screenshot** (mặc định bật).
- Dự án đang làm: từ `navori-procs` (cwd của terminal/editor/harness đang active → repo/worktree/branch).
- Timeline gần đây từ event journal; memory liên quan từ `navori-memory`.
- Gộp theo **ngân sách token** của model đang dùng, ưu tiên theo độ liên quan.

**Context Gateway** — cổng *duy nhất* dữ liệu được rời máy. Chính sách: **thấy hết, trừ bí mật**. Bên trong là **Sensitive Data Shield** (`navori-redact`) với 4 lớp, áp dụng *trước* khi gửi lên cloud **và** trước khi ghi xuống journal/memory:

| Lớp | Cơ chế |
|---|---|
| 1. Chặn tại nguồn | **Denylist app** (1Password, Bitwarden, Passwords, Keychain Access, app ngân hàng — cấu hình được) → không lấy title/text/screenshot; cửa sổ của chúng bị **loại khỏi screenshot ngay khi chụp** (`SCContentFilter` excluding apps). **Secure Input** đang bật (đang gõ password ở bất kỳ đâu) → bỏ qua text + screenshot ở thời điểm đó. Cửa sổ private/incognito → bỏ qua. |
| 2. Chặn theo đường dẫn | AI/tool **không bao giờ đọc**: `~/.ssh/**`, `~/.gnupg/**`, `~/.aws/credentials`, `~/.config/gh/hosts.yml`, `~/.netrc`, `~/.docker/config.json`, `**/.env*`, `*.pem`, `*.key`, `*.p12`, `~/Library/Keychains/**`, file credential của harness (`~/.claude/.credentials.json`, `~/.grok/auth.json`, `~/.codex/auth.json`…). Danh sách mở rộng được. |
| 3. Phát hiện trong nội dung text | Bộ rule bí mật (kiểu gitleaks): AWS, GitHub (`ghp_`, `gho_`, `github_pat_`), OpenAI, Anthropic, Slack, Stripe, Google, JWT, khối `PRIVATE KEY`, connection string có password, `KEY=…/TOKEN=…/PASSWORD=…`, chuỗi entropy cao gần từ khóa nhạy cảm → thay bằng `[REDACTED:github_token]`. |
| 4. Phát hiện trong ảnh | Swift chạy OCR on-device (Vision) → Rust chạy cùng bộ rule lớp 3 trên text OCR → Swift **blur** vùng chứa bí mật (CoreImage) → mới gửi đi. |

- **Fail-closed**: OCR lỗi, detector lỗi, hay không xác định được app → **không gửi** phần đó (gửi phần còn lại).
- **Audit log**: mỗi request ghi *loại* dữ liệu đã gửi + provider + số mục đã redact (không ghi nội dung) → "AI đã thấy gì về tôi hôm nay?".
- **Kiểm thử**: corpus bí mật giả (fixture) + test "không token nào lọt" chạy trong CI; regression test cho từng rule.
- Có công tắc tạm dừng quan sát ("Pause AI vision" 15 phút / 1 giờ) ngay trên menubar.

### 6.4 `navori-agent` — Agent Runtime

```
user/trigger ─► assemble context ─► Model Router ─► stream ─┬─► text → Swift
                                                             └─► tool call ─► Policy ─┬─ Read → chạy ngay
                                                                                      ├─ Write → theo allowlist / hỏi
                                                                                      └─ Destructive → luôn hỏi (approval UI)
                                                                     ▼
                                                         CommandRouter / MCP client / built-in tool
                                                                     ▼
                                                              tool result ─► loop (giới hạn bước)
```

- **Tool registry** = `ActionSpec` của mọi module (tự động) + tool MCP bên ngoài + built-in (`memory.search`, `memory.remember`, `context.snapshot`, `web.fetch`, `open_url`).
- **Policy engine**: quyết định theo `Risk` + allowlist do user cấp ("luôn cho phép `ports.kill` với project X"). Approval là một `AssistantEvent::ApprovalRequired` → Swift hiện nút Approve/Deny → `resolve_approval`.
- **Session**: lưu SQLite (messages canonical, tool calls, usage); cancel được giữa chừng; giới hạn số bước & token mỗi lượt.
- **Surfaces**: chat trong panel menubar · **quick-ask** bằng global hotkey (NSPanel kiểu Spotlight, kèm context hiện tại) · `navori ask "…"` từ terminal · notification có nút hành động.

### 6.5 `navori-memory`
- Mọi thứ ghi vào memory đều đã qua `navori-redact` — bí mật không bao giờ nằm trong DB.
- **Facts/preferences** ("tôi dùng squash merge", "repo X là công việc") — AI ghi qua tool `memory.remember`, user xem/xóa trong Settings.
- **Episodic**: model `fast` tóm tắt journal theo giờ/ngày → nhật ký hoạt động ngắn.
- **Embeddings** cho tìm kiếm ngữ nghĩa: trait `VectorIndex`; khởi đầu bằng brute-force cosine trong Rust (quy mô cá nhân, hàng chục nghìn vector vẫn nhanh), sau đổi sang `sqlite-vec` (hiện còn alpha) mà không đổi API.

### 6.6 `navori-triggers` — Proactive
- Rule trên EventBus, ví dụ: CI fail trên PR của tôi → tóm tắt log lỗi · có review request mới → tóm tắt diff · phiên Claude Code kết thúc → báo + thả awake · port xung đột khi start dev server.
- Pipeline: event → rule match (rẻ, không gọi AI) → model `fast` quyết định "có đáng làm phiền không" → nếu có, agent `chat` làm việc → notification.
- **Budget** token/ngày, **quiet hours**, rate-limit mỗi rule, nút "đừng báo loại này nữa".

### 6.7 `navori-mcp` (`rmcp`)
- **MCP client**: user thêm MCP server (Linear, Notion, filesystem…) → tool của chúng vào Tool Registry, mặc định `Risk::Write` trừ khi user hạ xuống.
- **MCP server**: `navori mcp` (stdio) → Claude Code/Cursor gọi được tool của Navori ("port nào đang chạy ở worktree này?", "giữ máy thức tới khi tôi xong", "PR nào đang chờ tôi?"). Cùng policy engine với agent nội bộ.

---

## 7. Dữ liệu & lưu trữ

| Loại | Vị trí | Công nghệ |
|---|---|---|
| Config | `~/Library/Application Support/Navori/config.toml` | `serde` + `toml`, section theo module/AI; đọc khi khởi động (file sai → chạy với mặc định + báo lỗi). Hot-reload bằng `notify`: chưa làm, thêm khi module đầu tiên cần |
| State, cache, journal, AI sessions, memory, usage | `~/Library/Application Support/Navori/navori.db` | SQLite WAL, `rusqlite` (bundled) + `rusqlite_migration`; bảng prefix theo owner (`github_*`, `ai_session_*`, `memory_*`) |
| Secrets (GitHub token, API key các provider) | Keychain, service `dev.navori` | `keyring` (apple-native). Dev: `NAVORI_SECRET_STORE=file` (0600) |
| Logs | `~/Library/Logs/Navori/` | `tracing` + `tracing-appender`; không log nội dung prompt trừ khi bật debug |
| Socket | `~/Library/Application Support/Navori/run/navori.sock` (dự phòng `/tmp/navori-<uid>/<profile>.sock` khi đường dẫn vượt 103 byte) | thư mục 0700, socket 0600, peer UID == UID chủ socket; socket cũng là khóa single-instance |

> **Chữ ký ổn định là bắt buộc** — dù không có Apple Developer trả phí. Ký ad-hoc thì mỗi lần build mã băm đổi → macOS coi là app mới: Keychain hỏi mật khẩu lại, quyền Accessibility / Screen Recording bị mất. Giải pháp: một **chứng chỉ code-signing tự ký** (`Navori Local Signing`, tạo một lần bằng `cargo xtask signing-setup`, hạn 10 năm) dùng cho **mọi** bản build. Designated requirement = bundle ID + chứng chỉ → giữ nguyên qua các lần build.
>
> Bản dev dùng bundle ID riêng (`dev.navori.app.debug`) và socket/DB riêng → không đụng tới bản bạn đang dùng hằng ngày.

---

## 8. Dịch vụ dùng chung: `navori-procs`

Awake, Ports và Context Engine đều cần biết "process này thuộc dự án nào, do ai chạy":
- **ProcessTable** (impl trong `navori-macos`: libproc + `sysctl KERN_PROCARGS2`): pid, ppid, uid, exe, argv, cwd, start time.
- **ProjectResolver**: cwd → `.git` → repo root; `.git` là file → **worktree** → `{project, worktree, branch}`.
- **HarnessCatalog**: nhận diện tổ tiên là `claude`, `codex`, `grok`, `gemini`, `opencode`, `cursor-agent`… (cấu hình được) và terminal host.
- **ProcWatcher**: `kqueue EVFILT_PROC/NOTE_EXIT` → biết process chết ngay, không poll.

---

## 9. Feature — Awake (`navori-mod-awake`)

**Lease model**:
```rust
struct Lease { id, source: Manual | Timer | Process{pid} | Agent{harness, session_id, pid} | Command,
               mode: System | Display, expires_at: Option<_>, grace: Duration }
```
- **Reconciler**: có ≥ 1 lease → giữ đúng một IOKit assertion (`PreventUserIdleSystemSleep`, + `PreventUserIdleDisplaySleep` nếu cần). Idempotent.
- Lease kết thúc khi: release · hết TTL · process chết (kqueue) · app thoát (OS tự thu hồi assertion).

**Tích hợp harness — đã kiểm chứng trên máy (2026-11-01)**: cả ba harness đều có hook kiểu Claude Code (JSON qua stdin, có `hook_event_name` + session id + `cwd`). Vì vậy chỉ cần **một** lệnh `navori hook` cho tất cả.

| Harness | Hook | acquire | release sau grace | release ngay |
|---|---|---|---|---|
| Claude Code | `~/.claude/settings.json` | `UserPromptSubmit` | `Stop` | `SessionEnd` |
| Grok Build (`grok` 1.0.70) | `~/.grok/hooks/navori.json` | `UserPromptSubmit` | `Stop` (chỉ khi `reason == "end_turn"`), `StopCancelled`, `StopFailure` | `SessionEnd` |
| Codex (`codex-cli` 0.166) | `~/.codex/hooks.json` | `UserPromptSubmit` | `Stop` | `SessionEnd` |
| Bất kỳ lệnh nào | — | `navori awake run -- <cmd>` | — | khi lệnh thoát |
| Harness chưa có hook | — | auto-detect qua HarnessCatalog | im lặng N phút | process chết |

Chi tiết quan trọng:
- **Nhận diện harness**: `navori hook` tự xác định nguồn (biến môi trường `GROK_*` của Grok runner, rồi đi ngược cây process tìm `claude` / `grok` / `codex`) → lấy PID thật của harness để kqueue theo dõi. Không cần tham số.
- **Chống đếm trùng**: Grok **cũng đọc hook trong `~/.claude/settings.json`** (chế độ tương thích Claude) → cùng một sự kiện có thể tới hai lần. Mọi thao tác lease là **idempotent theo `(harness, session_id)`**, nên trùng không gây hại.
- **"Stop" ≠ "xong việc"**: harness có thể còn background task (bash nền, subagent, long-running task) sau khi lượt kết thúc. Trước khi thả lease hết grace, Awake kiểm tra **cây process con của harness**; còn con đang chạy/tốn CPU thì gia hạn.
- **Cài đặt**: Settings → Integrations có nút Install/Repair/Remove cho từng harness. Navori **merge** vào file cấu hình hiện có (backup trước khi ghi, không đè hook khác), và `navori doctor` kiểm tra hook còn đúng không.
- **Lưới an toàn**: hook lỗi hoặc không chạy → kqueue (process chết) + TTL vẫn đảm bảo lease không treo vĩnh viễn.

AI tools: `awake.status` (Read), `awake.hold{duration, reason}` (Write), `awake.release` (Write).
Ngoài phạm vi (đã quyết định): giữ máy chạy khi gập nắp.

---

## 10. Feature — Ports (`navori-mod-ports`)

- **Scan** bằng libproc (không gọi `lsof`): pid của UID hiện tại → socket fd → TCP `LISTEN` + port. Thường < 50 ms.
- **Làm giàu** qua `navori-procs`: project · worktree · branch · harness · loại (dev server, database, docker).
- **Hiển thị**: Project → Worktree → Process → Ports.
- **Hành động**: mở `localhost:port` · copy · reveal trong Finder · kill một / theo worktree / project / tất cả. Kill = `SIGTERM` process group → 3s → `SIGKILL`. Không đụng UID khác; danh sách `protected` loại khỏi "kill all".
- Polling 3s khi panel mở, 30s ở nền.
- AI tools: `ports.list` (Read), `ports.kill{pid|worktree|project}` (**Destructive**), `ports.open` (Read).

---

## 11. Feature — GitHub PRs (`navori-mod-github`)

- **Inbox**: Needs my review · My PRs · Involved. Mỗi PR: repo, #, title, draft, review decision, CI rollup, mergeable, labels, updated.
- **Auth — OAuth App + Device Flow** (quyết định dài hạn, không phải tạm thời; xem lý do ở §15). Scopes: `repo`, `read:org`, `notifications`, `workflow`. Không client secret, không redirect server — giống cách `gh auth login` hoạt động. Token → Keychain.
  - Hỗ trợ nhiều account (cá nhân + công ty) ngay từ đầu: token key theo `account_id`.
  - Org bật SAML SSO: hướng dẫn authorize token cho org ngay trong app; org chặn OAuth App: gửi yêu cầu duyệt, hoặc dùng PAT (classic) làm phương án dự phòng.
  - Trait `TokenProvider` (`OAuthDevice`, `ClassicPat`) giữ chỗ cho cách khác mà không đổi module.
- **Data**: một GraphQL request (alias 3 search query) qua `reqwest` + `graphql_client`; theo dõi rate-limit header; cache SQLite → mở app thấy ngay.
- **Phát hiện thay đổi nhanh và rẻ**: poll **Notifications API** với `If-Modified-Since` và tôn trọng `X-Poll-Interval` (304 không tốn quota) → chỉ khi có thông báo mới mới chạy lại GraphQL. Đây là lý do cần OAuth App: theo docs GitHub, các endpoint notifications chỉ hỗ trợ token kiểu classic (OAuth/PAT classic), không hỗ trợ GitHub App user token hay fine-grained PAT.
- `domain::diff(prev, next) → Vec<GithubEvent>` → notification + journal + **Triggers**.
- AI tools: `github.inbox` (Read), `github.pr.get{diff, checks, comments}` (Read), `github.pr.summarize` (Read), `github.pr.comment` / `approve` (Write), `github.pr.merge` (**Destructive**), `github.checks.rerun` (Write).
- Kịch bản AI-first: "Tóm tắt 3 PR đang chờ tôi, cái nào nhỏ nhất?" · "CI PR #42 fail vì gì?" · trigger tự tóm tắt lỗi CI.

---

## 12. Tech stack

| Mảng | Lựa chọn |
|---|---|
| **Rust core** | |
| Async | `tokio`, `tokio-util`, `futures` |
| Errors | `thiserror` (lib), `anyhow` (biên) |
| Serde / Schema | `serde`, `serde_json`, `toml`, `schemars` |
| Time / ID | `time` (hoặc `jiff`), `ulid` |
| Logging | `tracing`, `tracing-subscriber`, `tracing-appender` |
| Storage | `rusqlite` (bundled), `rusqlite_migration`; (sau) `sqlite-vec` |
| Secrets | `keyring` (apple-native) |
| HTTP / SSE | `reqwest` (rustls), `eventsource-stream` |
| GitHub | `graphql_client` |
| MCP | `rmcp` |
| macOS FFI | `objc2-io-kit`, `libproc`, `libc` |
| FFI bridge | `uniffi` 0.32 |
| IPC / CLI | tokio `UnixListener` + JSON-RPC 2.0, `clap` |
| **Swift shell** | |
| UI | AppKit (`NSStatusItem`, `NSMenu`, `NSPanel`) + SwiftUI, `@Observable` |
| Markdown (chat) | renderer SwiftUI (vd. swift-markdown-ui) |
| Hotkey | `KeyboardShortcuts` (sindresorhus) |
| Hệ thống | `UserNotifications`, `ScreenCaptureKit`, `Vision` (OCR), `CoreImage` (blur), `ApplicationServices` (AX), `ServiceManagement` (SMAppService) |
| Cập nhật | `cargo xtask install` (build từ source); Sparkle 2 chỉ khi có Developer ID |
| Project | SwiftPM (`Package.swift`) + `cargo xtask bundle` |
| **Chất lượng** | `cargo-nextest`, `insta`, `wiremock`, `proptest`; Swift Testing cho `CoreStore`; `cargo-deny`; `[workspace.lints]` (`unsafe_code = "forbid"` trừ `macos`, `ffi`) |

---

## 13. Chất lượng & vận hành

- **Testability**: IO hệ thống & mạng sau trait (`PowerManager`, `ProcessTable`, `GithubApi`, `LlmProvider`, `PlatformServices`) → test bằng fake. Có `FakeLlm` kịch bản hóa (trả text/tool call theo script) để test agent loop không tốn token.
- **AI evals**: bộ kịch bản (`xtask eval`) — "kill port của worktree X" phải gọi đúng tool + đúng tham số + xin approval — chạy trên nhiều model để biết model nào dùng được cho vai trò nào.
- **Diagnostics**: `navori status`, `navori doctor` (socket, Keychain, quyền AX/Screen Recording, hook của từng harness, token GitHub, key từng provider, rate limit), `navori logs`, `navori ai usage`.
- **Ngân sách hiệu năng**: idle CPU ≈ 0% · cold start < 300 ms · mở panel < 100 ms · RSS mục tiêu < ~60 MB khi idle.
- **Dữ liệu bền vững**: migration SQLite có version, **tự backup DB trước mỗi migration**, rollback khi lỗi; config sai → dùng giá trị cũ + báo lỗi, không crash.
- **Phát hành (hiện tại — không cần Apple Developer trả phí)**: `cargo xtask install` = build release → ký bằng chứng chỉ tự ký (hardened runtime, ký `Helpers/navori` trước rồi tới app) → `codesign --verify --strict` → thay `/Applications/Navori.app` an toàn (thoát app cũ, giữ bản cũ làm `.previous` để rollback) → mở lại. Cập nhật = `git pull && cargo xtask install`. Login item qua SMAppService, dự phòng LaunchAgent trong `~/Library/LaunchAgents` nếu SMAppService từ chối chữ ký tự ký.
- **Phát hành (khi phân phối cho người khác)**: cùng pipeline, đổi `signing.identity` sang Developer ID → bật thêm notarize + staple + DMG + Sparkle appcast (`cargo xtask release`). Không đổi kiến trúc.
- **Crash**: panic hook Rust + crash marker; lần mở sau hiện "Navori vừa gặp lỗi" kèm nút mở log.

### Definition of Done — điều kiện để một feature được release

Một feature chỉ được merge vào release khi **tất cả** điều sau đúng:

1. **Đủ chức năng** theo đặc tả của feature trong tài liệu này — không có "để bản sau".
2. **Mọi đường lỗi được xử lý & hiển thị**: offline, hết quota, token hết hạn, thiếu quyền macOS → UI nói rõ lý do + cách sửa (không spinner vô hạn, không lỗi im lặng).
3. **Settings UI đầy đủ** cho mọi tùy chọn của feature; không bắt user sửa file tay.
4. **Onboarding**: lần đầu dùng feature được dẫn qua quyền/đăng nhập cần thiết.
5. **AI-ready**: action có schema + mô tả cho LLM + Risk; `context()`; event có `summary()`; có eval case.
6. **Test**: unit (domain), integration (actor với fake), conformance/fixture cho IO; CI xanh.
7. **Quan sát được**: `tracing` đủ để debug từ log; mục riêng trong `navori doctor`.
8. **Hiệu năng** nằm trong ngân sách ở trên (đo, không đoán).
9. **Tài liệu** người dùng ngắn trong `docs/features/<feature>.md`.
10. Đã tự dùng thật ≥ 3 ngày (dogfood) không có lỗi chặn.

---

## 14. Lộ trình — mỗi release là một feature hoàn chỉnh

Không chia v1/v2. Mỗi release thêm **một** năng lực hoàn chỉnh theo Definition of Done; release xong là dùng hằng ngày được luôn.

| Release | Bạn dùng được ngay | Bao gồm |
|---|---|---|
| **R0 — Foundation** *(nền móng, không phải feature)* | App menubar ký ổn định, cài/cập nhật bằng một lệnh, chạy cùng máy khởi động, Settings, `navori` CLI + `doctor` | workspace Rust + SwiftPM; kernel, store, config, logging, redact; `navori-ffi` + `CoreStore`; IPC; `xtask signing-setup` / `install` |
| **R1 — Awake** | Giữ máy thức thủ công/hẹn giờ/theo lệnh; tự giữ & tự thả theo phiên Claude Code, Grok, Codex; cài hook một nút | lease, IOKit, kqueue, `navori hook`, Integrations UI |
| **R2 — AI Assistant** | Chat trong menubar + quick-ask hotkey với **bất kỳ model nào** (Claude, ChatGPT, DeepSeek, Gemini, Grok, OpenRouter, endpoint tùy chỉnh); điều khiển Awake bằng lời | 4 adapter + conformance, catalog + roles + fallback + usage/budget, agent loop + policy + approvals, Context Engine/Gateway + Sensitive Data Shield, memory |
| **R3 — Ports** | Xem mọi port theo project/worktree/harness, mở/kill từng cái hoặc tất cả; hỏi AI "dọn port của worktree đã merge" | libproc, `navori-procs`, kill có approval |
| **R4 — GitHub** | Inbox PR (cần review / của tôi / liên quan), CI, approve/merge, thông báo; AI tóm tắt PR & lỗi CI | device flow, multi-account, notifications polling, GraphQL, cache |
| **R5 — Proactive** | AI tự lên tiếng khi đáng: CI fail, review mới, phiên agent xong, port xung đột | triggers, budget, quiet hours |
| **R6 — MCP** | Claude Code/Grok/Codex dùng tool của Navori; Navori dùng MCP server ngoài | `navori-mcp` client + server |

**Vì sao AI ở R2 chứ không ở cuối**: AI là lõi (P1). Đặt sớm để hợp đồng `Action` / `context()` / `Event` được kiểm chứng bằng agent thật trước khi viết Ports và GitHub — hai feature đó sinh ra đã là tool của AI. Quan sát màn hình (screenshot + OCR) nằm trong R2 vì Sensitive Data Shield là điều kiện bắt buộc để release AI.

---

## 15. Quyết định đã chốt

| # | Quyết định | Lý do |
|---|---|---|
| D1 | **UI**: Swift (AppKit + SwiftUI) + Rust core qua UniFFI | Native nhất cho macOS, logic vẫn ở Rust. |
| D2 | **AI**: model cloud, mọi provider, user chọn theo vai trò | AI-first, không khóa vào một hãng. |
| D3 | **GitHub**: OAuth App + Device Flow (dài hạn) | (a) Notifications API — cách phát hiện thay đổi rẻ nhất — chỉ hỗ trợ token classic, không hỗ trợ GitHub App user token. (b) Truy cập mọi repo bạn có quyền, không cần cài app vào từng org (GitHub App bị giới hạn ở repo đã cài, kể cả kết quả search). (c) Không cần server: GitHub App chỉ thật sự vượt trội khi có webhook, mà app local không nhận webhook. Đổi lại: scope `repo` rộng hơn và token không hết hạn → giữ trong Keychain, có nút revoke. Đây cũng là mô hình của `gh` CLI. |
| D4 | **CPU**: chỉ Apple Silicon (`aarch64-apple-darwin`) | Build nhanh hơn, binary nhỏ hơn, không cần test Intel. |
| D5 | **Harness**: Claude Code, Grok Build, Codex qua hook; còn lại auto-detect | Đã kiểm chứng hook của cả ba trên máy (2026-11-01). |
| D6 | **Gập nắp vẫn chạy**: không hỗ trợ | Cần privileged helper chạy root — không đáng. |
| D7 | **Quyền quan sát của AI**: thấy hết, trừ bí mật | Sensitive Data Shield 4 lớp, fail-closed, có audit log và nút tạm dừng. |
| D8 | **Ký app**: chứng chỉ code-signing tự ký, chưa cần Apple Developer trả phí | App chỉ chạy trên máy bạn, build từ source → không bị Gatekeeper chặn (không có cờ quarantine). Mọi API cần dùng (IOKit, libproc, AX, ScreenCaptureKit, Keychain thường, notifications) không cần entitlement trả phí; app không sandbox. Mua Developer ID chỉ khi muốn phân phối. |
