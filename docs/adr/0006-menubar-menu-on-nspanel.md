# 0006 — The menubar menu is a hand-built cascade on NSPanel, not NSMenu

Status: Accepted · 2026-11-02

`NSMenu` gives keyboard navigation, submenus and VoiceOver for free and did accept typing and inline buttons in a spike, but with a text field it peaked at 131 MB for about a second against the 60 MB budget, reopened about three times slower, blocked the main queue when opened in code, and leaves the glass to the system. We chose `NSPanel` with `NSGlassEffectView`, accepting that the Shell owns the cascade, edge flip, keyboard and VoiceOver, because GOAL-11 and control of the surface outweigh that code. Evidence: `docs/design/2026-11-01-menubar-ui-brief.md`; the cause of the peak was not found, so a later macOS could reopen this.
