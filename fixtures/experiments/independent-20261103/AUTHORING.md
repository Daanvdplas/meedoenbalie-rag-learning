# Independent source-authored reference candidate

Dataset: `independent-20261103-v1`. Origin: `agent_authored_not_human`.
Authored: 2026-11-03. Question reference date: 2026-10-20 (existing loader contract).
Status: **candidate; not independently reviewed or frozen**. Do not measure this set before independent review and an immutable freeze manifest.

This is a question-level holdout against the unchanged five-document fictional corpus at `ff1f13f20ff1be4329e454c278c5407b5fe715ce`. It is not a human reference or a novel-document generalization test. All people, locations, activities and dates remain fictional; this is not an existing COA offer.

The author read only the five source activity Markdown files and project workflow instructions. The author did not read existing questions, expectations, ranking code, rankings or measurement results. No search-engine, embedding, model, Codex judge or answer-generation calls were made. The data were authored before any retrieval output for this set.

- `questions.json`: loader-compatible inputs only; includes exact-byte source hashes and corpus hash.
- `expectations.json`: factual goals with OR alternatives of AND document/section references, explicit unknowns, forbidden claims and relevant sections.
- `reference-details.json`: expected actions, hard conditions versus soft preferences, evidence roles and dated verbatim source quotations. No typed-filter input is supplied; do not derive hidden runtime filters from expected outcomes or case IDs.

There are 12 questions: 9 with positive source facts (5 source-answer/comparison cases and 4 known-fact/unknown mixtures), 2 closed-corpus no-match cases with exclusion proof, and 1 clarification case without a retrieval target. The two English questions both request source-findable positive facts; one also asks about an unknown essential condition.

Every factual goal is required. Within each goal, any outer evidence alternative is sufficient; all references inside one alternative are jointly required. Canonical document ID and section ID identify a source together because section IDs repeat across documents. Positive support, valid exclusion, explicit unknown and clarification are separate roles. Exclusion evidence is not a recommended activity. The clarification case is excluded from retrieval denominators and requires manual assessment of its expected action.

Source dates do not establish current vacancies. Historical capacity observation dates must remain distinct from document dates. A missing condition is unknown, not a confirmed match or confirmed exclusion. No-match statements apply only to these five sources. Any eventual suggestion remains voluntary and must be discussed with the resident; the demo sends nothing, enrolls nobody and writes no dossier.

Independent review must validate source faithfulness, ambiguity, non-overlap with existing questions, coverage and the data-only contract. After any corrections, freeze the source hashes, question and expectation hashes, provenance, candidate commit and reviewer decision before the first measurement. Preserve candidate history. The separate reviewer, not this author, determines whether this set is ready to freeze.
