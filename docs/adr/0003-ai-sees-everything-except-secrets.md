# 0003 — The AI observes everything except secrets, behind a fail-closed Shield

Status: Accepted · 2026-11-01

An opt-in, per-app observation model would be safer by default but would leave Navori AI blind to the context that makes it useful; we instead let it see apps, windows, text and the screen, and remove secrets before anything is stored or sent. This makes the Sensitive Data Shield and the Context Gateway load-bearing: they fail closed, keep an audit log of what kinds of data went to which Provider, and the user can pause observation from the menubar (ARCH-6, ARCH-7; THR-1, THR-3, THR-10 in `docs/security/threat-model.md`).
