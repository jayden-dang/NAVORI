# Ports

See which TCP ports your own processes and Docker containers hold, grouped by project and worktree, and stop them from the menubar or the terminal. Ports does not scan on demand: it publishes a scan every 3 seconds while the panel is open and every 30 seconds while it is closed, and `list` and `open` answer from the last one.

## What you see

The `Running` row on level 0 replaces the Awake row. It shows a green dot while a session Lease is live or any port is listed, and no dot otherwise. Opening it shows `Keep awake`, `Tracking agents`, a problem line only when something is wrong, then one row per project, then `Stop all ports…`.

A project row is a dot and a name. A filled green dot means a session is working in it; an orange ring means Awake is known and nobody is; no dot means Navori cannot tell, and a note line in the submenu says why Stop is off (tracking agents off, Awake unreadable or stopped, or a session with no project label). Idle projects come first, then by name, with `Other` (ports with no project) last. Opening the submenu reads only the published state.

A project's child menu shows its checkout path and, when idle, how long its oldest listener has been up. Under `Sessions` it lists the Agent sessions, and under `Ports` each port with a mark for its kind, the process or container name and `:port`. Hovering a port row offers `Open`, `Copy` and `Stop`. The tooltip and the VoiceOver sentence name the worktree and the Harness that started the port, when known.

`Open` asks the Core for the port's URL and opens only an `http` URL in the default browser. If the Core gives none, the row says `Could not open :PORT`.

## How to stop

Hover an idle project row and press `Stop N`. The row turns into a confirmation that names the N ports; nothing is sent until you accept it. `Stop N ports` also ends every project's child menu, working projects included, and `Stop all ports…` stops every idle named project after a confirmation that says how many ports are left running (in use, and in `Other`). `Other` is never reached by `Stop all ports…`; it has its own `Stop N`. A single port row has its own `Stop`, with its own confirmation.

A press sends one `ports.kill` request. A port that did not stop keeps its row and shows why in the child menu.

From the terminal:

```
navori dispatch ports '{"kill":{"selector":{"project":"klynt"},"targets":[{"process":{"pid":4123,"start_us":1730000000000000}}]}}'
```

The selector (`{"pid":N}`, `{"worktree":{"project":"p","worktree":"w"}}` or `{"project":"p"}`, where `{"project":null}` means `Other`) only checks that the targets belong where you think. The `targets` are the exact processes (pid and start time) and containers (`{"container":{"id":"<hex>"}}`) to stop, copied from `ports.list`; a stop never adds one. A request needs 1 to 256 targets, a process pid above 1, and container ids of 1 to 64 hex digits, or it is rejected and nothing is signalled. The reply holds one result per target in request order: `stopped`, `stopped_alone`, `already_gone`, `still_stopping`, `stale`, `protected`, `outside_selector` or `failed`, with a fixed reason for anything but a clean stop.

## What a stop does

Before it signals anything Navori reads the process list and Docker again and judges each target from that fresh read, never from the request. A process is sent SIGTERM; after 3 seconds, if it is still the same process (same pid and start time), SIGKILL; 1 second later it is checked once more. A container gets one Docker `stop` request, so its own stop timeout applies. Processes and containers of one request stop at the same time, and the reply comes when every target has an answer or after 25 seconds, with `still_stopping` for any that are not finished. A scan starts at once after the reply, and one `ports.stopped` Event records only the number of targets per result.

A process is signalled through its whole process group only when it leads that group, the group is not Navori's own or one of its ancestors', and every other listener in the group is also in the request and unprotected; otherwise the pid alone is signalled (`stopped_alone`).

## What is protected

Under every selector, Navori refuses its own process, its ancestors, its `navori` helper and Docker's backend process, and it never touches another user's processes. Under a worktree or project selector it also refuses databases, system services and containers running a well-known database image (`postgres`, `mysql`, `mariadb`, `redis`, `valkey`, `mongo`). A `pid` selector deliberately stops a database or system process you point at. The set is fixed in code; there is no config key for it.

## Logs

The stop path logs each signal with the pid, start time, signal, whether it went to the process or its group and the result, each reason a group stop fell back to one process, each refused target, and each container stop with its id and outcome. It logs no path, executable name, environment value or Docker reply.

## Known limits

- A `Destructive` Action should run only after explicit approval (ARCH-5). The only approval today is the Shell's inline confirmation; a request from the CLI or the IPC socket of the same user is carried out without one. ARCH-5 is not met for that caller until ROAD-37.
- Stopping a group signals every member of the group, including processes in it that hold no port.
- A database container has no single-port stop, because a container has no pid and the group selectors refuse it; it is counted among the hidden system ports.
- `Open` fails for a port bound to several addresses that give different URLs.
- A Docker port has no Harness, so its tooltip names none.
- A group mate named by a second overlapping request is not claimed by the first (only the request's own targets are).
- Docker is written to only by stopping a container.
