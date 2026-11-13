# RAG-evaluaties met promptfoo

Promptfoo bestuurt nu de RAG-vraagsets, configuratiematrix, herhalingen en assertions. De bestaande Rust-code voert alleen lokale zoekacties of een begrensde Codex-sessie uit. De beoordeling komt uit de nieuwe promptfoo-assertions; de oude native rapportlezers blijven beschikbaar voor historische bestanden.

| Suite | Wat wordt gemeten? | Standaard |
| --- | --- | --- |
| `garden` | MiniLM tegenover oorspronkelijke Jaccard-woordoverlap | 5 tuinvragen × 2 methoden |
| `retrieval` | Vier zoekmethoden × vier opslag-/structuurprofielen | 50 vragen × 16 configuraties |
| `quality` | De vastgelegde context-, top-k- en filtervarianten | 18 legacyvragen × 24 + 12 onafhankelijke vragen × 28 |
| `agentic` | Codex met maximaal vier gerichte zoekacties | 10 geselecteerde vragen × 4 zoekmethoden |
| `models` | Bronvolledigheid en zoekwerk van Sol/Luna | 10 vragen × 2 modellen × 3 herhalingen |
| `compact` | Behoud van bronnen/metadata en exacte bytes bij de echte MCP-projectie | 1 bewaard T02-record; offline replay |
| `compact-live` | De vastgelegde full/compact-Codexparen | 3 paren, expliciet live |

De oude 13-call-payloadmeting blijft historische herkomst; die ruwe inputbundel is niet aanwezig in deze checkout. De nieuwe standaardreplay gebruikt eerlijk één beschikbaar record en claimt geen nieuwe model-, kosten- of snelheidsmeting. [Historisch protocol](docs/compact-mcp.md).

## Lokaal uitvoeren

Benodigd: Rust, Node 22.22 of nieuwer en de bestaande modelcache. Promptfoo is vastgezet op **0.125.1** (npm-publicatie 25 oktober 2026; gecontroleerd 13 november 2026). [Officiële versie](https://www.npmjs.com/package/promptfoo/v/0.125.1).

Vanuit de repository:

```sh
cargo build --locked --features sqlite
npm ci --ignore-scripts --no-audit --no-fund
npm run test:rag
npm run eval:rag -- --suite all-local
```

`all-local` doet verse retrieval, maar geen Codex-generaties of AI-judging. Het gebruikt alle bewaarde vraagsets; voor de nieuwe twaalfdocumentendemo zijn geen nieuwe goldlabels verzonnen. De SQLite-feature is nodig voor alle zestien profielen. Een kleine selectie kan met:

```sh
npm run eval:rag -- --suite retrieval --dataset legacy --cases T05,P05 --methods bm25 --profiles documents_sections
```

Een bronassertie mag falen. T05 heeft bijvoorbeeld vier benodigde secties en kan bij top-k drie onvolledig zijn. Dat blijft een echte mislukte broncontrole, geen installatiefout of groen gemaakte drempel. Gevallen zonder positief bewijsdoel krijgen geen perfecte retrievalscore: bronkwaliteit blijft N/A en valt buiten de noemer. Antwoordkwaliteit wordt hier niet beoordeeld.

Nieuwe rawresultaten, native bewijsrecords, logs en promptfoo-opslag staan onder genegeerde `results/local/<run>/`. Telemetrie, delen, promptfoo-cache en automatische retries staan uit. Voorbereide lokale batches komen uitsluitend uit het precieze promptfoo-plan en hebben een nieuwe execution-ID, bron-/config-/runtimebinding en oorspronkelijke zoektijden. Ze zijn geen replay van oude scores.

## Modelproeven blijven via Codex

Alleen `agentic`, `models` en `compact-live` starten Codex. Daarvoor zijn expliciet `--live`, een nieuw begrensd ledger en een sessie-/tijdplafond verplicht. De provider verwijdert API-keyinstellingen uit de childomgeving en heeft geen OpenAI-APIprovider of judge-fallback.

Een voorbeeld van een bewuste kleine selectie:

```sh
npm run eval:rag:live -- --suite agentic --cases P01 --methods dense --ledger evals/rag/results/local/codex-ledger.json --max-generations 1 --max-seconds 180
```

Bij hervatten hoort hetzelfde ledger; fouten en onzekere pogingen tellen mee en worden niet automatisch herhaald. Modelvergelijkingen hebben standaard drie herhalingen; een expliciete technische rookproef kan `--repeat 1` gebruiken zonder die als volledige benchmark te presenteren.

De volledige `models`-vergelijking behoudt de oorspronkelijke, afwisselende Sol/Luna-volgorde van zestig pogingen. Zij begint met vier vaste pilotpogingen en stopt daarna. `remaining` vereist een onafhankelijke `PILOT_APPROVED.json`, gebonden aan de exacte preregistratie en de vier bewaarde pogingen. Beide fasen gebruiken hetzelfde ledger en dezelfde ongewijzigde limieten; Codex/host, runtime, bronnen, embeddings en vectoridentiteit blijven gepind. Alleen de twee bewezen modelcontract-/antwoordfouten mogen als mislukte modeluitkomst doorgaan. Onzekere, authenticatie-, netwerk- en tracefouten stoppen de proef zonder retry.

```sh
npm run eval:rag:live -- --suite models --phase pilot --ledger evals/rag/results/local/modelvergelijking/ledger.json --max-generations 60 --max-seconds 7200
npm run eval:rag:live -- --suite models --phase remaining --approval evals/rag/results/local/modelvergelijking/PILOT_APPROVED.json --ledger evals/rag/results/local/modelvergelijking/ledger.json --max-generations 60 --max-seconds 7200
```

Een afzonderlijke technische modelrookproef gebruikt expliciet `--phase smoke`, een caseselectie en `--repeat 1`, met maximaal acht sessies en 900 seconden. Dit vervangt de pilot of volledige vergelijking niet.


## Eigen resultaatpagina

De bestaande pagina `/resultaten/` leest de publieke whitelistprojectie `site/promptfoo-results.json`. Zij toont echte promptfoo-run-ID's, vraagsets, configuraties, assertionstatus, redenen, bronhits, N/A en uitvoeringsfouten. De oude zestig sessies blijven bytevast als uitklapbare historie. De pagina start geen modelcalls en er is geen nieuw publiek startendpoint.

De exporter neemt geen volledige config, machinepaden, auditbestanden of interne reasoning over. Rawresultaten blijven lokaal. Alleen bron-/testgegevens die bewust voor de pagina geselecteerd zijn worden gedeeld. [Provider- en assertiondetails](promptfoo/README.md).

Een gewijzigde assertion kan bestaande uitvoer opnieuw beoordelen zonder het model opnieuw te starten:

```sh
node evals/rag/scripts/regrade_promptfoo_results.mjs evals/rag/results/local/<run>/<suite>.json evals/rag/results/local/<nieuwe-herbeoordeling>
```

Dat maakt een nieuwe promptfoo-grading-ID met de oorspronkelijke uitvoerings-ID, datum en rawhash. Codex-bewijs wordt opnieuw aan de oorspronkelijke native records en bytegebonden audit gekoppeld. De pagina noemt dit expliciet herbeoordeling; het is geen nieuwe retrieval- of modelmeting.


De bestanden `src/`, `datasets/`, `fixtures/` en de oudere handleidingen blijven de onderliggende uitvoerders, bronverwachtingen en historische meetdefinities documenteren. Het oude `scripts/model_trials.py run` is uitgeschakeld; nieuwe RAG-modelproeven lopen via promptfoo. De native volledige-assistentbenchmark onder [coa-assistent](../coa-assistent/README.md) verandert niet.
