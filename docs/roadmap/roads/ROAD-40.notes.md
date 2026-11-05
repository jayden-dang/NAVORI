# ROAD-40 computer-use — notes

Recorded 2026-11-05 from the operator's conversation (held in Vietnamese; translated here).
Decision of record: [ADR 0009](../../adr/0009-computer-use-ported-from-cua-driver.md).

## What the operator asked for

- Navori AI controls the Mac's apps "without taking the screen and the mouse", in the way
  cua-driver does.
- After weighing embedding cua-driver against building our own, the operator chose to build
  our own, optimised for Navori, using cua-driver's current code as the base.
- Use whatever Apple's Siri AI developer preview offers, where it helps.

## Control paths, cheapest first

1. Navori's own Actions and MCP tools.
2. Other apps' App Intents through Shortcuts (`shortcuts run`), or AppleScript for scriptable apps.
3. The Accessibility tree: read, then act on an element (port of cua-driver `ax/`).
4. Background input through SkyLight with the focus guard (port of `input/skylight.rs`,
   `key_focus_guard.rs`); private API, so a fallback, not the default.
5. Screenshot plus pixel click (ScreenCaptureKit; port of `capture.rs`).

Paths 4 and 5 act on another app, so they are expected to be `Destructive` and go through
approvals (ARCH-5, ROAD-37). Screenshots reach a model only through the Context Gateway, so
the screen-facing Shield layers (ROAD-39) come first.

## Known limits of cua-driver on macOS (2026-11-05, commit ade0801)

- Electron: background scroll refused; Tauri and WKWebView: background drag refused.
- Some clicks still activate the target app first.
- Open issues worth tracking: #4663 (screenshot buffer leak in `serve`), #4697 (click hits a
  button behind a modal sheet), #4696 (stale element token accepted after a label change).
- Not run on this Mac (macOS 27.0.1); daemon memory and CPU unmeasured.

## Siri AI developer preview — spike questions

Apple opened it on 2026-11-03 for macOS 27.3 beta. It lets Siri call into an app that adopts
App Intents, App Schemas and onscreen-awareness annotations; no API found for one app to
discover or invoke another app's intents. A spike needs macOS 27.3 beta, Apple Intelligence
on, and Siri set to English (Vietnamese is "soon"). Questions:

1. Can an `LSUIElement` menubar app be a Siri target?
2. Does the self-signed `Navori Local Signing` identity (ADR 0004) count as "dev-signed" for
   the preview, or is an Apple Development certificate required?
3. Do `appEntityIdentifier` annotations in other apps surface through the Accessibility tree,
   so that path 3 can read meaningful entities instead of bare controls?

## Risks to watch

- SkyLight is private API; each macOS release can break path 4.
- Apple's 2026-10-02 notice tightens Full Disk Access for AI agents; it names no other
  permission yet, but Accessibility and Screen Recording may follow.
- Do not port cua-driver's `cua-perception` extension (AGPL-3.0, OmniParser).

## Sources

- cua-driver: https://github.com/trycua/cua/tree/main/libs/cua-driver
- Siri AI preview: https://developer.apple.com/news/?id=gdk2umrf
- WWDC26 240 (App Schemas): https://developer.apple.com/videos/play/wwdc2026/240/
- WWDC26 343 (advanced App Intents): https://developer.apple.com/videos/play/wwdc2026/343/
- Full Disk Access notice: https://developer.apple.com/news/?id=p6zjojqw
