# Vector, keyword and combined retrieval: investigation

Research date: **29 October 2026**. Proposal only: no retriever, test, demo or experiment has been changed or run for this investigation.

## Recommendation

This fits the current Rust project without a database, external search service or additional model. Reuse MiniLM and the existing BM25 implementation, add rank fusion, and first compare the retrieved evidence **without Codex or an AI judge**. Add the same-agent comparison separately if that first comparison is useful. Hybrid is a hypothesis to measure, not an assumed improvement.

## What already exists

Code inspected at [cada34f](https://github.com/Daanvdplas/meedoenbalie-rag-learning/tree/cada34fbb4d22bde46d31c3cdbcda495e6d93e9b). The shared checkout's existing README/AGENTS edits and learning work were left untouched. This note is isolated from them.

| Capability | Current state and source |
| --- | --- |
| Vector retrieval | Local multilingual MiniLM, exact cosine ranking, in-memory vectors; title + heading + text. [`Index`](../../src/lib.rs), `search_with_top_k`. |
| Keyword retrieval | BM25 already exists in [`experiment/arms.rs`](../../src/experiment/arms.rs): `k1=1.2`, `b=0.75`, lowercase/alphanumeric tokens, no stemming or synonym expansion, same title/heading/text. |
| Older keyword baseline | `keyword_search` in [`lib.rs`](../../src/lib.rs) is Jaccard word overlap, **not BM25**. The old five-garden-question `eval` uses this baseline. |
| Fair one-search answer comparison | `dense_once` and `bm25` already use the original question, top three passages and the same answer prompt/model. Both still generate a Codex answer; `--max-judgements 0` disables the judge, not generation. [`runner.rs`](../../src/experiment/runner.rs). |
| Agentic search | Only `agentic_dense` exists. Codex can request up to four searches; [`main.rs`](../../src/main.rs) wires the MCP server to dense search. |
| Combined retrieval | Not implemented. |
| Full local retrieval-only comparison | Not implemented for the 18-question experiment dataset. Existing experiment records require an answer; the old local `eval` uses a different five-question dataset. |
| Evidence metrics | Fact-goal coverage, section precision/recall and first relevant reciprocal rank already exist. Source/date/text identities are checked. [`metrics.rs`](../../src/experiment/metrics.rs), [definitions](../evaluation/metrics.md). |

The corpus has **five fictional documents / 30 sections**. The evaluation has **12 pilot/development questions plus six previously frozen comparison questions**. All have already been used; the latter six are now regression examples for this investigation, not fresh unseen evidence.

## Smallest useful design

Introduce one shared local retrieval boundary, configured **once per run** as `vector`, `keyword` or `hybrid`. The agent gets the same search tool and cannot change the selected mode. Keep source sections, dates and citation behavior identical.

- **Vector:** existing MiniLM + cosine ranking.
- **Keyword:** existing BM25, extracted from its experiment-only location so it is reusable.
- **Hybrid:** run both over the same query; combine their ranked results with reciprocal rank fusion (RRF). Return one final list, not two concatenated lists.

Proposed initial frozen settings: return at most **three** sections in every mode; hybrid takes up to **six candidates from each branch**, equal weights, and `score = sum(1 / (60 + rank))`, with ranks starting at one. The RRF constant 60 is a starting choice, not an optimality claim or the result-count setting. Break final ties by document ID then section ID. Deduplicate by **(document ID, section ID)**, never by section ID alone.

BM25 currently pads its top-k with zero-score sections, even when no query term matches. Those arbitrary lexical ties must **not** earn fusion credit. Proposed new keyword/hybrid behavior: keep only positive BM25 matches; fewer than three results, including zero, is valid. Zero lexical matches means no matching query terms, **not** that the corpus lacks a relevant answer. Preserve historical artifacts and identify the new behavior as a new retriever version. Also sort query terms before summing BM25 contributions to remove randomized `HashSet` iteration from new deterministic rankings; do not silently claim byte-identical old scores.

Cosine and BM25 scores have different meanings/scales. RRF uses their ranks; do not add raw scores or display any of them as answer confidence. Elastic explains the scale mismatch and rank fusion in its [8 January 2026 article](https://www.elastic.co/search-labs/blog/hybrid-search-multi-stage-retrieval-esql). Microsoft's [RRF documentation](https://learn.microsoft.com/en-us/azure/search/hybrid-search-ranking) gives the rank formula and distinguishes its constant from the result-count parameter; page metadata dates are **8 June 2026**, updated **17 September 2026**. These are references for fusion, not recommendations to install either product.

Hybrid has the same maximum final passage count, not necessarily equal context length; record offered text length/token counts when available. It does **more internal retrieval work**: two branch searches and up to 12 candidate occurrences before deduplication. Record both branch candidate limits and final top-k. One MCP call can contain two local branch searches; show both counts. This is not an equal-compute comparison.

## Comparison 1: retrieval only, no judge

Run all **18 existing questions × three modes = 54 retrieval results**, entirely locally once the embedding model is cached. No answer generation, no Codex login and no AI judge are needed for this proposed stage.

Hold constant the exact original question, corpus hashes, title/heading/text inputs, reference date and final top-k. Do not let Codex rewrite the question in this comparison. Freeze tokenizer, BM25 settings, embedding identity, fusion formula, candidate depth and tie policy before collecting results.

Use a dedicated retrieval-result record with configuration, ordered hits, timings and source provenance. Extract/reuse the evidence calculations beneath the answer-record wrapper; **do not invent answers or label fixtures as live answer runs**. Preserve branch ranks/raw scores and fused rank/score so a person can explain each hybrid result.

| Measure | Human meaning | Limitation |
| --- | --- | --- |
| Evidence-goal coverage | How many expected facts have a complete supporting section combination? Show e.g. `1/2`, not just `0.50`. | Does not judge a generated answer or prove every relevant passage was found. |
| Section recall | How many labelled relevant sections were retrieved? | Requires complete relevance labels. |
| Section precision | How many retrieved sections are labelled relevant? | Requires complete labels; empty results give N/A. |
| First relevant reciprocal rank | How high did the first relevant section appear? | Requires complete, nonempty labels; finding one section can still leave the answer unsupported. |
| Local retrieval time | What does embedding/scoring/fusion cost? | Separate warm query latency from index/model startup; repeat locally for timing, not to select a nicer ranking. |

P06 has incomplete relevance labels: precision, recall and ranking metrics stay **N/A** there. Questions with no positive supporting evidence have **N/A evidence coverage**, not 100% or zero. If relevant evidence exists but none is retrieved, coverage is zero. Show counts of eligible questions beside aggregates; retain failures separately. No current automatic overall retrieval-quality pass/fail threshold exists.

Concrete example, **T02**: “Welke activiteit kan ik op dinsdag binnen doen, hoe laat is die en waar moet ik zijn?”

| Expected evidence goal | Required sections |
| --- | --- |
| Bicycle workshop is on Tuesday, 14–16, indoors | ACT-FIETS-002:S2 **and** S3 |
| Werkplaats De Werkbank, Esdoornstraat 8, Lindedorp | ACT-FIETS-002:S3 |

S2 + S3 gives **2/2**; only S3 gives **1/2**; only S2 gives **0/2**, although the opening-time information is useful. These expectations measure complete goals, not isolated word matches. No mode is assumed to retrieve a particular combination before execution.

A result budget also limits achievable scores. T05 needs four distinct sections to cover its four goals: a single top-three result cannot exceed **3/4**, regardless of retriever quality. T02 has eight labelled relevant sections, so section recall at three results cannot exceed **3/8**, although two sections can establish **2/2** evidence goals. Show these structural limits beside results. Predeclare primary top-three results for all modes, with an optional secondary **top-six comparison for all modes** (same fixed six-per-branch hybrid candidate depth); do not increase only the losing mode's budget or change gold labels to make scores look better.

## Comparison 2: the same agent with each backend

Later, compare `agentic_vector`, `agentic_keyword` and `agentic_hybrid` with the same Codex model/configuration, instructions, tool schema, top-k, source corpus, timeout and maximum four search calls. Fix mode outside the model's tool arguments and propagate it explicitly into the MCP subprocess.

The agent may choose different follow-up queries because it sees different results. This measures the **whole agent + retrieval system**, not just the ranking algorithm. Record first-search coverage, combined coverage, actual queries, final answer/citations, tool calls, branch searches, latency, failures and available token usage separately. Search count is an efficiency measure, not the primary success criterion.

Keep the free-text AI judge paused. Source/citation validity remains mechanically checkable; semantic answer quality requires separately labelled human inspection or a future explicitly resumed judge. Evidence coverage alone must not become an answer-correctness badge.

One run of all 18 questions in three answer-producing modes would use **54 generation sessions**, already exceeding the current shared 40-session protocol. A later agentic experiment needs a predeclared smaller balanced subset or a separately authorized protocol; do not reset/evade the historical ledger. Decide repeats and case selection in advance, retain every failed attempt, and never retry only weak answers.

## How a person could compare the results

Extend the evaluation experience with a separate offline comparison page initially. It should open without model calls and show **Vector / Keyword / Both** beside one another for a selected question:

- Expected facts first, with the source sections that can establish them.
- Each mode's `found / expected` facts and visibly missing facts.
- Ordered passage cards with source/date, plus branch rank details for hybrid on expansion.
- A compact table across questions showing improvements, regressions and ties; filter by question type.
- Retrieval timing/work alongside quality, with N/A and execution failures visible.

Do not compare the numeric BM25, cosine and RRF scores across columns. Label the coverage as retrieval evidence, and display “answer not generated” in retrieval-only results. Keep the existing demo unchanged in this first step. Its current answer report hardcodes four arms and six-case comparison assumptions, so a new three-mode view should not pretend to be a drop-in existing report option.

## Implementation boundaries and verification for a future task

1. Shared retriever configuration + BM25 reuse + RRF; typed score kinds and versioned settings/provenance. Keep dense behavior as the default. Keyword-only mode must not load/download embeddings.
2. Local retrieval-only runner + reusable metrics + saved JSON + simple offline comparison view.
3. Optional later MCP mode propagation and agentic arms, with updated trace validation and answer reports.

Relevant seams: [`lib.rs`](../../src/lib.rs) for `Index`, `Hit` and dense-shaped `SearchResult`; [`manifest.rs`](../../src/manifest.rs) for embedding-specific provenance; [`experiment/types.rs`](../../src/experiment/types.rs) for arms and records; [`runner.rs`](../../src/experiment/runner.rs) and [`metrics.rs`](../../src/experiment/metrics.rs) for strict dense/score-kind assumptions; [`mcp.rs`](../../src/mcp.rs), [`main.rs`](../../src/main.rs) and [`codex.rs`](../../src/codex.rs) for the subprocess boundary; [`report.rs`](../../src/experiment/report.rs) for fixed arm/layout assumptions. Preserve old artifact readers rather than relabelling historical records.

A future implementation should verify fusion ordering on small hand-calculated examples, duplicates/ties/zero-match behavior, source/date preservation, no accidental gold indexing, mode propagation and trace rejection on mismatched provenance. Run a tiny end-to-end retrieval-only example with Codex deliberately unavailable, proving it still produces a correctly labelled report; check vector-default behavior and the independent browser experience if a page is built. These checks are proposed, **not performed** here.

The existing 18 questions support an honest, small regression comparison. They cannot establish general superiority. If tuning fusion settings or publishing unseen-quality claims later, use development questions for tuning and freeze new separately reviewed questions first. Do not enlarge the dataset merely to begin this comparison.
