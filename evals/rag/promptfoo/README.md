# RAG-evaluaties met promptfoo

Dit is de canonieke uitvoerroute voor de RAG-evals. Promptfoo bepaalt de vragen,
configuraties, herhalingen en assertions. De Rust-code voert uitsluitend lokale
retrieval of een afgeschermde Codex/MCP-sessie uit. De oude Rust-beoordelaars en
historische rapporten blijven beschikbaar voor compatibiliteit en controle;
hun scores worden niet als nieuwe promptfoo-scores overgenomen.

## Starten zonder modelcalls

Benodigd: Rust en Node **22.22.0 of hoger**. Vanuit de repo-root:

```sh
npm ci
cargo build --locked --features sqlite
npm run eval:rag -- --suite all-local
```

Gebruik eventueel `--binary /absoluut/pad/naar/meedoen-retrieval` en
`--model-cache /absoluut/pad/naar/.model-cache`. SQLite moet in de gekozen binary
zijn ingebouwd om alle vier opslagprofielen werkelijk te vergelijken.

| Suite | Wat draait er standaard? | Modelgebruik |
| --- | --- | --- |
| `garden` | Vijf vaste tuinvragen × dense en historische Jaccard-woordoverlap: 10 rijen. | Geen antwoordmodel. |
| `retrieval` | Vier bestaande vraagsets, samen 50 vragen × vier methoden × vier opslag-/structuurprofielen: 800 rijen. | Geen antwoordmodel. |
| `quality` | De bestaande contextproef: legacy 18 × 24 configuraties en independent 12 × 28 configuraties: 768 rijen. | Geen antwoordmodel. |
| `compact` | Exacte Rust-projectie van één bewaarde canonieke T02-retrievaltrace, met bron-/metadatabehoud, SHA-256 en UTF-8-bytemetingen. | Geen retrieval of modelcall. |
| `agentic` | De bestaande selectie van tien vragen × vier zoekmethoden: 40 afzonderlijke Codex-sessies. | Expliciete live-opt-in. |
| `models` | Tien aangescherpte vragen × Sol/Luna × drie herhalingen: 60 sessies. | Expliciete live-opt-in. |
| `compact-live` | De drie bestaande T02/T05/P05-paren met werkelijk `full` en `compact` transport: zes sessies. | Expliciete live-opt-in. |

`all-local` kiest de eerste vier suites. De vier methoden zijn `dense`, `bm25`,
`hybrid_rrf` en `hybrid_weighted`; de profielen zijn `documents_sections`,
`documents_activities`, `sqlite_sections` en `sqlite_activities`.

Een kleine lokale selectie:

```sh
npm run eval:rag -- --suite retrieval --dataset legacy --cases T05,P05 --methods bm25 --profiles documents_sections
```

De volledige vraag-/configuratiematrix staat in [cases.cjs](cases.cjs).
Bestaande vragen, beschermde labels en bronnen blijven bytegelijk. De huidige
twaalfdocumentendemo heeft hiermee geen nieuw, onafhankelijk goldlabelcorpus.

## Codex blijft de enige modelroute voor evals

Een modelproef vraagt `--live`, een expliciet ledger en sessie-/tijdgrenzen.
Dit voorbeeld is een **kleine uitvoercontrole**, geen volledige modelvergelijking:

```sh
npm run eval:rag:live -- --suite models --phase smoke --cases RAG26-01 --repeat 1 --ledger evals/rag/results/local/model-smoke/ledger.json --max-generations 8 --max-seconds 900
```

De voorbeeldselectie vraagt precies één Sol- en één Luna-sessie. De standaard
`models`-suite behoudt de oorspronkelijke afwisselende volgorde van 60 expliciete
case/model/herhalingsslots. Gebruik `--phase pilot` voor de eerste vier sessies.
`--phase remaining --approval <bestand>` vereist daarna de oorspronkelijke, door
een onafhankelijke reviewer gegeven hashgebonden pilotgoedkeuring. Een schone
onderbreking hervat bij het eerstvolgende onuitgevoerde slot; onzekere pogingen
stoppen zonder herhaling. Een selectie met `--phase smoke --repeat 1` is een
begrensde technische proef en geen volledige modelvergelijking. Zonder fase
blijft het plan zichtbaar, maar start geen modeluitvoering.

Dezelfde gedeelde ledger begrenst alle
verdere sessies gezamenlijk; een volledig nieuwe vergelijking van zestig
sessies vereist dus een afzonderlijk, expliciet passend budget.

De globale teller reserveert vóór elke Codex-start. Mislukte of onzekere pogingen
tellen mee, stoppen vervolguitvoering en worden niet automatisch herhaald. Alleen
aantoonbare `model_output_invalid`- en `model_contract_violation`-uitkomsten mogen
binnen de modelvergelijking als gefaalde modelantwoorden doorgaan; hun bronkwaliteit
blijft onbekend. Authenticatie-, netwerk- en tracefouten stoppen de proef.
Een lopend proces is begrensd op de resterende gezamenlijke tijd, met ruimte
voor opruiming. Native deelledgers zijn gescheiden per dataset/model/treatment;
ze verruimen de globale grens niet. Een bestaand ledger wordt nooit stil gereset
of verlengd. Cache en automatische retries staan uit, met één uitvoering tegelijk.

De provider verwijdert OpenAI API-sleutelinstellingen uit de subprocessomgeving.
Hij laadt geen sleutelbestand en gebruikt geen OpenAI-provider of modelgrader
van promptfoo. Alleen vraagtekst en vastgelegde zoekinstellingen gaan naar
Codex; labels, bronverwachtingen en beoordelingscriteria blijven bij assertions.

## Resultaten lezen

Elke uitvoering krijgt een nieuwe map onder `evals/rag/results/local/` met echte
promptfoo-JSON, samenvattingen en afgeschermde native artifacts. Lokale retrieval
wordt per exact geselecteerde configuratie voorbereid om model/indexopbouw te
delen. De provider leest alleen de bytegebonden records van datzelfde verse
execution-ID; dit is geen hergebruik van oude benchmarkresultaten of promptfoo-cache.

`gradingResult.componentResults[].metadata.source_metrics` bewaart onder meer:

- complete bewijssets en afzonderlijke gedeeltelijke doeldekking;
- relevante bronnen, gemiste doelen en rangorde waar labels dat toelaten;
- minimale sectie-/activiteitcapaciteit tegenover de gekozen top-k;
- `null`/niet-toepasselijk bij ontbrekende positieve doelen;
- uitsluitend bronkwaliteit: vrije antwoordkwaliteit blijft `not_assessed`.

Een complete uitvoering met ontbrekende bronnen blijft een echte assertionfail.
Een onbekend-/verduidelijkingsgeval zonder positief doel krijgt alleen een
technische integriteitscontrole, zonder perfecte of nul-bronkwaliteitsscore.
Providerfouten, ongeldige traces en unsupported profielen blijven afzonderlijke
uitvoerproblemen. Een CLI-exitcode 100 kan daarom een normale, bewaarde
inhoudelijke misser betekenen; controleer de foutcategorieën in het resultaat.

De exportadapter werkt de bestaande eigen pagina `/resultaten/` bij vanuit deze
promptfoo-resultaten. Alleen toegestane waarneembare velden worden gepubliceerd;
ruwe runtimepaden, credentials en interne reasoningevents blijven erbuiten.
De zestig historische runs in `../site/results.json` blijven ongewijzigd.

De oorspronkelijke [dertien-callmeting](../docs/compact-mcp.md) blijft historisch
bewijs. De standaard `compact`-suite hierboven gebruikt één andere, expliciet
herleidbare T02-record en claimt die dertien oude responses niet opnieuw te hebben
uitgevoerd. Payloadbytes zijn geen antwoordkwaliteit, providertokens of kosten.

## Implementatie en bronnen

- [provider.py](provider.py): uitvoering en begrensde Codex-ledgers.
- [prepare.py](prepare.py): verse, exacte lokale requestgroepen, zonder beoordeling.
- [assertions.py](assertions.py): canonieke bron-/tracecontroles en eigen scores.
- [data-pins.json](data-pins.json): hashes van bestaande inputbytes, geen nieuwe labels.

Gepind: **promptfoo 0.125.1**, gepubliceerd **25 oktober 2026 om
02:12:24.374 UTC**, volgens de [officiële npm-registry](https://registry.npmjs.org/promptfoo).
Versie en uitvoering gecontroleerd op **13 november 2026**. De officiële
[Python-providerdocumentatie](https://www.promptfoo.dev/docs/providers/python/)
en [Python-assertions](https://www.promptfoo.dev/docs/configuration/expected-outputs/python/)
zijn als implementatiereferentie gebruikt; het werkelijk geïnstalleerde
0.125.1-contract is met de CLI gecontroleerd.
