# Real Codex retrieval bridge

Scope: one fictional document, six sections, five questions. This is a local learning prototype, not a COA system or a production-quality claim.

```sh
cargo run -- ask --top-k 3 'Waar en wanneer is het tuinieren?'
cargo run -- mcp
```

`ask` starts authenticated Codex, which calls the Rust stdio MCP tool `search_document(query, top_k)`. The tool calls the same `Index::search_with_top_k` as direct retrieval and evaluation. It cannot read expectations, arbitrary paths, resources or URLs. Discovery does not load the embedding model; the first search does. Protocol stdout contains JSON-RPC only.

The async application API is `CodexRunner::new()?.run(question, top_k).await`. A successful result contains the answer, quoted citations, unknowns, actual completed tool calls and evidence, source/date/as_of, the index manifest, and client/model details. Failures return an error, never a successful invented answer. `configuration()` records prompt/schema/policy versions, model/effort and SHA-256 fingerprints of the runtime, code-mode host and MCP executable. A changed executable requires restarting the application before another run.

## Authentication and runtime

The CLI reuses the existing ChatGPT login. Check `codex login status`; do not copy authentication files or put credentials in the repository. No global settings are changed. On this machine, the Homebrew CLI 0.163.0 could authenticate but timed out negotiating its code-mode host. The already-installed matched app-daemon package worked. Discovery therefore prefers an installed app-daemon binary with its adjacent host; set `COA_CODEX_BIN` to override that choice.

The model is pinned to `gpt-6.1-sol`, the user's available configured model verified on 2026-10-20. `COA_CODEX_MODEL` is an explicit local override. Reasoning effort is `low` for these small lookups and is recorded for replay.

The bridge currently targets macOS/Linux. Each run forces `--no-daemon` so generation and its helpers stay owned by this child process, ignores user configuration and command rules, uses an empty temporary working directory and a named permissions profile denying filesystem reads/writes except minimal runtime paths. It does **not** pass `--sandbox`, which would override the named profile. Command networking is disabled. Shell, apps, plugins, browser/computer tools, memories, hooks, extra skills and multi-agent tools are disabled; only the required MCP server's `search_document` tool is enabled. MCP runs outside the command sandbox and is bound to the current application executable and corpus, not browser-provided settings. A matched installed runtime is needed for these settings.

## Bounds and validation

Questions contain 1–1000 characters; top_k is 1–6. There is one active Codex run per runner, at most four searches per MCP session, a 120-second process timeout, 512 KiB stdout and 16 KiB stderr limits, and a 16 KiB MCP request limit. User text goes through stdin with argument arrays; no shell interpolation. A dedicated process-group guard stops Codex and descendants on completion, timeout, output failure or future cancellation; temporary run files also have a cleanup guard. Private reasoning and raw diagnostics are not saved in public run records.

A successful answer requires an actual successful MCP search, a completed Codex turn, a consistent nonempty source/index manifest, and valid structured output. Every cited section must have been returned and every quote must occur exactly in its returned text. This checks provenance; it does not automatically prove every sentence's meaning. Human review still checks relevance, unsupported claims and faithful unknowns. Missing evidence remains visible. There is no calibrated no-match threshold or accuracy claim from five questions.

## Verification

Deterministic `cargo test` covers protocol discovery/validation, explicit tool failures, actual CLI framing without model downloads, citation/quote and stream rejection, literal child input, process timeout/output limits, descendant cleanup on timeout/cancellation, and binary changes. Live model checks are separate from CI. The real Rust Q1 run used two searches, preserved the first missing-address result, then retrieved the address with a focused query; citations and source date came from those actual results. All five real CLI cases completed: time/location, no experience, schedule/language help, unknown current places, and unknown transport. Q1 and Q5 used a second focused search; every quote came from actual returned evidence. These five observations do not establish an accuracy rate.

The filesystem boundary was independently checked on 2026-10-20: the same named deny-root policy allowed `/bin/echo` but denied reading a fresh harmless canary. An independent real Codex/MCP probe retained search access and returned transport as unknown. A model's claim about unavailable tools alone is not treated as proof of OS isolation.

Platform reference: OpenAI's [Harness-aware plugin evaluations](https://developers.openai.com/cookbook/examples/partners/harness_aware_plugin_evals/harness_aware_plugin_evals), published **2026-09-23**, describes isolating evaluation expectations from Codex while retaining MCP access. Installed client help and the actual checks above were observed on **2026-10-20**.
