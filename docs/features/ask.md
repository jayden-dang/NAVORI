# Ask

Send one question to DeepSeek from the terminal and print the answer. This is the walking skeleton of the AI path: text only, one message in, one reply out, no history, no streaming, no tools.

```
navori ask --model <id> "<text>"
```

`--model` is required and is DeepSeek's own model name, passed through unchanged. The prompt is one positional argument. The reply goes to stdout with control characters and escape sequences removed (newline, tab and printable text are kept), so a reply cannot drive the terminal; every failure goes to stderr as `navori: <message>`.

## The key

Save the DeepSeek API key in Settings → AI (`Provider API key`, then `Save`; `Remove` deletes it). It is stored in the macOS Keychain under the item `llm-provider/deepseek`, in the service `dev.navori` (release) or `dev.navori.dev` (dev profile), so each profile keeps its own key. The Keychain is opened on first use, not at startup. With no key saved the ask stops before anything else.

## What is sent

The app, not the CLI, makes the request. The Core reads the key, runs the prompt through the Sensitive Data Shield, writes the audit row, and only then sends. The provider receives the redacted text as a single `user` message in a Chat Completions request to `https://api.deepseek.com/chat/completions`, with the key as a bearer token and `stream` off. The `--model` string is sent as given and is not shielded. The reply travels from the provider to the CLI as returned; only the CLI's printing removes control characters.

If the Shield fails or returns a malformed result, nothing is sent. If the audit row cannot be written, nothing is sent. The client does not follow redirects, so a reply cannot move the request off the base URL.

## The audit table

Each ask that reaches the Shield writes one row to the table `context_audit` in the Navori database before any send: `ts_ms`, `provider` (`deepseek`), `kinds` (`text`), `redacted_count` and `outcome` (`authorized` or `blocked`). A row never holds the prompt, the redacted text or the reply. The table is created on the first ask. Nothing reads it yet: the user cannot see what the AI has seen, so GOAL-6 is not met by this feature.

## Exit codes and failures

| Exit | Meaning |
|---|---|
| 0 | the reply was printed |
| 1 | the ask failed (every message below) |
| 2 | usage error, for example a missing `--model` |
| 3 | the app is not running |

The messages, with the IPC code the app returns:

- `No key set. Add one in Settings → AI.` (-32004)
- The Keychain failure text, or `the Keychain call did not finish` (-32005)
- `The Shield failed, so nothing was sent.` or `The audit log could not be recorded, so nothing was sent.` (-32006)
- The provider failure (-32007): `could not reach the provider`, `the provider did not answer in time`, `the provider answered with status <N>` with a hint for 401, 402, 404, 422 and 429, or `the provider's answer had no text`.
- `the question is too large: the request would be <N> bytes and the limit is <L>` (checked before connecting).
- `the app did not answer within 150 s (the request may still finish in the app)`.

## Limits

- The request line is at most 1 MiB (1,048,576 bytes), counting the JSON around the prompt and model.
- The CLI waits 150 s for the app. After 3 s it prints `waiting for the answer…` to stderr.
- The app waits 120 s for the provider.
- While an ask is pending the app keeps answering `status` and its other methods.

Two environment variables exist for tests only and are not in `--help`: `NAVORI_ASK_WAIT_MS` sets the CLI's wait, and `NAVORI_ASK_MAX_BYTES` lowers the size limit (it can never raise it above 1 MiB).

## Logs

Every line is a `warn`. The gateway lines carry `provider` and a `reason` field; the provider-failure line carries `provider` and a `kind` field; three lines in the FFI layer carry neither.

- `reason` is one of `shield_refused`, `keychain_failed`, `keychain_unfinished`, or an audit reason: `migration`, `write`, `shield_pass`, `unfinished`.
- `kind` is one of `network`, `timeout`, `status`, `malformed`. `network` is shared: it covers a refused connection, a failed HTTP client build and any other transport error that is not a timeout.
- A Shield refusal whose `blocked` audit row also fails to write logs two lines: `shield_refused`, then the audit reason.
- An audit write that fails before a send logs one line with the audit reason.
- No key stored is a refusal (`AskError::NoKey`) that logs nothing.
- The FFI layer logs `keychain call did not finish`, `keychain open failed` and `provider key slot invalid`, with no `provider` field. Saving or removing a key reaches them too, not only Ask.

No line carries the prompt, the redacted text, the reply, the key, the model or the URL.

## The live test

`ai/llm/tests/live_deepseek.rs` calls DeepSeek for real. It is `#[ignore]`d and does nothing unless `NAVORI_LIVE_LLM=1`; it reads the key from `pass show $NAVORI_LIVE_PASS_ENTRY` and the model from `NAVORI_LIVE_MODEL`:

```
NAVORI_LIVE_LLM=1 NAVORI_LIVE_PASS_ENTRY=<entry> NAVORI_LIVE_MODEL=<model> cargo test -p navori-llm --test live_deepseek -- --ignored
```

## What it does not do

- Other providers, a model router or fallback.
- Streaming, tools, images, history or more than one message.
- Show the audit table anywhere.
- A `navori doctor` row for the AI path.
