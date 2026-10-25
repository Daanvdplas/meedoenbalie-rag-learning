# Zelf de evaluatie uitvoeren

De evaluatie gebruikt vijf fictieve documenten: twaalf ontwikkel-/pilotvragen en zes vooraf apart gehouden vergelijkingsvragen. Je bekijkt bewijsdekking, antwoordkwaliteit en efficiëntie afzonderlijk. De kleine set en de voorlopige AI-beoordelaar onderbouwen een verkennende vergelijking.

## 1. Begin zonder modelaanroepen

Voer dit vanuit de repository uit. Rust en de afhankelijkheden zijn nodig; Cargo kan die bij de eerste build downloaden. Voor deze opdrachten is geen Codex-aanmelding nodig.

```sh
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo run --locked -- experiment validate
cargo run --locked -- experiment validate --questions fixtures/experiments/heldout-questions.json --expectations fixtures/experiments/heldout-expectations.json

mkdir -p /tmp/coa-rag-preview
cargo run --locked -- experiment report --input fixtures/experiments/report-example.jsonl --output /tmp/coa-rag-preview/voorbeeld.html
```

Open `voorbeeld.html` in je browser. Deze zes handgemaakte rendererchecks blijven als **voorbeelden** gemarkeerd. Filteren en uitklappen werken rechtstreeks vanaf schijf, zonder netwerk of modellen. De bestaande demo blijft apart.

## 2. Controleer de keten met twee bekende vragen

Hiervoor zijn een aangemelde, compatibele Codex-runtime en de lokale embeddingbestanden nodig; zie de [startinstructies](../../README.md#starten). MiniLM maakt embeddings lokaal. Antwoorden en afzonderlijke beoordelingen gebruiken OpenAI via Codex en kunnen usage verbruiken. `--live` is daarom verplicht.

Voor een nieuw zelfstandig traject kies je een eigen map. Bij hervatten gebruik je de **bestaande map en hetzelfde ledger**; dat is de budgetregistratie. Uitvoerbestanden krijgen een nieuwe naam en worden niet overschreven.

```sh
COA_EVAL_DIR=/tmp/coa-rag-evaluation
mkdir -p "$COA_EVAL_DIR"

cargo run --locked -- experiment run --live --cases P01,P06 --arms agentic_dense,full_context,dense_once,bm25 --output "$COA_EVAL_DIR/pilot-generation.jsonl" --ledger "$COA_EVAL_DIR/ledger.json" --max-cases 2 --max-generations 8 --max-judgements 0 --max-seconds 1200
```

Dit selecteert twee bekende ontwikkelvragen, met maximaal acht generaties. Iedere poging reserveert vooraf een sessie. Eén gezamenlijk ledger begrenst het traject op 40 generaties, 56 beoordelingen en een venster van vier uur; mislukte pogingen tellen mee. Er loopt één sessie tegelijk, maximaal 120 seconden. Een modelsessie kan intern meerdere providerrequests bevatten. De afzonderlijke MCP-zoekcalls staan wel in de trace.

Een verlopen venster stopt het werk. Vernieuw het ledger niet om de tellers te omzeilen. Alleen een expliciet toegestane voortzetting mag het bestaande venster verlengen; de [audit en sessieplafonds blijven behouden](../../README.md#reproduceerbare-experimenten).

## 3. Leg de zes vragen langs dezelfde vier aanpakken

Controleer de pilot en zet prompt, brondata, criteria en instellingen vast vóór de eerste uitvoering op deze zes vragen. Zij zijn nieuwe combinaties van bekende bronfeiten. Pas de generator niet op de uitkomsten aan en noem een volgende afgestemde uitvoering dan opnieuw ontwikkeling.

```sh
cargo run --locked -- experiment run --live --questions fixtures/experiments/heldout-questions.json --expectations fixtures/experiments/heldout-expectations.json --cases T01,T02,T03,T04,T05,T06 --arms agentic_dense,full_context,dense_once,bm25 --output "$COA_EVAL_DIR/heldout-generation.jsonl" --ledger "$COA_EVAL_DIR/ledger.json" --max-cases 6 --max-generations 24 --max-judgements 0 --max-seconds 3600
```

De aanpakken zijn Codex met vervolgzoeken, alle documenten vooraf, één MiniLM-zoekactie en één BM25-zoekactie. De vaste bron- en vraaghashes, modelbestanden, programma-identiteit, werkelijke context en fouten blijven in het artifact staan. Modelaliassen en provideruitvoering beperken exacte herhaalbaarheid.

## 4. Beoordeel bestaande antwoorden zonder opnieuw te genereren

Deze opdrachten maken alleen afzonderlijke beoordelingen. Zij genereren geen nieuwe antwoorden en voeren geen nieuwe zoekacties uit. De beoordelaar krijgt het volledige opgeslagen antwoord, de aangeboden context en de apart geladen criteria/referentiepassages; hij heeft geen tools.

```sh
cargo run --locked -- experiment grade --live --input "$COA_EVAL_DIR/pilot-generation.jsonl" --output "$COA_EVAL_DIR/pilot-graded.jsonl" --ledger "$COA_EVAL_DIR/ledger.json" --max-judgements 8 --max-seconds 1200

cargo run --locked -- experiment grade --live --questions fixtures/experiments/heldout-questions.json --expectations fixtures/experiments/heldout-expectations.json --input "$COA_EVAL_DIR/heldout-generation.jsonl" --output "$COA_EVAL_DIR/heldout-graded.jsonl" --ledger "$COA_EVAL_DIR/ledger.json" --max-judgements 24 --max-seconds 3600
```

Ongeldige uitvoer, een time-out en een onbeslist criterium leveren geen positieve kwaliteitsclaim op. Een hoge AI-score heft een harde fout niet op. Een beoordeling opnieuw uitvoeren krijgt een nieuw outputbestand en gebruikt hetzelfde resterende budget.

## 5. Open de echte resultaten lokaal

```sh
cargo run --locked -- experiment report --input "$COA_EVAL_DIR/pilot-graded.jsonl,$COA_EVAL_DIR/heldout-graded.jsonl" --output /tmp/coa-rag-preview/resultaten.html
```

Open `resultaten.html`. De gepaarde tabel houdt dezelfde zes vragen naast elkaar, met noemers, ontbrekende uitvoeringen en technische fouten. Eén unieke run krijgt zijn ID; bij herhalingen kiest het rapport geen automatische winnaar. Volledige historie blijft opvraagbaar. Voeg een generatiebestand en zijn beoordeelde kopie niet samen: dezelfde run-ID mag maar één keer voorkomen.

Het rapport valideert de huidige datasetversies en rekent de metingen opnieuw uit. Bewaar de oorspronkelijke JSONL-bestanden, het ledger en de bijbehorende codeversie voor latere controle. Een HTML-rapport is een leesbare weergave, geen ondertekend attest. De [meetdefinities](metrics.md) en [rapportuitleg](report.md) leggen floats, N.v.t., onbekende usage en de grenzen van tijdsvergelijking uit.

## 6. Houd AI-referentie en privacy expliciet

```sh
cargo run --locked -- experiment calibrate --live --output "$COA_EVAL_DIR/ai-reference-check.json" --ledger "$COA_EVAL_DIR/ledger.json" --max-judgements 8 --max-seconds 1200
```

Deze optionele controle vergelijkt acht vaste kandidaatantwoorden met een apart beoordeelde **AI-referentie**. Ontwikkel- en controlegroepen blijven gescheiden; referentielabels gaan niet naar de beoordelaar. `human_calibration: false` blijft zichtbaar. De menselijke labels blijven leeg; er hoeft nu niemand labels aan te leveren. Overeenstemming met deze kleine AI-set bewijst geen betrouwbare beoordelaar voor andere onderwerpen.

Gebruik uitsluitend de fictieve data. Lokale embeddings betekenen niet dat alle inhoud lokaal blijft: geselecteerde passages, of alle dertig secties bij de directe-contextaanpak, gaan voor generatie via Codex naar OpenAI. De afzonderlijke beoordelaar ontvangt ook criteria en referentiepassages. Aanmelding blijft buiten de gedeelde artifacts; fouten worden begrensd en geschoond vastgelegd. Lees de [privacygrenzen](../privacy-boundaries.md) vóór uitbreiding of publicatie.

Serveer alleen `/tmp/coa-rag-preview` als je een lokale websitepreview wilt. Houd JSONL, diagnostiek, ledger en aanmeldgegevens buiten die map. Publiek delen heeft een afzonderlijke vrijgave; deze handleiding publiceert niets.
