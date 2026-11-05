# Navori

AI-first personal cockpit for macOS (Apple Silicon): a menubar app with a Rust core and a
Swift (AppKit + SwiftUI) shell. Architecture: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Requirements

- macOS 26+ on Apple Silicon, Xcode (Swift 6) command-line tools
- Rust 1.97.1 (`rust-toolchain.toml` pins the exact toolchain and components)

## Install

Not published yet; available once v0.1.0 is released.

```sh
brew tap jayden-dang/navori
brew trust --cask jayden-dang/navori/navori
brew install --cask navori
```

Navori is self-signed and not notarized; the cask clears the quarantine flag so macOS opens
it without a Gatekeeper prompt. macOS 26 and Apple Silicon only.

Navori's hooks assume the harness runs them through a POSIX-compatible shell (sh, bash, zsh,
dash and ksh were tested; fish and nushell were not).

Release candidates install as the separate cask `navori-rc`. To move to the stable release:
`brew uninstall --cask navori-rc && brew install --cask navori`; your settings stay.

## Everyday commands

```sh
cargo xtask signing-setup   # once: stable self-signed identity (keeps Keychain / privacy grants across rebuilds)
cargo xtask run             # build + launch "Navori Dev.app" (separate data, logs and socket)
cargo xtask install         # build release → /Applications/Navori.app (old one kept as .previous)
cargo xtask ci              # fmt, clippy -D warnings, tests, dependency rules, Swift build
cargo xtask perf            # GOAL-11 budget; --record writes perf-baseline.json when the run passes
cargo xtask outdated        # compare dependencies with the latest stable crates.io releases
```

The `navori` CLI ships inside the bundle (`Contents/Helpers/navori`); Settings → General
links it into `~/.local/bin`. Talk to the dev build with `--profile dev`:

```sh
navori status      # version, uptime, module health
navori doctor      # config, database, IPC, modules, logs, awake, integrations, ports
navori events      # journal, newest first
navori actions     # every action modules accept, with risk level
navori awake       # hold, release-all, status, run
navori ask --model <id> "..."   # one question to DeepSeek; key in Settings → AI
navori dispatch system '"ping"'
navori dispatch ports '"list"'   # TCP ports your processes listen on, from the last scan
```

## Where things live

| | Release | Dev |
|---|---|---|
| App | `/Applications/Navori.app` | `build/dev/Navori Dev.app` |
| Data, DB, config, backups | `~/Library/Application Support/Navori/` | `…/Navori Dev/` |
| Logs | `~/Library/Logs/Navori/` | `~/Library/Logs/Navori Dev/` |

## License

[MIT](LICENSE)
