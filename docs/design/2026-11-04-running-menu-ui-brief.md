# UI brief: Running menu (Awake and Ports in one menu)

> Locked 2026-11-04 via `draft-ui`. Screenshot: [`2026-11-04-running-menu-ui-brief.png`](2026-11-04-running-menu-ui-brief.png). It shows real local data from the lock date.
> Replaces Surface 3 (for the Holding list) and Surface 4 of [`2026-11-01-menubar-ui-brief.md`](2026-11-01-menubar-ui-brief.md). Waits for the ROAD-18 spec that adopts it. Nothing here is built yet.

## Decision

**One level-0 row, `Running`, with one project list. Each project is a dot and a name; every detail is in its child menu.** Hybrid of direction B (project rows), reduced over two review rounds. Chosen over A (a flat list of sessions and ports) and C (idle ports first), and over separate Awake and Ports menus.

The user's words, in order (Vietnamese verbatim, with an English translation):
- "tôi đang muốn có cách nào kết hợp awake và port vaof chung 1 menu để có thể quản lý chung? sẽ tối ưu hơn thành 2 menu - nó làm duplicated với các session đã và dang chạy trên awake." (EN: "I want a way to combine awake and ports into one menu to manage them together. That is better than 2 menus, which duplicate the sessions already running in awake.")
- "đồng ý với layout của B - nhưng menu list project cần gọn hơn … cần hiện đại và clean hơn … cần show thông tin thông minh hơn, các chi tiếc thi đi vào trong menu child để xem chi tiếc" (EN: "agreed on B's layout, but the project list must be tidier … more modern and clean … show smarter information, and details go in the child menu")
- "Tôi nghỉ không cần show gì, chỉ cần dot show status thôi. button running bên ngoài cũng ko cần show 4 idle, cũng chỉ cần show có đang chạy hay không thôi." (EN: "I don't think it needs to show anything, just a dot for status. The Running button outside doesn't need to show 4 idle either, only whether something is running.")
- "chốt" (EN: "locked")

## Grounding

- **Token source of record:** macOS system semantics, as in the 2026-11-01 brief (Grounding, Type & color). No new color.
- **Kit:** `MenuRowView`, the panel cascade, the status dot, `AwakeSubmenuView` headers, `HarnessMarkImage`.

## Job

- **Question:** Is anything running on this Mac that nobody is working in?
- **Finding:** on the lock date, `openklynt-main` and `Other` read as a ring: 4 ports with no session. `openklynt-main` had been up 2 days 20 hours. The ring is the finding; this menu states it by shape, not in words.
- **Primary job:** free those ports. Hover the ringed row, press `Stop 3`, confirm.

## Signature

**A dot and a name.** The list carries no counts, ages, port numbers or status words.

## Surface 1: Level-0 row

- **Layout:** the Awake row becomes `Running`: icon, name, dot, chevron. No text value.
- **Components:** `rung 2 — MenuRowView`; `rung 2 — status dot`.
- **States:** something running (green dot: any session or any listed port) · nothing running (no dot) · record missing or module stopped (no submenu, as the Awake row today).
- **Type & color:** as Surface 1 of the 2026-11-01 brief.
- **A11y:** VoiceOver reads `Running, active` or `Running, nothing running`. The dot no longer means the Mac is held awake; the status icon still says that.

## Surface 2: Project list (submenu)

- **Layout:** `Keep awake` (opens its third level) · `Tracking agents` · a problem line only when something is wrong · separator · project rows · separator · `Stop all ports…`. Rows are 26pt, in a panel about 272pt wide. Order: projects with no session first, then by case-insensitive name, `Other` last.
- **Components:** `rung 2 — MenuRowView` with a 14pt lead for the dot; `new (rung 7) — ring variant of the status dot`, because state must differ in shape as well as color.
- **States:**
  - Filled green dot: at least one session is working.
  - Orange ring: nobody is working here. `Other` follows the same rule.
  - Hover on a ringed row with ports: `Stop N` appears, then an inline confirmation. A row with a working session has no `Stop`; it is in the child menu.
  - Nothing running: `Nothing running`, and no `Stop all ports…`.
  - `Stop all ports…` always confirms.
- **Type & color:** name 14pt regular. Color is only the dot. The row carries no secondary text.
- **A11y:** VoiceOver reads the state the dot shows, for example `klynt, working` and `openklynt-main, 3 idle ports`. ↑ and ↓ move through rows, → opens the child, and Tab reaches `Stop N`.

## Surface 3: Project child menu

- **Layout:** about 330pt wide. Header: project name and path in mono, and `up 2d 20h` when the project has no session. `Sessions` section: Harness mark, title, dot, `branch · state`. `Ports` section: kind mark, process or container name, `:port` right-aligned. Then `Stop N ports`. The child of `Other` also says `N system ports hidden`.
- **Components:** `rung 2 — MenuRowView`; `rung 2 — HarnessMarkImage`; the confirmation is `new (rung 7) — inline confirmation`.
- **States:** hover on a port row swaps the port for `Open` (web only) · `Copy` · `Stop` · `Stop` running · `Stop` failed (the row says why) · a section with no rows is left out.
- **Type & color:** as Surface 1 of the 2026-11-01 brief. SF Mono 11.5pt for ports and the path. A stopped session is dimmed.
- **A11y:** each row reads as one sentence. The state word stays in the session row, so it is never color alone.

## Amendments (decided in review)

1. One menu. Awake and Ports stay separate Modules (ARCH-3); the Shell joins their state by project.
2. Layout B, not a flat list.
3. No secondary line, no capsules, no counts, no ages and no port numbers in the project list.
4. Status is a dot only, on project rows and on `Running`.
5. Details live in the child menu.
6. `Running` is the row's name.

## What the draft showed about the data

Read from this Mac on the lock date. These are inputs for ROAD-17 and ROAD-18, not decisions.

- Docker's 8 ports all belong to one `com.docker.backend` process. Signalling that process group would stop Docker itself, so containers need their own read and stop path.
- The Compose project label put every container under `klynt`. Three containers point at the old worktree `feat-pinwin-lane1`.
- `kimi-webbridge` falls under `klynt` only because of its working directory, so `Stop 9 ports` would stop it. This needs a `protected` list or a stricter rule for joining a project.

## Open questions for `design-solution`

Agent proposals, not reviewed by the user:

- **`Held by you` and `Commands`:** the draft left out these Holding groups. Proposal: `Keep awake` shows the manual hold, and a command Lease joins its project by working directory, else `Other`.
- **A project with only stopped sessions:** proposal: a ring.
- **Join key:** the Lease label's project text today, so two checkouts with the same directory name share a row.
- **Spec conflicts to amend:** AWAKE-11.7 (menu text about Leases only), AWAKE-11.13 (Holding groups), AWAKE-11.15 and AWAKE-11.17, and Amendment 5 of the 2026-11-01 brief.
- **Rename:** whether `Awake` stays the name of the Module, Actions and CLI while the menu says `Running`. Proposal: yes, only the row changes.
- **GOAL-11:** the menu reads state already scanned and never scans on open (AWAKE-11.10). The scan cost with the Docker read is unmeasured.
