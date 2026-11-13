# Retrieval lab: implementation contract

**Actuele runner:** [promptfoo](../README.md) bestuurt de RAG-proeven. De native commando’s hieronder beschrijven de behouden uitvoeringsonderlaag of historische rapportage, niet een tweede actuele benchmark/scorer.

Approved scope, 2 November 2026. This is an independent experiment with five fictional activities, thirty unchanged source sections and eighteen existing development/regression questions. It is not a production COA system or a new held-out benchmark.

## What can be compared

The three main axes are **search method × storage/structure profile × assessment type**.

| Search method | Meaning |
| --- | --- |
| `dense` | Existing multilingual MiniLM embeddings and exact cosine |
| `bm25` | Existing Rust BM25 formula, versioned positive-score candidate policy |
| `hybrid_rrf` | Reciprocal-rank fusion of dense and positive BM25 candidates |
| `hybrid_weighted` | Explicitly normalized weighted fusion; component scores retained |

| Profile | Storage | Representation |
| --- | --- | --- |
| `documents_sections` | Documents/in-memory adapter | Original canonical source sections |
| `documents_activities` | Documents/in-memory adapter | Activity records, source-backed typed fields and original sections |
| `sqlite_sections` | Optional SQLite adapter | Persisted canonical source sections and embeddings |
| `sqlite_activities` | Optional SQLite adapter | Persisted activity records, source-backed typed fields and original sections |

These are sixteen retrieval variants per advanced-settings profile: **16 × 18 = 288** local retrieval records. The complete catalogue can be listed before running it. A missing SQLite build feature produces an explicit unsupported result, never a silent document or keyword fallback. Storage and representation can each be compared while holding the other fixed.

Assessment happens **after retrieval on stored runs**. It never repeats retrieval or generation:

| Assessment | Status and contract |
| --- | --- |
| Deterministic | Source/identity/config checks, expected-evidence coverage and ranking metrics where labels permit |
| Manual | Initially blank, explicit human judgement and notes tied to the immutable run/config/case identity; export/import |
| AI | Visible as paused; no automatic AI-judge calls in this implementation run |

Retrieval-only records contain no generated answer. Answer correctness and answer-citation validity are therefore **N/A**, not passed. Evidence coverage means expected support was offered, not that the whole answer is right. Incomplete relevance labels make precision/recall/MRR N/A where required. Missing facts and unknown answers are not counted as positive retrieval targets. Existing historical evaluation files remain readable and unchanged.

## Secondary controls

Candidate depth, seed top-k (initial presets 3 and 6), context ceiling, activity expansion, metadata filtering and MMR are independent advanced settings. They are not extra compulsory primary axes. Cold preparation, warm reuse and cache hit/miss counts are separate measured phases; an unmeasured phase stays null/N/A.

Activity expansion is **context selection, not a database schema**. It preserves seed hits, then admits whole unchanged related sections under the same context ceiling. Record seeds, added sections, overflow omissions, final section count and estimated context size. Do not call an estimated character-based token count actual provider usage. MMR is optional and must not be confused with a learned reranker or exact identity deduplication. No automatic age penalty is introduced.

Activities profiles must expose actual records and source-verified typed fields. Their field values retain supporting document/section/date references. Inputs may provide typed filters; a conservative general question parser may extract supported constraints, with the extraction and unsupported/unknown conditions visible. No question-ID routing, expectation-derived fields or forced answers. Unknown fields are neither confirmed matches nor known false values. Preferences such as nature interests are not automatically hard constraints.

## Shared boundary and ownership

- `src/retrieval/**`: pure search engine, public config/corpus/identity types, activity metadata and context-selection policies. Owns initial `lib.rs` module registration.
- `src/storage/**`, `Cargo.toml`, `Cargo.lock`: document and optional SQLite adapters, schema and persistent embedding cache. Storage imports public retrieval types; retrieval does not depend on SQLite or model initialization.
- New offline-lab modules: finite catalogue, local preparation, immutable retrieval records, deterministic reassessment, CLI commands and run summaries. Existing experiment records are not forged to represent generation-free runs.
- Codex/MCP integration: selected retrieval configuration fixed for the session, versioned honest score kinds and provenance; preserve source/date/citation guards and old routes.
- Report/UI: one inspectable local comparison page and manual-review controls driven by the stored record format.

Agreed core shape (the engine owner's concrete Rust definitions are authoritative):

```text
PreparedCorpus {
  sections: Vec<Section>,
  vectors: Option<Vec<Vec<f32>>>,
  activities: Vec<ActivityRecord>,
  identity: CorpusIdentity
}
CorpusIdentity { source_sha256, embedding_inputs_sha256, vectors_sha256, embedding: Option<EmbeddingIdentity> }
EmbeddingIdentity { provider, model, sdk, dimensions, encoding }
retrieve(corpus, request, config, query_vector: Option<&[f32]>) -> RetrievalOutcome
```

The common scorer consumes the same ordered canonical sections and f32 vectors from both stores. SQLite is a replaceable storage/index/cache adapter; no native L2 or approximate-vector candidate stage is introduced into this storage comparison. SQLite FTS5 would be a distinct future ranker, not an invisible replacement for current BM25.

Each stored retrieval record identifies run, case, configuration, method/profile/settings, query and parsed filters, corpus/source hashes, embedding identity and vector hash, execution status, actual candidates/seeds/offered sections, score kinds/component ranks, context size, timings and failures. Assessment artifacts bind to that immutable record and record their own type/version. Invalid combinations or missing embeddings remain visible failures/unsupported states; no fabricated scores or completion.

Runner/UI artifact contract: a versioned `RetrievalRecord` has `schema_version`, `run_id`, `case_id`, `question`, `config_id`, full method/profile/advanced `config`, `execution_status`, preparation identity, nullable `outcome: RetrievalOutcome`, stage-separated `timings`, and nullable `error`. Reuse the engine's `RetrievalOutcome` shape (`corpus_identity`, `filters`, `candidates`, `seeds`, exact-section `hits`, `omitted`, `estimated_context_tokens`, `token_estimator`, `work`); do not create a second incompatible hit representation. A separate `AssessmentRecord` binds `run_id` plus retrieval-record hash, case/config identity, assessment kind/version/status, and either deterministic results or an explicit human decision/notes. Manual and paused-AI states start empty; no fabricated answer or model verdict. Concrete Rust field definitions may refine this contract while preserving those boundaries and a schema version.

## Cache and source contracts

Document input and model identity determine cache eligibility. Preserve provider/model/SDK/encoding/dimension plus exact embedding-input hash; reject nonfinite/wrong-dimension vectors. An unchanged source can reuse embeddings; changed source/title/heading/text or model settings invalidates affected entries. Persist exact f32 values, source dates and document+section identity. Reopening the SQLite store must not require a new embedding call for a valid cache hit. Documents mode works independently in a build without the SQLite feature.

All source facts come from the fictional corpus, never evaluation expectations. Gold remains separately loaded only for assessments. A schedule does not establish current available places. Returning no keyword candidates means no lexical match, not proof that the corpus lacks an answer.

## Delivery and gates

Use isolated worktrees based on `origin/main` at `6f6c309ceed5d6b69ffcc8923f89c13ac85711ae`. Shared checkout, user changes and unrelated PRs remain untouched. Product PRs target `feat/retrieval-lab`; root coordinates independent review before any integration merge. A final umbrella PR targets main and gets independent review again.

1. Unit/contract tests first: source identity, exact storage parity, filtering unknowns, fusion, zero lexical matches, deterministic ties, whole-section budgets, cache invalidation and strict output schemas.
2. Verify all-feature SQLite build and document-only build. Run existing deterministic checks; preserve the current demo's source/citation behavior.
3. Execute the full real-MiniLM local matrix (288 records per declared advanced profile), with zero Codex and judge calls. Account for every planned row and failure; never present fixtures as live measurements.
4. Run at most six explicitly bounded Codex generations through selected configurations using a fresh budget ledger, zero judges. This checks CLI/MCP wiring, not a statistically complete answer-quality comparison.
5. The new HTML page shows configuration/assessment selection, expected facts, actual passages, missing support, denominators, quality/work comparisons, failure/unsupported states and manual export/import. No flowchart or decorative diagram is required.
6. A separate agent performs browser use of the new page (desktop/mobile, keyboard, filters, case details, manual review/export/import) and end-to-end CLI/data/UI verification. Serve locally for user review; public hosting remains out of scope.

No quality winner, speedup, human calibration or complete answer guarantee is claimed before the corresponding evidence exists.

## Tracking

Epic [#67](https://github.com/Daanvdplas/meedoenbalie-rag-learning/issues/67) owns native subissues [#68](https://github.com/Daanvdplas/meedoenbalie-rag-learning/issues/68) (contract), [#69](https://github.com/Daanvdplas/meedoenbalie-rag-learning/issues/69) (search), [#70](https://github.com/Daanvdplas/meedoenbalie-rag-learning/issues/70) (storage/cache), [#71](https://github.com/Daanvdplas/meedoenbalie-rag-learning/issues/71) (metadata/context), [#72](https://github.com/Daanvdplas/meedoenbalie-rag-learning/issues/72) (matrix/assessments), [#73](https://github.com/Daanvdplas/meedoenbalie-rag-learning/issues/73) (CLI/MCP), [#74](https://github.com/Daanvdplas/meedoenbalie-rag-learning/issues/74) (HTML/manual review), and [#75](https://github.com/Daanvdplas/meedoenbalie-rag-learning/issues/75) (independent end-to-end evidence).
