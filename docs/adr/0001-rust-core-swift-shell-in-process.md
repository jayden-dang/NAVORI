# 0001 — Rust Core linked in-process into a Swift Shell via UniFFI

Status: Accepted · 2026-11-01

A menubar app could have been pure Swift, a Tauri/WebView app, or a Rust daemon driven by a thin UI; we chose a Rust Core built as a static library and linked into an AppKit + SwiftUI Shell through UniFFI, in one process. Rust keeps the logic portable, testable and shareable with the `navori` CLI and MCP, while Swift is the only way to get a truly native menubar and the Apple frameworks tied to the app bundle and its permissions — and one process avoids a launchd daemon, its install story and a second IPC hop, at the cost of a two-language build and a deliberately narrow FFI surface (ARCH-1, ARCH-2, ARCH-15).
