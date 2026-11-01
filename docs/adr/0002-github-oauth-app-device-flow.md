# 0002 — GitHub access through an OAuth App with the Device Flow, not a GitHub App

Status: Accepted · 2026-11-01

GitHub recommends GitHub Apps, but Navori is local-only: it cannot receive webhooks, the Notifications API (its cheapest change signal) accepts only OAuth/classic tokens, and a GitHub App sees only repositories where it is installed. We use an OAuth App with the Device Flow — the `gh` CLI model — accepting a broad `repo` scope and a non-expiring token, which is kept only in the Keychain with a revoke button in Settings (ARCH-8).
