# More efficient retrieval for activity matching

Research date: **29 October 2026**. Research only: no implementation, benchmark, model call, or claimed quality/speed improvement. This note complements [the keyword/vector/hybrid investigation (draft PR #64)](https://github.com/Daanvdplas/meedoenbalie-rag-learning/pull/64).

## Recommendation

**First investigate returning a bounded activity record after finding a relevant section. Then investigate structured filters for explicit conditions.** Our use case asks about activities, while the current search ranks isolated sections. Improving what a result contains may remove more unnecessary searching than changing its ranking algorithm. This is a project hypothesis, not a measured result.

For example, the bicycle workshop's time is in `ACT-FIETS-002:S2`, while its indoor setting and address are in `S3`. A hit on the workshop can lead to a local lookup of its other sections rather than another model-directed search. The lookup cannot discover an activity absent from the initial results, and expanding a wrong hit can waste context.

For a question such as “indoors on Tuesday”, checking recorded `day` and `setting` fields is more direct than expecting text similarity to enforce both. For “I enjoy nature”, semantic search can still be useful. These methods complement keyword/vector/hybrid search rather than necessarily replacing them.

## What exists today

Inspected source commit: [`cada34fbb4d22bde46d31c3cdbcda495e6d93e9b`](https://github.com/Daanvdplas/meedoenbalie-rag-learning/tree/cada34fbb4d22bde46d31c3cdbcda495e6d93e9b). Research branch starts from `origin/main` at `6f6c309`; shared checkout and its existing `README.md`/`AGENTS.md` changes are untouched.

- Five fictional activities, each with six sections. `Section` already retains activity identity, source date, title and section identity; see [`src/lib.rs`](../../src/lib.rs) and [`src/source.rs`](../../src/source.rs).
- MiniLM ranks thirty section vectors using exact cosine, not an approximate vector index. BM25 exists as an experiment baseline; hybrid is proposed separately. See [`src/experiment/arms.rs`](../../src/experiment/arms.rs).
- The `full_context` experiment already supplies all thirty sections without retrieval tools. Its retrieval metrics are N/A; `offered_fact_coverage` describes available evidence, not searching ability or answer correctness. See [`docs/evaluation/metrics.md`](../evaluation/metrics.md).
- Current records do **not** contain typed activity schedules, settings, accessibility or eligibility fields. Adding filters entails source-data modeling and maintenance, not just writing a faster query.

## Alternatives and trade-offs

| Option | Why it might help here | Cost or limitation | Priority |
| --- | --- | --- | --- |
| Bounded activity/parent expansion | Keep time, place and requirements together after a section hit; no extra embedding or answer-model call is inherently required for the lookup | More text per result; wrong seeds expand too; cannot recover missing activities | First small experiment |
| Structured constraints plus text retrieval | Enforce explicit day/time/setting requirements, then rank remaining activities by interests | Source annotation and natural-language interpretation can be wrong; missing fields remain unresolved | Next targeted experiment |
| All five documents in context | Already available; removes retrieval omissions at the current tiny corpus size | More input text and exposure; model can still miss/misinterpret evidence; does not scale indefinitely | Keep as small-corpus control |
| Local cross-encoder reranking | Re-score candidate passages against the whole question when useful evidence is present but ranked too low | Additional model download, memory and pairwise inference; cannot recover absent candidates | Only after observing ranking failures |
| Learned sparse retrieval | Neural document expansion can bridge vocabulary differences with lexical-style matching; an inference-free-query variant exists in pinned fastembed | New index/model and maintenance; Dutch suitability and local efficiency unverified | Watchlist, not first work |
| Query routing or selective follow-up | Use direct lookup/filter for supported questions and broader search only when necessary | Routing failures and extra planning can cancel savings; similarity is not calibrated confidence | Start with explicit typed requests, not an automatic confidence threshold |

Changing retrieval granularity, filtering, reranking and routing are different interventions. Do not introduce all of them at once and attribute any improvement to a single method.

## Small first experiment

Compare **the same ranker and same original question**, with two result policies: existing section selection versus bounded same-activity expansion. Predeclare one shared context-token ceiling, a maximum number of distinct activities, deduplication and a deterministic admission order. Preserve seed hits before adding sibling sections, within that ceiling. Admit whole unchanged sections only, count every admitted unique section, and show sections omitted for overflow. Preserve document ID, section ID, date and exact source text; do not present truncated text as an unchanged canonical section. Three complete activities are not equivalent to three sections. If a complete activity does not fit, follow the declared section-admission policy instead of silently raising the budget.

Use the existing eighteen questions as development/regression cases, retaining broad questions and all-activity cases alongside the time/address example. Measure expected-evidence goal coverage, complete-goal success, context tokens and local work. Count first-stage ranking and expansion separately. A local no-Codex/no-judge runner is still proposed work, not an existing command. This first experiment cannot prove fewer agent searches; that requires a later bounded comparison with the same Codex model, prompt and search limit.

Add structured filtering as a **separate** experiment on independently source-verified fields and relevant cases. Begin with annotated typed requests to isolate the filter engine; do not count this oracle-input experiment as end-to-end free-question performance. Record which annotations were produced manually and their preparation/update cost. Later test a general query parser separately, including its latency, model work, failures and ambiguous requests. Never build runtime rules from case IDs or expected answers.

Check these behaviors explicitly:

- Hard requirements versus preferences: “must be indoors” differs from “prefer indoors”. Preserve negation and AND/OR, including “Thursday or Friday”.
- Schedule semantics: specify whether “available 14–15” requires full containment or any overlap; handle recurring days, dates and the reference date explicitly.
- Unknown is not false: return confirmed matches and unresolved candidates separately. An unknown wheelchair-accessibility field must not become “inaccessible”, and must not be presented as a confirmed accessible match.
- Historical places are not current capacity. Preserve source dates and source-backed unknowns; a schedule never proves an available place.
- A failed parse, empty lexical result or absent metadata does not establish that no suitable activity exists. Make the limitation visible and use a declared fallback or clarification.

For structured selection, score both wrong inclusions and wrongly excluded qualifying activities against independently annotated expected sets. For expansion, score complete evidence combinations, including AND between necessary sections. Do not confuse returning every loosely relevant section with providing sufficient evidence for the requested facts. Precision/recall/rank metrics require complete relevance labels; otherwise show N/A. No AI judge is needed for these checks, but these checks do not establish free-text answer correctness.

## What “more efficient” must mean

A candidate is promising when it maintains required quality with less work, or improves quality at an acceptable measured cost. Do not declare a winner by tool-call count alone.

Record retrieval time, end-to-end elapsed time separately, unique context tokens, model inference work, generation sessions/search calls, failures, startup/download/index preparation, memory and update effort. A local embedding call, an MCP call and a Codex session are different units; provider request counts may remain unknown. Report cold and warm paths separately. Repeated local timings can describe variation but do not increase the number of independent questions. Keep failures and all questions visible.

At five documents, full context is a reasonable practical cost/quality reference, not a presumed winner. Keep it separate from the equal-budget ablation if all thirty sections exceed the shared ceiling. At larger future corpora, compare with additional independently authored synthetic activities and frozen questions; the parser currently caps the corpus at five, so a scaling benchmark itself needs a separate implementation decision. Duplicating existing documents is not evidence of realistic search quality. Previously inspected questions are not a fresh holdout.

A later HTML comparison can show each question, expected facts, sections admitted by each policy, missing facts, context size and measured work. It should show improvements **and regressions**, and distinguish “evidence was available” from “the answer was correct”. No page is added by this research.

## Primary sources and applicability

All external sources below were published or demonstrably updated in 2026. They establish mechanisms and trade-offs, not performance results for our Dutch activity corpus.

1. **Microsoft Learn, updated 27 April 2026 — [Filters for keyword search](https://learn.microsoft.com/en-us/azure/search/search-filters).** Exact-value predicates on nonvector fields can work alone or with text search. Supports combining structured constraints with relevance ranking; it does not establish that our free-text questions can be parsed correctly or that we need Azure.
2. **Microsoft Learn, updated 27 April 2026 — [Vector query filters](https://learn.microsoft.com/en-us/azure/search/vector-search-filters).** Describes filter placement and recall/latency trade-offs. Azure HNSW/shard benchmarks do not transfer to our thirty-vector exact scan; “prefiltering is always faster” would be unsupported.
3. **Microsoft Learn, updated 28 July 2026 — [Parent-child index projections](https://learn.microsoft.com/en-us/azure/search/search-how-to-define-index-projections).** Describes parent IDs and application-side parent lookups after child matches. Supports the lookup mechanism; our bounded activity-expansion policy and expected benefit are our own hypothesis, not this source's benchmark result.
4. **Elastic, published 9 March 2026 — [Building effective database retrieval tools](https://www.elastic.co/search-labs/blog/database-retrieval-tools-context-engineering/).** Discusses curated query interfaces, input validation, output control and evaluation of agent tools. Supports measuring retrieval-tool behavior and avoiding unrestricted query generation; no Elasticsearch migration is proposed.
5. **LangChain, published 23 October 2026 — [Context engineering starts with retrieval](https://www.langchain.com/blog/context-engineering-starts-with-retrieval).** Distinguishes exact file/keyword lookup, semantic retrieval and structured SQL retrieval, and discusses repeated searches/context volume. Its offload/delegate architecture is not justified by our thirty short sections.
6. **fastembed-rs v7.1.2, released 19 October 2026 — [release](https://github.com/anush008/fastembed-rs/releases/tag/v7.1.2), [reranker definitions](https://raw.githubusercontent.com/anush008/fastembed-rs/v7.1.2/src/models/reranking.rs), [sparse-model definitions](https://raw.githubusercontent.com/anush008/fastembed-rs/v7.1.2/src/models/sparse.rs).** The pinned library exposes multilingual rerankers; its default reranker is described as English/Chinese, so it should not be adopted blindly for Dutch. Its OpenSearch sparse variant expands documents with ONNX while encoding queries through tokenizer/IDF lookup, avoiding query neural inference. Neither mechanism has been measured on this corpus; models, licensing and Dutch performance need separate verification before adoption.

No SQL database is required for the first structured experiment: typed Rust records and read-only filters are sufficient. A structured tool is still retrieval and can ground generated answers; embeddings are one retrieval option, not a requirement for RAG.
