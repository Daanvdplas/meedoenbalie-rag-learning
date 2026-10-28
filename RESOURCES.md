# Codebase Resources

## Knowledge

Source snapshot: `ce721adf6cf15d80ca258d364101eefa288b2a05`, checked 28 October 2026. These are primary sources for this particular application, not general platform recommendations.

- [README](README.md) — Current scope, start commands, known retrieval limits, and the separation between the demo and experiments.
- [Browser](static/index.html) and [Rust web coordinator](src/web.rs) — Use to trace a submitted question, the pending run, and the answer shown to the user.
- [Codex runner](src/codex.rs) — Use for the restricted search tool, generation instructions, and exact citation checks. Those checks do not establish the truth of every sentence.
- [Search index](src/lib.rs), [MCP tool](src/mcp.rs), and [CLI](src/main.rs) — Use for local embedding/search and the shared retrieval implementation. Different processes load separate in-memory indexes.
- [Fictional gardening document](fixtures/document.md) — Use to distinguish a document's date from when availability was last checked.
- [Decisions](decisions.md) — Recorded reasons for the small corpus, local index, source identities, and bounded searches.
- [CI checks](.github/workflows/checks.yml) and [MCP CLI test](tests/mcp_cli.rs) — Use to distinguish deterministic engineering checks from live model quality.
- [Recorded evaluation](docs/evaluation/results/2026-10-26/README.md), dated 26 October 2026 — Existing experiment evidence and its limits; not human quality validation.

## Wisdom (Communities)

No community resource is needed for the first lesson about this repository's own behavior. No community preference has been established.

## Gaps

- No evidence of the learner's understanding has been collected yet.
- The existing semantic evaluations have not been calibrated by human reviewers.
