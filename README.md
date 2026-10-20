# Meedoenbalie: one-document retrieval experiment

A small learning experiment: one fictional activity document, five questions, and a Rust search function using real local embeddings. This is an independent prototype inspired by public COA information, not an internal COA plan.

## Try it

```sh
cargo run --locked
```

Open **http://127.0.0.1:8766/**. The first run downloads the multilingual model (about 235 MB) and supporting files into `.model-cache/`; it requires internet access but no API key. The server starts after the model loads. Subsequent runs reuse cached files. Rust 1.94.1 was used locally.

Read the document, choose Q1–Q5, and click **Find passages**. Judge the exact passages before revealing the expected evidence. You can edit the question; custom questions have no prepared criteria. Judgements stay in this browser session only. Repeating a question after revealing its criteria is marked as informed review.

CLI uses the same retrieval function:

```sh
cargo run --locked -- search 'Waar en wanneer is het tuinieren?'
cargo test --locked
```

## What it does

- `fixtures/document.md` is the only indexed document, split into six sections. Each short section is embedded once in memory.
- Rust embeds arbitrary question text and returns the three closest sections by cosine similarity, with exact source text, section IDs, source date and a fixed reference date of 2026-10-20.
- The local model is the quantized multilingual `Qdrant/paraphrase-multilingual-MiniLM-L12-v2-onnx-Q` (384 dimensions), loaded through `fastembed` 7.1.2. Document sections and questions use the same sentence-transformer encoding without prefixes.
- `fixtures/questions.json` contains only questions. `fixtures/expectations.json` holds human review criteria; it is never embedded or included in search results. The page requests criteria separately after a judgement.

There is **no generated answer or Codex connection yet**, no database, and no calibrated no-match threshold. With six passages, search always has a closest passage; that does not mean it answers the question. Historical places do not establish current availability. Transport is unknown when the document says nothing about it. Five questions are a human learning exercise, not proof of retrieval quality.

A later MCP adapter could expose this same Rust search function to Codex. That is a separate next step after user feedback, not part of this slice. The long-term goal remains a small, explainable agent harness.

## Boundaries

All people, activities and dates are fictional. Suggestions must retain sources and dates; missing or outdated facts remain unknown. Participation stays voluntary and an employee discusses options with the resident. This demo changes no records, sends no messages and registers nobody. It listens only on localhost.

All changes go through a branch and pull request. Never commit directly to `main`; merge only with explicit user permission.

## References

- [fastembed 7.1.2 release](https://github.com/anush008/fastembed-rs/releases/tag/v7.1.2), **19 October 2026**: the Rust SDK version used here. Model selection/API were checked against this release's source.
- [User journey](user_journey.md), [project instructions](AGENTS.md) and [2026 agent-harness orientation](agent-harness-landscape.md). The broader orientation is background; this experiment implements only the tiny retrieval slice above.
