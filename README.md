# Meedoenbalie: activiteiten verkennen

Een kleine Rust-leerproef met tien verzonnen activiteiten, echte lokale embeddings en Codex. Je stelt een vraag en kunt de gebruikte bronpassages controleren. Dit is een eigen prototype, geen intern COA-plan.

Bekijk de [interactieve codegids](docs/code-tour.html) voor het systeemoverzicht, de route van één vraag en verwijzingen naar de echte functies. De HTML werkt ook lokaal zonder CDN.

Wil je de RAG zelf evalueren? Volg de [korte reproduceerhandleiding](docs/evaluation/reproduce.md): technische tests en een voorbeeldrapport zonder modelcalls, daarna optioneel begrensde generaties, afzonderlijke beoordelingen en een lokaal HTML-rapport. Bewijsdekking, voorlopige AI-oordelen en efficiëntie blijven apart; menselijke ijking is nog niet uitgevoerd.

De [vastgelegde echte evaluatie van 23–26 oktober 2026](docs/evaluation/results/2026-10-26/README.md) bevat de bewaarde pogingen, voorlopige uitkomsten, fouten en reproduceercommando’s. Die kleine synthetische vergelijking is geen menselijke kwaliteitsvalidatie.

## Starten

Benodigd: Rust 1.94.1 en een aangemelde Codex-client. De demo zoekt lokaal naar de bij de Codex-app horende runtime. Een losse CLI moet een bijpassende `codex-code-mode-host` hebben. Zo nodig kun je het juiste uitvoerbare bestand instellen met `COA_CODEX_BIN`; bestaande aanmeldgegevens blijven buiten de runverslagen.

```sh
cargo run --locked
```

Open **http://127.0.0.1:8766/**. De startpagina verbindt de [live demo](http://127.0.0.1:8766/demo/) en [technische uitleg](http://127.0.0.1:8766/techniek/). De opgeslagen [tests en resultaten](http://127.0.0.1:8766/resultaten/) blijven rechtstreeks bereikbaar. De [architectuurgids](docs/CODEBASE_GUIDE.md) beschrijft de routes en de scheiding tussen live vragen en opgeslagen resultaten. De eerste start downloadt ongeveer 235 MB aan modelbestanden en aanvullende bestanden naar `.model-cache/`. Voor embeddings is geen API-sleutel nodig; Codex gebruikt je bestaande aanmelding. De server start nadat het lokale model is geladen.

Een andere lokale poort kan met `COA_PORT=8767 cargo run --locked`. `COA_CODEX_MODEL` kan het expliciet gekozen antwoordmodel veranderen; de gebruikte versie en instellingen worden per run vastgelegd.

## De website en opgeslagen vergelijking

De resultatenpagina toont de zestig vastgelegde sessies van 6 november 2026: tien vragen, Sol en Luna en drie herhalingen. De modelkeuze daar selecteert bewaarde runs; zij verandert het live-demomodel niet en start geen modelcalls. Beide modellen zijn in deze proef met zoeken op betekenis getest. Bronvolledigheid is geen inhoudelijke antwoordkwaliteitsscore.

De gepubliceerde presentatie is een beperkte export van de werkelijke brontraces. Ruwe logs, reasoning, aanmeldgegevens en lokale paden worden niet door de webserver aangeboden. Opnieuw exporteren uit een bestaande, bevroren proefmap kan zonder modelcalls:

```sh
python3 scripts/export_site_results.py --experiment-dir /pad/naar/de/proef
python3 -m unittest discover -s scripts -p 'test_export_site_results.py'
```

## Zelf testen

1. Bovenaan staan tien compacte activiteiten, waaronder **Tuinieren**, **Fietsen opknappen**, **Creatief atelier** en **Parkonderhoud**. Klik een titel om het volledige fictieve document met bron en datum te lezen. Er staat steeds maximaal één activiteit open.
2. Schrijf één eigen vraag. Alle documenten beschrijven vrijwillige activiteiten, geen betaalde banen.
3. Codex maakt één antwoord. Onder **Bekijk de resultaten** staan de echte zoekacties van Codex. Codex kan meerdere keren zoeken om ontbrekende informatie voor dat ene antwoord te verzamelen; dit zijn geen extra gebruikersvragen.
4. Exact geciteerde tekst is gemarkeerd in de werkelijk gevonden passages. Bron en datum staan erbij; de volledige activiteiten blijven bovenaan leesbaar. Er is geen afzonderlijk bewijsblok of beoordelingsflow.
5. Pas de vraag aan en zoek opnieuw. Het vorige antwoord verdwijnt zodra je typt; iedere ingediende vraag krijgt een eigen uitvoering. Voorbeeldvragen, losse knoppen voor nieuwe vragen of herhalen, technische uitvoeringsgegevens en geschiedenis worden niet getoond.

Een Codex-antwoord kan ongeveer een minuut duren; de bron blijft ondertussen leesbaar. Er kan één modeluitvoering tegelijk lopen. Fouten blijven zichtbaar. Uitvoeringen staan in begrensd servergeheugen en verdwijnen bij herstart; de demo toont geen geschiedenis. Er is geen beoordelings- of criteriaflow; cosinusovereenkomsten staan alleen bij de technische zoekresultaten.

## Rust-zoekfunctie en evaluatie

```sh
cargo run --locked -- search 'Waar en wanneer is het tuinieren?'
cargo run --locked -- ask 'Zijn er nog plekken vrij?'
cargo run --locked -- eval > /tmp/meedoen-eval.json
cargo test --locked
```

De interactieve demo gebruikt tien documenten met elk zes secties: 60 bronstukken. De oorspronkelijke vijf documenten zijn byte voor byte behouden. De nieuwe corpusversie heeft referentiedatum 5 november 2026; iedere bron behoudt ook zijn eigen publicatiedatum.

De oudere CLI-regressie (`eval` en `search`/`ask` zonder corpusoptie), datasets `legacy` en `independent-20261103`, en opgeslagen rapporten blijven de oorspronkelijke vijf documenten gebruiken. Kies de nieuwe versie expliciet bij losse CLI-zoekacties:

```sh
cargo run --locked -- search --corpus expanded-ten 'Wat kan ik op dinsdag binnen doen?'
```

De tien activiteitendocumenten:

| Bestand | Activiteit | Brondatum | Moment |
| --- | --- | --- | --- |
| `fixtures/document.md` | Tuinieren | 2026-10-20 | dinsdag 10–12 |
| `fixtures/bicycle-workshop.md` | Fietsen opknappen | 2026-10-18 | dinsdag 14–16 |
| `fixtures/language-cafe.md` | Taalcafé | 2026-10-19 | woensdag 14–15.30 |
| `fixtures/community-cooking.md` | Samen koken | 2026-10-17 | vrijdag 11–13 |
| `fixtures/walking.md` | Wandelen | 2026-10-20 | donderdag 9.30–10.30 |
| `fixtures/expanded-10-20261105/activity-006.md` | Creatief atelier | 2026-11-05 | dinsdag 13–15 |
| `fixtures/expanded-10-20261105/activity-007.md` | Parkonderhoud | 2026-11-05 | vrijdag 10–12 |
| `fixtures/expanded-10-20261105/activity-008.md` | Buurtbibliotheek | 2026-11-05 | woensdag 10–12 |
| `fixtures/expanded-10-20261105/activity-009.md` | Muziekgroep | 2026-11-05 | zaterdag 10–11.30 |
| `fixtures/expanded-10-20261105/activity-010.md` | Digitale hulp | 2026-11-05 | maandag 15–16 |

De parser accepteert maximaal tien documenten. De secties van de gekozen corpusversie krijgen lokale meertalige MiniLM-embeddings via fastembed 7.1.2: documenttitel, sectiekop en exacte tekst. CLI en MCP delen dezelfde Rust-zoekfunctie. Iedere hit en ieder citaat behouden document-ID, sectie-ID en eigen brondatum. De manifest bevat per-documenthashes, corpus- en embeddinginvoerhashes. Standaard top-k is 3, instelbaar van 1 tot 6 per zoekactie; Codex kan maximaal vier gerichte zoekacties doen. Dit garandeert geen volledige lijst of volledig antwoord. Cosinusovereenkomst is geen zekerheid; een grens voor “geen match” is niet gekalibreerd.

De oudere [zoekevaluatie met vijf activiteiten](reports/five-activity-eval.json) vergelijkt echte embeddings met woordoverlap op dezelfde titel/kop/tekst. De vijf vaste tuinvragen en beschermde verwachtingen blijven uitsluitend intern beschikbaar voor regressie en menselijke evaluatie via `fixtures/questions.json`, `fixtures/expectations.json` en de bestaande server-API. De demo toont deze reeks niet. Gold wordt nooit geïndexeerd of naar Codex/zoekresultaten gestuurd. Gemiddelde bronsectiedekking op de vier gelabelde tuinvragen is in dit snapshot 0,625 voor beide methoden. Dit is geen antwoordnauwkeurigheid; actuele plekken en ontbrekend vervoer blijven onbekend.

De rapporten voor [één document](reports/one-document-eval.json) en [twee documenten](reports/two-document-eval.json), inclusief de [oude titelcontextvergelijking](reports/two-document-context-comparison.json), zijn historische snapshots. Documenttitels en corpus zijn nu gewijzigd; vergelijk die scores niet alsof alleen één zoekinstelling veranderde. De [vijf verkennende zoekvragen](reports/five-activity-searches.json) tonen ook echte missers: bij “waar en wanneer tuinieren” ontbreken tijd en locatie in de eerste zes hits. Codex kan gericht verderzoeken, maar een volledig antwoord is niet gegarandeerd. Er wordt geen rangschikking op vaste vragen afgedwongen.

CI controleert formattering, clippy, deterministische tests en build zonder modelaanroepen. Live checks: `cargo test --locked live_report_retains -- --ignored` en `cargo test --locked live_five_activity -- --ignored --nocapture`. De laatste schrijft een kleine titelcontextvergelijking naar de tijdelijke systeemmap, geen grotere benchmark. Zie [besluiten](decisions.md) voor instellingen en beperkingen.

## Nieuwe testversie met tien activiteiten

`expanded-10-20261105` bevat precies tien vooraf vastgelegde vragen over de zestig bronstukken. De verwachte bronnen zijn afzonderlijk opgesteld en gereviewd voordat de methoden worden vergeleken. Bestaande vijfbronnenresultaten worden niet als meting op tien bronnen gepresenteerd. De bronverwachtingen komen nooit in de zoekindex, modelprompt of stopbeslissing.

Bekijk eerst het plan, zonder zoekacties of modelcalls:

```sh
cargo run --locked -- lab agent-plan --dataset expanded-10-20261105
cargo run --locked -- lab plan --dataset expanded-10-20261105 --methods bm25 --profiles documents_sections
```

Een lokale zoektest zonder antwoordmodel of AI-beoordelaar:

```sh
cargo run --locked -- lab run --dataset expanded-10-20261105 --methods bm25 --profiles documents_sections --output /tmp/expanded-ten-bm25.jsonl
cargo run --locked -- lab assess --dataset expanded-10-20261105 --input /tmp/expanded-ten-bm25.jsonl --types deterministic --output /tmp/expanded-ten-assessed.jsonl
```

Alle vier zoekmethoden en de bestaande document-/SQLite-structuren kunnen dezelfde versie kiezen via `--dataset expanded-10-20261105`. SQLite vereist `--features sqlite`. Nieuwe SQLite-caches krijgen een eigen map op basis van de corpus-hash, zodat oude en nieuwe metingen gescheiden blijven. Een agentvergelijking vereist bovendien expliciet `lab agent-run --live` en een nieuw begrensd ledger; de limiet blijft vier zoekacties en 120 seconden per sessie. Een volledige bronset vinden beoordeelt bronretrieval, niet automatisch de inhoud van het uiteindelijke antwoord.

## Herhaalde modelvergelijking

De aparte ontwikkelset `strengthened-10-20261106` bevat tien brongebaseerde vragen op hetzelfde corpus. Vier vragen zijn behouden en zes zijn nieuw of aangescherpt. De oude datasets en resultaten blijven afzonderlijk beschikbaar; een score op deze nieuwe vragen is geen verbetering ten opzichte van de eerdere 35/40-meting.

`scripts/model_trials.py` voert een vooraf vastgelegde proef uit met `gpt-6.1-sol` en `gpt-6-luna`, de bestaande standaardzoekmethode `dense`, tien vragen en drie herhalingen: zestig pogingen. Modelvolgorde wisselt per vraag/herhaling. Beide gebruiken dezelfde gepinde runtime, bron- en embeddingidentiteit, `low` reasoning, top-3, contextlimiet 1200, maximaal vier zoekpogingen en 120 seconden per sessie. Dezelfde reasoningnaam betekent geen gegarandeerd gelijk rekenwerk.

Het script heeft drie stappen: `plan` schrijft de volledige volgorde en hashes zonder modelcalls; `run --live --phase pilot` voert eerst vier vastgelegde pogingen uit; `run --live --phase remaining` vereist een onafhankelijke, hashgebonden `PILOT_APPROVED.json`. `assess` herberekent daarna zes afzonderlijke model/herhalingcohorten met de bestaande Rust-beoordeling. Gebruik `python3 scripts/model_trials.py --help` en de subcommando's voor alle expliciete paden. De globale limiet is zestig pogingen en 7200 seconden, met twee begrensde modelledgers van elk dertig; er zijn geen automatische herhalingen of alternatieve modellen bij fouten.

Een volledig ontvangen maar aantoonbaar ongeldig eindantwoord wordt bewaard als modelmislukking. Een specifieke ontvangen toolactie buiten het vaste contract kan eveneens een modelafwijking aantonen, ook zonder volledig eindantwoord. Brondekking blijft bij zulke mislukte uitvoeringen onbekend. Alleen begrensde veilige diagnostiek wordt bewaard: stage, exitstatus, terminaleventtypen, vaste reden- en toolcategorieën; geen rawreasoning, toolargumenten of modelteksten. Netwerk-, authenticatie-, runtime-, hash- en onduidelijke parserfouten stoppen de proef. Vrije antwoordkwaliteit wordt niet door een AI-beoordelaar geëvalueerd. Een kleiner model is niet vooraf beter of slechter: de vergelijking meet het model samen met deze harness.

De gerepareerde proef gebruikt een nieuw ledger en verbindt de eerdere zes afgebroken pogingen via `--previous-stop-manifest`. Die zes blijven apart; de nieuwe vergelijking bevat zestig pogingen, met een expliciete totale autorisatie van 66 inclusief de afgebroken voorproef. De eerdere onbekende fout wordt niet achteraf tot modelafwijking hernoemd.

De budget-/volgorde-/annuleringschecks draaien zonder modelcalls:

```sh
python3 -m unittest discover -s scripts -p 'test_model_trials.py'
```

## Grenzen en werkwijze

Lees de [privacygrenzen en open besluiten](docs/privacy-boundaries.md) voordat je gegevens of toegang uitbreidt.

Alle gegevens zijn verzonnen. Suggesties behouden bron en datum; ontbrekende of oude feiten blijven onbekend. Deelname is vrijwillig en een medewerker bespreekt opties met de bewoner. Geen dossierwijzigingen, berichten of aanmeldingen. De demo luistert alleen op localhost.

Alle codewijzigingen gaan via branches en pull requests, met onafhankelijke beoordeling. Niet rechtstreeks naar `main` committen; alleen mergen met toestemming van de gebruiker.

Bron: [fastembed 7.1.2](https://github.com/anush008/fastembed-rs/releases/tag/v7.1.2), gepubliceerd **19 oktober 2026**. Zie verder [projectinstructies](AGENTS.md), [use case](user_journey.md) en [2026-oriëntatie](agent-harness-landscape.md). Het langetermijndoel blijft een kleine, uitlegbare agent harness; de interactieve demo gebruikt tien activiteiten en vrije vragen. De oorspronkelijke vijf activiteiten en interne tuinvragen blijven een afzonderlijke regressieset.

## Reproduceerbare experimenten

`experiment` staat los van de demo en de historische vijf tuinvragen. Validatie gebruikt geen modelaanmelding:

```sh
cargo run --locked -- experiment arms
cargo run --locked -- experiment validate
```

Start modeluitvoeringen uitsluitend expliciet. Dit voorbeeld selecteert twee vragen en twee armen, dus maximaal vier generatie-sessies. Het maakt een nieuw JSONL-bestand; bestaande bestanden worden nooit overschreven. Voer het uit vanuit de map met de bestaande `.model-cache`.

```sh
cargo run --locked -- experiment run --live --cases P01,P02 --arms agentic_dense,full_context --output /tmp/rag-smoke.jsonl --ledger /tmp/rag-evaluation-budget.json --max-cases 2 --max-generations 4 --max-judgements 0 --max-seconds 600
```

Gebruik voor alle vervolgopdrachten hetzelfde ledger-bestand. Dit reserveert vóór iedere poging een sessie en begrenst het gezamenlijke traject op 40 generatie-sessies, 56 beoordelingssessies en vier uur, met één uitvoering tegelijk. Ook mislukte pogingen tellen mee. Iedere sessie duurt maximaal 120 seconden; Ctrl+C stopt de actieve subprocessen en schrijft de resterende gevallen als geannuleerd. Een Codex-sessie kan intern meerdere providerrequests bevatten; die zijn niet apart zichtbaar.

`agentic_dense` gebruikt de bestaande echte MCP-zoekfunctie, `full_context` alle dertig bronsecties zonder tools. `dense_once` zoekt één keer met MiniLM; `bm25` zoekt één keer met BM25 (k1=1.2, b=0.75, zonder stemming). De laatste twee geven uitsluitend hun geselecteerde passages aan hetzelfde antwoordmodel. BM25-scores zijn onbegrensde lexicale scores. Zoekstappen, aangeboden secties, brondata, letterlijke citaten, beschikbare usage, limieten en hashes worden opgeslagen. Onbekende usage blijft `null`; een gelukte uitvoering is nog geen inhoudelijke kwaliteitsbeoordeling. Modelaliassen en providerroute beperken exacte reproduceerbaarheid.

Een verlopen gezamenlijk tijdvenster wordt nooit automatisch vernieuwd. Alleen na expliciete toestemming van de taaklead kan een operator het bestaande ledger verlengen:

```sh
cargo run --locked -- experiment extend-window --ledger /tmp/rag-evaluation-budget.json --seconds 14400 --authorize-continuation
```

Dit werkt uitsluitend op een bestaand, verlopen en exclusief vergrendeld ledger. De oorspronkelijke start, alle verbruikte sessies en de plafonds 40/56 blijven staan. Het ledger bewaart de oude en nieuwe deadline, het tijdstip, de vaste autorisatie/reden en de toenmalige tellers. Een actief venster, ongeldige auditketen of beschadigd ledger wordt geweigerd. Gebruik daarna een bijgewerkte uitvoerbare versie; oudere versies weigeren het uitgebreide schema.

Een opgeslagen antwoord opnieuw beoordelen maakt **geen nieuwe antwoorden of zoekacties**. Generator- en graderbudgetten zijn apart. Gebruik hetzelfde ledger-bestand als bij generatie:

```sh
cargo run --locked -- experiment grade --live --input /tmp/rag-smoke.jsonl --output /tmp/rag-smoke-graded.jsonl --ledger /tmp/rag-evaluation-budget.json --max-judgements 4 --max-seconds 600
cargo run --locked -- experiment calibrate --live --output /tmp/rag-judge-calibration.json --ledger /tmp/rag-evaluation-budget.json --max-judgements 8 --max-seconds 1200
```

De beoordelaar krijgt de vraag, het volledige antwoord, de werkelijk aangeboden passages en de afzonderlijk geladen criteria. Deze sessie heeft geen MCP, shell, bestanden, browser of andere tools. Zij beoordeelt juistheid, volledigheid, relevantie, bronsteun en terecht onbekend blijven afzonderlijk, met korte letterlijke bewijsfragmenten. Per criterium volgt `good`, `bad` of `review_needed`; ongeldige uitvoer en time-outs geven geen inhoudelijke score. De fractie goede criteria heeft een expliciete teller/noemer en houdt twijfelgevallen apart. Harde controles blijven bindend.

`calibrate` beoordeelt acht vooraf gemaakte kandidaatantwoorden: vier voor ontwikkeling en vier voor controle. Het vergelijkt de uitkomst pas daarna lokaal met de afzonderlijk beoordeelde AI-referentie. Referentielabels en notities worden niet naar de beoordelaar gestuurd. Het rapport bewaart herkomst, rubric/runtime/promptversies, afwijkende goedkeuringen, afwijkende afkeuringen, twijfelgevallen en noemers. Na ieder antwoord wordt een controlepunt opgeslagen. Dit is een kleine voorlopige vergelijking met AI, **geen menselijke ijking en geen definitieve inhoudelijke kwaliteitspas**. De menselijke velden blijven leeg; een later afzonderlijk referentieartifact kan via `--reference` worden geselecteerd.
