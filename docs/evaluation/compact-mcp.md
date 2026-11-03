# Compact MCP, volledige lokale audit

`lab ask --live` gebruikt standaard `--payload-mode compact`. Kies expliciet `full` om het oude modelzichtbare formaat te vergelijken. De keuze staat vast voor de hele sessie, naast dezelfde zoekmethode, opslagvorm, top-k en filters. De gewone demo en historische schema-1-bestanden behouden hun bestaande gedrag.

Compact geeft Codex één kopie van de exact aangeboden bronsecties, met document-ID, titel, sectie-ID, brondatum en oorspronkelijke tekst. Vraag, top-k, referentiedatum, broncatalogus en het bestaande configuratie/corpusmanifest blijven beschikbaar. Rankscores, alle kandidaten, opstartmetingen en de tweede kopie van dezelfde hits gaan naar het lokale auditbestand. We gebruiken geen `_meta` of `structuredContent` als verondersteld verborgen kanaal.

## Audit en controles

Bij `--output /tmp/antwoord.jsonl` maakt de CLI ook `/tmp/antwoord.mcp-trace.jsonl`, beide met Unix-bestandsrechten `0600`. Deze bestanden moeten nieuw zijn. Het pad komt uitsluitend van de CLI, nooit uit een zoekargument. De enige MCP-tool blijft de bestaande alleen-lezen zoekfunctie; goldbestanden worden niet geladen.

Voor elk succesvol zoekresultaat schrijft en synchroniseert de MCP-server eerst het volledige resultaat naar de trace. Daarna verschijnt het compacte antwoord op stdout. Het bevat een sessie-ID, volgnummer, request-ID, trace-ID en SHA-256 van het volledige resultaat. Deze IDs identificeren succesvolle zoekacties; zij zijn geen provider-HTTP-request-IDs. De trace bewaart geen private redeneerstappen, aanmelding of runtime-stdout.

Na generatie vergelijkt de parent elk daadwerkelijk aangeboden toolresultaat met de bijbehorende trace. Ontbrekende, dubbele, verwisselde, vreemde of afgebroken records worden geweigerd. Vervolgens controleren de bestaande guards de volledige configuratie, oorspronkelijke broninhoud, brondata en letterlijke citaten. Alle toolpogingen, ook ongeldige argumenten, tellen mee voor maximaal vier pogingen in een vaste lab-sessie. Een kloppende guard is geen oordeel over inhoudelijke antwoordkwaliteit.

Een timeout of Ctrl+C laat de al gesynchroniseerde trace staan. `trace_integrity_status=partial_not_validated` betekent dat bewaarde records leesbaar zijn, maar geen afgerond antwoord ermee verbonden kon worden. Een ongeldige trace geeft `invalid`, een mislukte uitvoering en geen verzonnen byte- of kwaliteitscijfers.

Nieuwe livebestanden gebruiken schema 2, met `payload_mode`, `transport_treatment_id`, `audit_trace` en `trace_integrity_status`. Historische schema-1-bestanden laden als `full`, zonder achteraf verzonnen auditgegevens. Het retrieval-config-ID blijft hetzelfde; het transport-treatment-ID onderscheidt de payloadkeuze.

`audit_trace.payload_bytes` meet de UTF-8 JSON-tekst in succesvolle MCP-tekstblokken, zonder JSON-RPC-envelope of foutmeldingen. `trace_bytes` meet het daadwerkelijk opgeslagen JSONL-bestand. Dit zijn geen providertokens. Usage en doorlooptijd blijven hun eigen metingen; lagere byteaantallen bewijzen geen kosten- of snelheidswinst.

## Offline replay

De pure Rust-projectie kan bestaande volledige toolresultaten opnieuw compact maken zonder model- of embeddingcalls. De ingebouwde expliciete replay controleert dezelfde 13 historische responses en oorspronkelijke bron/configguards:

```sh
COA_MCP_REPLAY_INPUT=/pad/naar/de/historische/live-map \
COA_MCP_REPLAY_OUTPUT=/tmp/mcp-replay-nieuw.json \
cargo test --locked --features sqlite --lib \
  replay_historical_model_payload_bytes_without_models -- --ignored --nocapture
```

Gebruik SQLite-feature omdat de historische reeks ook SQLite-configuraties bevat. De uitvoer moet nieuw zijn. Het rapport controleert exact dezelfde aangeboden secties, vraag, top-k, broncatalogus, datum en manifest, en vermeldt expliciet dat antwoordkwaliteit, latency, geld en providertokens niet gemeten zijn.

De replay van 3 november 2026 gaf **161.029 → 35.405 bytes (78,0% minder)** over 13 calls. Per call: full 5.973–17.547, compact 1.963–5.116. Dit gebruikt minified UTF-8 serde-JSON; JSON met extra spaties of ASCII-escaping geeft andere getallen. Het is een structurele payloadvergelijking op bestaande resultaten, geen nieuwe retrievalbenchmark of live A/B-test.

## Vooraf vastgelegde live vergelijking — wacht op toestemming

Nog niet uitgevoerd. Voorgestelde minimale reeks: drie paren = zes generatie-sessies, nul judge-calls:

| Regressievraag | Methode | Opslagvorm | Payloads |
|---|---|---|---|
| T02 | BM25 | documents_sections | full + compact |
| T05 | hybrid_rrf | documents_activities | full + compact |
| P05 | dense | documents_activities | full + compact |

Gebruik in elk paar exact dezelfde vraagtekst, model/runtime, bronversie en zoekinstellingen: sections, top-k 3, candidate-k 30, contextlimiet 1200, MMR uit, filters uit. Wissel de volgorde full/compact af. Leg vooraf alle commando's vast; verander vragen, drempels en instellingen niet na resultaten. De actuele gold gaat nooit naar het model. Gebruik nieuwe uitvoerpaden en één gedeeld budgetledger.

Controleer technische voltooiing, canonical-source/citationguards, tracebinding en beschikbare modelusage afzonderlijk. Bewaar echte zoekvragen, aangeboden secties, aantal toolpogingen, payloadbytes, tracebytes en doorlooptijd. Een agent kan in twee sessies andere vervolgvragen stellen: identieke retrieval is uitsluitend bewezen bij identieke zoekrequests, niet automatisch bij dezelfde gebruikersvraag. Vergelijk inhoudelijke antwoorden apart; die kwaliteitsbeoordeling blijft vooralsnog open. Zes sessies ondersteunen geen algemene ranglijst of statistisch onderbouwde snelheidsclaim. Een vooraf goedgekeurde herhaling kan twaalf sessies gebruiken; er starten geen modelcalls automatisch.
