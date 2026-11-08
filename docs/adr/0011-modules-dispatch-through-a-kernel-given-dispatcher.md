# 0011 — Modules dispatch Actions through a kernel-given dispatcher, and only the composition root's grant lets one run a Destructive Action

Status: Accepted · 2026-11-08 (NEEDS, ROAD-13; drafted in an unattended `/shape ROAD-13 --auto` run; accepted by the operator's "approve", 2026-11-08 17:37 +07, after reading the spec sheet at 270a07d)

Needs you must let any Module raise a Decision, and must run the Envelope the user picks — possibly `Destructive`, as `ports.kill` is — from inside the Core (GOAL-1, ARCH-1, ARCH-5). Until now a Module could only emit Events: `ModuleCtx` held no handle to other Modules (`crates/kernel/src/module.rs:122-131`), and only the Shell, the CLI, IPC and Navori AI called `Kernel::dispatch`.
So the kernel gives every Module a `ModuleDispatcher` on its context. It sends an Envelope without awaiting the target, so a Module never blocks its own run loop and a dispatch to itself cannot deadlock. It carries the 30 s timeout itself, and it reads the Action catalog so a caller can ask an Envelope's Risk. ARCH-3 already names "dispatched actions" as a way Modules meet, so no crate depends on another. A dispatcher refuses any `Destructive` Action unless the composition root (`build_kernel` in `crates/ffi`) granted that Module the right by ID. Today only `needs_you` gets it, and it uses it only for the choice the user answered.
Rejected: Events only, with producers subscribing to `decision_answered` and running their own answer. That puts a Destructive run outside the one place the user approves it, and no Module subscribes to the bus today. Also rejected: a name check on `"needs_you"` inside the kernel, which knows no Module by name (`crates/kernel/src/tray.rs:1-2`).

## Known limits

- The grant is a line of code in the composition root, not a sandbox: a reviewer must refuse a second grant without a new ADR.
- The CLI and the IPC socket still check no Risk (`crates/ffi/src/app.rs:521-528`); this ADR does not change that (named for ROAD-37).
