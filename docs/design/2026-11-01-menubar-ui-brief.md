# UI brief — Navori menubar (level 0 + feature submenus)

> Locked 2026-11-01 via `draft-ui`. Screenshot: [`2026-11-01-menubar-ui-brief.png`](2026-11-01-menubar-ui-brief.png).
> **Provisional by design:** từng feature (R1 Awake, R3 Ports, R4 GitHub, R2/R5 AI) sẽ cập nhật section của mình khi được triển khai; layout tổng và các quy tắc dưới đây là cố định.

## Decision

**Hướng A — "Menu" (NSMenu + cascading submenus), bản A5.** Chọn A thay vì B (Triage tabs) và C (Cockpit theo dự án), rồi tinh chỉnh qua 4 vòng review.

Lời người dùng (nguyên văn, theo thứ tự):
- "Tôi muốn đi theo hướng của A — nhưng A chưa đủ tốt, chưa đủ clean và modern"
- "icons … cần đơn sắc để đồng bộ với macos hơn"
- tham chiếu RepoBar: "vẫn cần tối ưu hơn nữa, đẹp hơn, gọn gàng hơn, dễ dùng hơn"
- "phần awake, ports, PR cần đơn giản hơn, 1 dòng với info quan trọng only — không cần switch account github"; "trong trường hợp nhiều PR, nhiều repo … cần hiển thị rõ hơn"
- "Bỏ dòng đầu … needs you … tương lai sẽ là nơi hiển thị notification từ AI, thông tin, to do … needs you ở đây là quyết định của tôi và AI, không phải là PR của git"
- "Chốt, tạm thời như thế — khi triển khai features sẽ update thêm."

## Grounding

- **Token source of record:** macOS system semantics — `labelColor`, `secondaryLabelColor`, `tertiaryLabelColor`, `separatorColor`, `controlAccentColor` / `selectedContentBackgroundColor`, `systemGreen/Red/Orange/Blue`; system menu material. Repo chưa có `docs/standards/design-tokens.md`; không đề xuất token riêng.
- **Kit hiện có** (`apps/macos/Navori`): `Banner`, `SectionTitle`, status dot (`Circle().fill`), borderless buttons. Ladder dùng bên dưới: `rung 1 — <native control>` = AppKit/SwiftUI có sẵn; `rung 2 — <kit piece>` = thành phần Navori đã có; `new (rung 7)` = thành phần mới, kèm lý do.

## Job (màn hình làm việc)

- **Question:** Có gì cần tôi quyết định ngay, và máy tôi đang ở trạng thái nào?
- **Finding:** (dữ liệu ngày lock) "2 quyết định đang chờ: dừng 3 server Claude để không trong openklynt-main; giữ máy thức cho Codex trong cargo-zip" — nằm ở đầu menu, dòng đầu tiên dưới ô Ask.
- **Primary job:** trả lời quyết định đầu tiên bằng nút inline ngay trên dòng đó (`Stop 3` / `Keep`) — một click, không mở thêm gì.

## Signature

**Mọi chi tiết cách một lần hover** — menu cấp 0 chỉ gồm *Ask → Needs you (inbox) → 3 dòng feature một dòng*, mọi thứ còn lại nằm trong submenu macOS thật, lật sang trái khi sát mép màn hình, tối đa 3 cấp.

---

## Surface 1 — Menu cấp 0

- **Layout:** rộng 360pt, padding 5pt. Thứ tự cố định:
  1. Ô **Ask Navori…** (`⌥ Space`) — không có dòng tiêu đề/caption phía trên.
  2. Separator → header **Needs you** + tổng số (căn phải).
  3. Tối đa **3** mục inbox (xem quy tắc inbox), rồi `Show all N ›` khi N > 3.
  4. Separator → 3 dòng feature, **mỗi dòng 1 dòng**: icon · tên · *một* thông tin quan trọng (căn phải) · `›`
     - **Awake** — `● On · Claude, Codex` (chấm xanh khi bật; `Off` khi tắt)
     - **Ports** — `17 open`
     - **Pull Requests** — `1 to review · ● 2 failing` (phần failing chỉ hiện khi > 0)
  5. Separator → `Settings… ⌘,` · `About Navori` · `Quit Navori ⌘Q`.
  - Không có account, API quota, hay dòng "updated" ở cấp 0.
- **Components:**
  - Menu + submenu cascade: `rung 1 — NSMenu / NSMenuItem.submenu` (cần spike, xem Open questions).
  - Hàng tùy biến: `new (rung 7) — MenuRowView` (SwiftUI trong `NSMenuItem.view`): lead 20pt, title, end value, chevron, tối đa 2 dòng phụ, vùng action inline. Lý do: NSMenuItem chuẩn không có dòng phụ / nút inline.
  - Ô Ask: `rung 1 — NSSearchField` trong item view.
  - Inbox row: `new (rung 7) — InboxRowView` (xem Surface 2).
  - Status dot: `rung 2 — status dot`.
  - Icon: `rung 1 — SF Symbols` (monochrome rendering): `sparkle`, `cup.and.saucer`, `point.3.connected.trianglepath.dotted`, `arrow.triangle.pull`, `gearshape`, `bell`, `square`.
- **States:** default · hover/selected (nền accent bo 8pt, chữ + icon trắng) · inbox rỗng (`✓ You're all caught up`, menu ngắn lại) · core không chạy (`rung 2 — Banner` style error ở vị trí inbox: "Navori core isn't running" + `Restart`) · GitHub offline (`Pull Requests  offline · 5 min ago`) · Awake off (`Off`, không chấm).
- **Type & color:** SF Pro 14pt title/item, 13pt giá trị căn phải (`secondaryLabelColor`), 12pt dòng phụ. SF Mono 11.5pt cho số port, `#PR`, pid. Icon đơn sắc `secondaryLabelColor` (trắng khi selected). Màu ngữ nghĩa **chỉ** là chấm 7pt (`systemGreen` on/pass, `systemRed` fail, `systemOrange` conflict, `systemBlue` review/decision). Không có ô màu, gradient, hay icon nhiều màu.
- **A11y:** điều hướng bàn phím kiểu NSMenu (↑↓, → mở submenu, ← đóng, Return, Esc); VoiceOver đọc gộp "title, dòng phụ, thời gian"; trạng thái không chỉ bằng màu (luôn có chữ: "2 failing", "On"); tôn trọng Reduce Transparency (material đục) và Reduce Motion; vùng bấm nút inline ≥ 24pt cao.

## Surface 2 — Needs you (inbox) — trên cấp 0 và submenu `Show all`

Inbox là **nơi quyết định chung giữa người dùng và Navori AI** — *không* phải danh sách PR (PR chỉ nằm ở Surface 5).

- **Layout:** mỗi mục: lead (20pt) · title (1 dòng, cắt …) · tuổi căn phải · dòng phụ = *lý do* (1 dòng) · [Decision] hàng nút inline.
  - Thứ tự: **Decision → To-do → Update**; trong cùng loại, mới nhất trước (Decision hết hạn sớm nhất trước).
  - Cấp 0: tối đa 3 mục; submenu `Show all N`: header `Needs you N`, segmented **All · Decisions n · To-dos n · Updates n**, danh sách đầy đủ, ô **Add a to-do…**, nút inline **Clear updates** · **History**.
- **Loại mục:**

  | Loại | Lead | Hành vi |
  |---|---|---|
  | **Decision** (AI hỏi, người quyết) | `sparkle` | 1 nút primary + 1 nút phụ inline (vd. `Stop 3` / `Keep`, `Keep 1 h` / `Let it end`). Hành động `Risk::Destructive` hiện chú thích "needs your approval" và **không bao giờ** tự chạy. Trả lời xong → mục biến mất, kết quả thành một Update. |
  | **To-do** (người hoặc AI tạo) | ô vuông bấm được | Tick → hoàn thành (biến mất, có Undo trong History). Dòng phụ: hạn + ai tạo ("Due today · added by you", "Suggested by Navori AI"). |
  | **Update** (thông báo) | `bell`, mờ hơn | Không có nút; click mở ngữ cảnh. Tự ẩn sau khi xem / `Clear updates`. |

- **Components:** `new (rung 7) — InboxRowView` (lead, title, age, reason, action slot); nút inline: `new (rung 7) — CapsuleButton` (primary = accent, secondary = fill) — lý do: NSButton chuẩn không có kiểu capsule nhỏ trong menu; segmented: `rung 1 — NSSegmentedControl`; ô thêm to-do: `rung 1 — NSTextField`.
- **States:** rỗng · nhiều (N > 3 → `Show all N`) · nút đang chạy (spinner trong nút, các nút khác disabled) · hành động lỗi (dòng phụ đổi thành lỗi + `Retry`) · Decision hết hạn (tự chuyển thành Update "expired").
- **Type & color:** title 14pt regular (Update nhạt hơn: lead `tertiaryLabelColor`); lý do 12pt `secondaryLabelColor`; nút 12.5pt medium cao 24pt; khi row selected: nút primary đảo thành nền trắng chữ accent, nút phụ nền trắng 22%.
- **A11y:** nút inline nằm trong thứ tự Tab của row; VoiceOver đọc loại mục ("Decision: Stop 3 idle servers…, needs your approval"); phím tắt: Return = nút primary, ⌘⌫ = dismiss.

## Surface 3 — Awake submenu (+ Agent sessions, Integrations)

- **Layout:** lệnh nhanh (`Keep awake for 1 hour`, `Keep awake until I turn it off`, `Let my Mac sleep now`) → **status block** `Awake · since 11:38` + "3 leases · system sleep blocked · display may sleep" với nút inline `Pause 15m · Display on · Release all` → header **Holding**: mỗi lease 2 dòng (`Claude · NAVORI/main` / `working · pid 23719`, thời gian căn phải; hover → nút `Release`) → `Agent sessions [6] ›`, `Integrations [3] ›`.
  - Cấp 3 **Agent sessions:** mọi phiên harness đang mở (claude/codex/grok), trạng thái, thời gian; hover → `Show terminal`, `Keep awake for this session`.
  - Cấp 3 **Integrations:** Claude Code / Grok Build / Codex với đường dẫn file hook và trạng thái `● installed`; click → Repair. Chú thích: Grok cũng đọc hook của Claude, sự kiện được khử trùng lặp theo session.
- **Components:** `new (rung 7) — StatusBlockView` (icon, tiêu đề + phụ đề, dòng mô tả, hàng nút inline — dùng chung cho Ports/project); count capsule `new (rung 7) — CountBadge`; hover actions `new (rung 7) — RowActionButton` (icon 26×22).
- **States:** off (status block "Awake off" + `Keep awake for 1 hour` nổi bật) · không có lease agent · hook chưa cài (Integrations hiện `● not installed` + inbox To-do "Install the Grok hook").
- **Type & color / A11y:** như Surface 1; nút hover phải truy cập được bằng bàn phím (Tab trong row).

## Surface 4 — Ports submenu (+ Project)

- **Layout:** `Open all web servers [5]`, `Stop all ports…` → header **Projects** (mỗi dự án 2 dòng: tên + badge số port; dòng phụ `⑂ main · by Claude · :5173 :3101 :3100` hoặc `2 worktrees`) → chú thích "4 macOS system ports hidden · refreshed 3 s ago".
  - Cấp 3 **Project:** header tên + đường dẫn (mono); segmented lọc worktree khi > 1 (`All · dev · feat-lmedia`); mỗi tiến trình 2 dòng (`postgres` / `docker · klynt-postgres`, port mono căn phải; hover → `Open` (nếu web) · `Copy` · `Stop`); hàng inline cuối `Terminal · Finder · Stop N`.
- **Components:** như Surface 3 + `rung 1 — NSSegmentedControl`.
- **States:** không có port · port của UID khác/hệ thống luôn ẩn · `Stop` đang chạy / thất bại (dòng phụ báo lỗi) · `Stop all` luôn đi qua xác nhận (hoặc Decision trong inbox nếu do AI đề xuất).

## Surface 5 — Pull Requests submenu (+ lists, By repository)

- **Layout:** `Open pull requests on GitHub`, `Open notifications [4]` → nhóm **Review requested · My pull requests · Failing checks · Merge conflicts** (badge + `›`) → header **By repository N**: top 6 repo theo số PR (badge + `›`), sau đó `All N repositories ›` → chú thích "@account · updated 40 s ago". **Không** có account block / Switch account.
  - Cấp 3 **danh sách PR:** header + segmented khi hữu ích (`All · Ready · Drafts`); mỗi PR 2 dòng: avatar · tiêu đề · tuổi căn phải / `repo #n · author` (mono) + **một** trạng thái quan trọng nhất (thứ tự ưu tiên: checks fail › checks running › conflicts › review requested › approved › draft › checks pass); hover → `Open on GitHub` · `Copy link`.
- **Components:** avatar `new (rung 7) — AvatarView` (ảnh GitHub, fallback chữ cái đầu trên nền fill); còn lại dùng chung.
- **States:** chưa đăng nhập (một dòng `Sign in to GitHub…` thay toàn bộ submenu) · offline/rate-limited (chú thích cuối đổi thành lý do + thời điểm thử lại) · danh sách rỗng ("Nothing here.").

---

## Amendments (ràng buộc đã quyết trong review)

1. Hướng **A** (NSMenu + submenu) — không dùng tabs (B) hay dashboard theo dự án (C).
2. Gọn & hiện đại: hàng một dòng khi có thể, action chỉ hiện khi hover, icon kiểu SF Symbols.
3. **Icon đơn sắc** — không ô màu, không gradient; màu ngữ nghĩa chỉ là chấm 7pt; switch dùng accent hệ thống.
4. Pattern RepoBar: submenu nhiều cấp (≤ 3), status block có nút inline, count capsule xám, trạng thái dạng chấm + chữ, avatar thật, chữ 14pt.
5. Cấp 0: Awake / Ports / Pull Requests mỗi mục **một dòng, một thông tin quan trọng**; bỏ account & Switch account ở mọi nơi.
6. Chịu tải lớn: cấp 0 tối đa 3 mục inbox + `Show all N`; PR gom theo nhóm và **By repository** (top 6 + All); mỗi PR chỉ hiện 1 trạng thái.
7. Bỏ dòng caption "Navori · N need you · time" — menu mở bằng ô Ask.
8. **Needs you = inbox quyết định giữa người dùng và AI** (Decision / To-do / Update), không chứa PR.
9. Brief tạm thời: mỗi feature cập nhật surface của mình khi ship; các ràng buộc 1–8 giữ nguyên trừ khi review lại.

## Open questions cho `design-solution`

- **NSMenu thật hay NSPanel tự vẽ cascade?** NSMenu cho hành vi native miễn phí (bàn phím, material, lật submenu) nhưng ô nhập (Ask, Add a to-do) và nút inline trong `NSMenuItem.view` cần spike để xác nhận focus/tracking. R0 hiện dùng `NSPanel` — quyết định này có thể thay thế `StatusPanel`.
- Dữ liệu inbox (Decision/To-do/Update) cần một module mới trong kernel (đề xuất `navori-mod-inbox`, nhận Decision từ AI runtime qua `ActionSpec` + `Risk`).
