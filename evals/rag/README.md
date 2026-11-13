# RAG-evaluaties

Deze proeven meten het ophalen en aanbieden van bronnen. Een volledig gevonden bronset bewijst niet dat een geschreven antwoord goed is.

| Onderdeel | Waar? |
| --- | --- |
| Vijf oorspronkelijke tuinvragen | `datasets/garden/`, `src/legacy.rs`, [historische rapporten](results/legacy/) |
| Vier zoekmethoden en vier opslag-/structuurprofielen | [Retrieval-lab](docs/retrieval-lab.md), `src/` |
| Top-k, contextplafond, activiteituitbreiding en filters | [Meetlat](docs/rag-quality-hardening.md) |
| Codex met gericht vervolgzoeken | [Protocol](docs/agentic-search-comparison.md), `datasets/presentation-selection.json` |
| Sol/Luna: tien vragen, drie herhalingen | `scripts/model_trials.py`, gecureerde resultaten in `site/` |
| Compact/full-MCP-payload | [Protocol en offline replay](docs/compact-mcp.md) |

De vier uitgebreide vraagsets staan één keer in [shared/datasets](../shared/datasets/). De resultatenwebsite gebruikt de bestanden in `site/`; URL's zoals `/resultaten/` blijven hetzelfde.

Vanuit de repository, zonder modelcalls:

```sh
cargo run --locked -- lab plan --dataset legacy
cargo run --locked -- experiment validate
python3 -m unittest discover -s evals/rag/scripts -p 'test_*.py'
```

Voor één lokale BM25-uitvoering:

```sh
mkdir -p evals/rag/results/local
cargo run --locked -- lab run --dataset independent-20261103 --methods bm25 --profiles documents_sections --output evals/rag/results/local/bm25.jsonl
cargo run --locked -- lab assess --dataset independent-20261103 --input evals/rag/results/local/bm25.jsonl --types deterministic --output evals/rag/results/local/bm25-assessed.jsonl
```

Dit maakt geen geschreven antwoorden. AI-beoordeling in het retrieval-lab blijft gepauzeerd; menselijke oordelen worden niet door agents ingevuld.
