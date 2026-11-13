# Saved ranking-role regression

`t02-dense-search.json` is one unchanged raw record copied from the sealed
2026-11-02 baseline `matrix.jsonl`, source commit
`0e2ee44` (full release provenance in the retained manifest). It is the T02
`documents_sections` dense search, not a new model execution or new evaluation set.

Its first-ranked section is labelled relevant as exclusion evidence, so the
historical relevance MRR is 1. Bicycle time appears second, but location is
missing, so no complete goal is covered. First goal-support reciprocal rank is
0.5. This checks
that a relevance-role difference cannot silently become a quality improvement.
Protected reference labels remain loaded separately from the existing dataset.
No embeddings, Codex or judge are called by this regression test.
