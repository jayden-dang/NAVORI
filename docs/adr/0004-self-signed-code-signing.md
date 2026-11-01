# 0004 — Self-signed code signing until open source

Status: Accepted · 2026-11-01

With one user building from source there is no quarantine flag for Gatekeeper to act on, and every API Navori needs (IOKit, libproc, Accessibility, ScreenCaptureKit, the regular Keychain, notifications) works without a paid entitlement, so we sign with a stable self-signed identity (`Navori Local Signing`) instead of a paid Developer ID. The identity must stay stable because macOS ties Keychain access and privacy grants to it; the cost is that Navori cannot use any capability that needs a paid entitlement (APNs push, iCloud, managed keychain groups), and switching to Developer ID + notarization at open-sourcing will make the user re-grant permissions once.
