# COA-assistent als geheel evalueren

Hier staan de proeven met gegenereerde antwoorden: vier RAG-aanpakken, harde bron-/citaatcontroles, de afzonderlijke inhoudelijke beoordelaar en diens kalibratie. Generatie en AI-beoordeling gebruiken Codex CLI. De modelbeoordelaar blijft voorlopig en is niet menselijk geijkt.

- [Actuele reproduceerhandleiding](docs/reproduce.md), [criteria](docs/metrics.md) en [datasetuitleg](docs/dataset.md).
- `src/`: de bestaande `experiment`-harness, inclusief judge, budget en rapportbouw.
- `fixtures/`: acht kalibratievoorbeelden, AI-referentie en handgemaakt rapportvoorbeeld.
- [Bewaarde uitvoering van 23–26 oktober 2026](results/2026-10-26/README.md): volledige ruwe geschiedenis, inclusief oorspronkelijke fouten en hashes.
- [Gedeelde vraagsets](../shared/README.md): twaalf pilotvragen, zes oorspronkelijke vergelijkingsvragen en latere versies, zonder duplicaten.

## Bestaand rapport openen zonder modelcalls

Voer dit uit vanuit de repository:

```sh
mkdir -p evals/coa-assistent/results/local
cargo run --locked -- experiment validate
cargo run --locked -- experiment report --input evals/coa-assistent/fixtures/report-example.jsonl --output evals/coa-assistent/results/local/voorbeeld.html
```

Het voorbeeld bevat handgemaakte rendererchecks, geen nieuwe RAG-meting.

Voor het echte oktoberarchief:

```sh
COA_EVIDENCE=evals/coa-assistent/results/2026-10-26
cargo run --locked -- experiment report --input "$COA_EVIDENCE/smoke-graded.jsonl,$COA_EVIDENCE/pilot-graded.jsonl,$COA_EVIDENCE/heldout-final-graded.jsonl" --output evals/coa-assistent/results/local/oktober.html
```

Het oktoberarchief, inclusief zijn README, is meegehasht en daarom ongewijzigd. Oude padnamen daarin blijven historische herkomstinformatie; bovenstaande commando's gebruiken de huidige plekken. Nieuwe modeluitvoeringen starten uitsluitend expliciet met `--live`, nieuwe uitvoerbestanden en een begrensd gedeeld ledger volgens de handleiding.
