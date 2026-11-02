# Een gekozen zoekvariant gebruiken met Codex

De lokale vergelijking (`lab run`) gebruikt geen Codex en geen AI-beoordelaar.
Met `lab ask` kun je daarna één zoekmethode en één opslag-/structuurprofiel in
een echte Codex-uitvoering gebruiken. Codex krijgt alleen de read-only zoektool.
De gekozen instellingen en getypeerde filters staan vast tijdens de uitvoering;
Codex mag maximaal vier zoekvragen maken, steeds met dezelfde gekozen top-k.

## Zelf uitvoeren

Voer dit uit vanuit de repository. Er moet een aangemelde, bijpassende Codex-runtime
beschikbaar zijn, zoals voor de bestaande demo. De bestaande `COA_CODEX_BIN` en
`COA_CODEX_MODEL`-instellingen blijven bruikbaar. Er is geen nieuwe API-sleutel nodig.

```sh
rag_output_dir=$(mktemp -d /tmp/coa-selected-rag.XXXXXX)
cargo run --locked --features sqlite -- lab ask --live \
  --methods bm25 --profiles sqlite_activities \
  --question 'Welke activiteit kan ik op dinsdag binnen doen, hoe laat en waar?' \
  --output "$rag_output_dir/antwoord-1.jsonl" \
  --ledger "$rag_output_dir/budget.json" \
  --max-generations 6 --max-seconds 1200
```

De eerste opdracht maakt een begrensd budget: maximaal zes sessies in twintig
minuten, één tegelijk, en maximaal 120 seconden per sessie. Gebruik hetzelfde
ledger en dezelfde limieten voor vervolgvragen, met steeds een nieuw uitvoerbestand.
Ook mislukte pogingen verbruiken een reservering. Een verlopen of beschadigd ledger
wordt niet automatisch vernieuwd; bestaande resultaten worden nooit overschreven.
Ctrl+C stopt de eigen subprocessen en bewaart de gereserveerde poging als geannuleerd.
Ook runtime-startup valt binnen deze grens: de `--version`-probe wordt asynchroon
uitgevoerd en bij timeout/annulering met zijn eigen subprocessgroep gestopt.
Het controleren van grote uitvoerbare bestanden onderbreekt de deadline niet.
Debug-builds optimaliseren alleen de bestaande SHA-256-implementatie, zodat deze
integriteitscontrole de startup niet onnodig vertraagt. De hashwaarden en het
releaseprofiel blijven hetzelfde.
Deze aantallen zijn sessies, geen bekende aantallen provider-HTTP-requests.

`--methods` accepteert precies één van `dense`, `bm25`, `hybrid_rrf` en
`hybrid_weighted`. `--profiles` accepteert precies één van `documents_sections`,
`documents_activities`, `sqlite_sections` en `sqlite_activities`.
Voor alleen documenten kan de opdracht zonder `--features sqlite` worden gebouwd.

De geavanceerde instellingen van `lab plan` zijn ook beschikbaar: onder andere
`--top-k 3`, `--selection activity_expansion`, `--mmr on` en `--context-limit 1200`.
Getypeerde filters kunnen als JSON met `--query-filters` worden meegegeven;
`--apply-filters on` activeert ze. Ze gelden voor alle vervolgzoekvragen in de sessie.
Er wordt geen filter afgeleid uit een test-ID of uit verwachte antwoorden.

BM25 initialiseert geen embeddingmodel. Vector- en hybridvarianten gebruiken
dezelfde lokale MiniLM en cache als `lab run`; de eerste voorbereiding kan
modelbestanden downloaden. Draai de lokale vergelijking eerst om deze cache
te vullen. `--model-cache` en `--sqlite-dir` kunnen expliciet worden ingesteld.

## Wat wordt bewaard en gecontroleerd?

Het aparte, versiegebonden live-JSONL-bestand bewaart de gekozen configuratie,
budgetreservering, eventuele Codex-uitvoer en echte zoekacties met passages,
componentrangen/scores, brondata, voorbereiding en gemeten zoektijden.
BM25- of hybrid-scores worden niet als cosinusovereenkomst of zekerheid gelabeld.
Ontbrekende tokenusage blijft onbekend.

Harde controles binden de zoekacties aan de vaste configuratie en de oorspronkelijke
secties. Citaten moeten letterlijk voorkomen in werkelijk aangeboden passages van
het juiste document en de juiste brondatum. Veranderde broninhoud, verkeerde
scoretypen, verzonnen toegepaste filters en gewijzigde contextboekhouding worden
geweigerd. Stille terugval naar een andere zoekmethode is niet toegestaan.

Een geslaagde controle bewijst geen juist antwoord. `answer_quality_status` blijft
`not_assessed`; de AI-beoordelaar blijft uit. De live-antwoordbestanden hebben een
eigen schema en worden niet voorgesteld als retrieval-only rapporten.

De SQLite-opslag en embeddings zijn lokaal. De vraag en geselecteerde toolinformatie
gaan naar Codex via de bestaande aanmelding. Gebruik alleen fictieve informatie.
Er worden geen dossiers aangepast, berichten verstuurd of deelnemers aangemeld.
