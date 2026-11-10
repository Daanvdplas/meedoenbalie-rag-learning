# Architectuurgids: de webdemo en de evaluatieresultaten

Deze gids beschrijft de actuele weblaag en de grenzen tussen live uitvoering en opgeslagen evaluaties. De beschrijving is op 10 november 2026 aan de broncode gecontroleerd. De bestaande [README](../README.md), [Codex-uitleg](retrieval-codex.md), [evaluatiehandleiding](evaluation/reproduce.md) en [privacygrenzen](privacy-boundaries.md) geven verdere context. De oudere [interactieve code-tour](code-tour.html) is een historische vijfbronnenmomentopname; gebruik de getallen daarin niet als huidige corpusomvang.

## Recente architectuurwijzigingen

| Datum | Verandering | Implementatie |
| --- | --- | --- |
| 2026-11-10 | De demo kan per nieuwe vraag kiezen tussen dense-zoeken op betekenis en BM25 op woorden. De methode wordt in de MCP-launch vastgezet; de zichtbare gebruikte methode komt uit gevalideerde toolresultaten. | [`web::run`](../src/web.rs), [`CodexRunner::run_for_corpus_with_method`](../src/codex.rs), [`Index::search_for_method`](../src/lib.rs) |
| 2026-11-10 | De demo is direct de homepage op `/`; `/demo` en `/demo/` sturen met HTTP 308 door naar `/`. De aparte home-template en Home-link zijn verwijderd; Demo verwijst naar `/`. | [`web::router`](../src/web.rs), [`static/index.html`](../static/index.html), [`static/site/`](../static/site/) |
| 2026-11-10 | Zoekacties staan als afzonderlijke blokken met letterlijke query en leesbare bronpassages. Technische labels en de aparte onbekendenlijst zijn uit de UI verwijderd. De webprompt houdt bronmetadata in `citations` en ontbrekende informatie in `unknowns`, met behoud van rechtstreeks gevraagde feiten en noodzakelijke onzekerheid. | [`demo.js`](../static/site/demo.js), [`static/index.html`](../static/index.html), [`generation_prompt_for_answer_language`](../src/codex.rs) |
| 2026-11-10 | Een Nederlandse/Engelse taalkeuze vertaalt de demo-interface en kiest de taal van nieuwe antwoorden. De taal staat in de runcontext; originele broninhoud en citaten blijven ongewijzigd. Historische CLI- en experimentprompts blijven gelijk. | [`language.js`](../static/site/language.js), [`demo.js`](../static/site/demo.js), [`web::run`](../src/web.rs), [`CodexRunner::run_for_corpus_in_language`](../src/codex.rs) |
| 2026-11-10 | De live demo toont alleen activiteiten, vrije vraag, antwoord en zoekresultaten. De voorbeeldvragen, extra vraag-/herhaalknoppen, uitvoeringsmetadata en geschiedenis zijn uit de browser verwijderd; de bestaande API blijft beschikbaar. De navigatie toont geen resultatenlink en de demo heeft één projectdisclaimer. | [`static/index.html`](../static/index.html), [`static/site/`](../static/site/) |
| 2026-11-09 | Eén webserver verbindt startpagina, bestaande live demo, opgeslagen modelvergelijking en technische uitleg. De live modeluitvoering blijft dezelfde. | [`web::router`](../src/web.rs), [`static/site/`](../static/site/) |
| 2026-11-09 | Een aparte, controleerbare export maakt van de zestig vastgelegde sessies een openbare presentatieprojectie. Ruwe uitvoer en lokale bestandspaden worden niet geserveerd. | [`export_site_results.py`](../scripts/export_site_results.py) |

## Eén origin, drie onderdelen

De executable start met `serve` of zonder argumenten. De CLI laadt eerst de bronnen/index. [`web::serve`](../src/web.rs) ontvangt die index, maakt `WebState` en bindt op localhost. De standaardpoort blijft 8766; `COA_PORT` kan een andere lokale poort kiezen.

| Route | Wat de bezoeker krijgt |
| --- | --- |
| `/` | De demo zelf: vrije vraag, brondocumenten, antwoord en echte zoekacties. |
| `/demo`, `/demo/` | Een permanente HTTP 308-redirect naar `/` voor bestaande links. |
| `/resultaten/` | Zestig opgeslagen sessies: tien vragen, twee modellen, drie herhalingen. |
| `/techniek/` | Een leesbare samenvatting van de bestaande bron-, zoek-, Codex- en evaluatielogica. |
| `/assets/site.css`, `/assets/language.js`, `/assets/demo.js`, `/assets/results.js`, `/assets/results.json` | Expliciet ingebouwde presentatiebestanden. Andere bestanden worden niet via deze routes aangeboden. |

Deze pagina's zijn `include_str!`-assets in de Rust-binary. Er is geen algemene fileserver die de checkout of experimentele outputmap ontsluit. Een gewijzigde asset vraagt opnieuw bouwen en starten. De webserver houdt zijn bestaande localhostbinding; deze wijziging voegt geen publieke hosting toe.

## De vrije demo blijft live

De browser toont de brondocumenten, één vrij vraagveld, het antwoord en de openklapbare echte zoekacties met bron, datum en letterlijke citaten. Een wijziging in het vraagveld verbergt het vorige antwoord. Voorbeeldvragen, een losse knop voor een nieuwe vraag, herhalen, uitvoeringsmetadata en geschiedenis staan niet meer in de interface. De bestaande API-routes voor sessies, historie en herhalen blijven behouden. De browser bewaart het sessie-ID in `sessionStorage` en hervat een nog lopende uitvoering bij herladen. De merknaam is gewone tekst; de gedeelde navigatie bevat Demo op `/` en technische uitleg; `/resultaten/` blijft rechtstreeks bereikbaar. Eén footer in de demo bevat de projectgrenzen en de bestaande code-link.

[`static/index.html`](../static/index.html) gebruikt absolute `/api/...`-routes. De demo op `/` en de oude padredirects veranderen die requests daarom niet. `POST /api/session` maakt een begrensde sessie in [`RunStore`](../src/runs.rs). `POST /api/runs` legt een contextsnapshot vast, start een achtergrondtaak en geeft een pending run terug. De browser pollt daarna `GET /api/runs/{id}`.

[`CodexRunner::run_for_corpus_with_method`](../src/codex.rs) gebruikt voor Nederlands en Engels een afzonderlijke webprompt. Deze prompt houdt `answer.text` bij het directe antwoord: bronverwijzingen en ongevraagde bronpublicatiegegevens blijven in de gestructureerde `citations`, ontbrekende informatie in `unknowns`. Een datum of ID mag wel in het antwoord staan als de gebruiker juist dat feit vraagt. Noodzakelijke onzekerheid en een rechtstreeks onbekend antwoord blijven in de antwoordtekst. De Engelse variant kiest Engels voor antwoord en `unknowns` en verbiedt vertaling van letterlijke citaatquotes. De bestaande CLI-, lab- en experimentingangen gebruiken hun oorspronkelijke prompts. De runner start de bestaande native Codex-route met read-only MCP-zoeken. De MCP-child gebruikt dezelfde Rust-zoekimplementatie, maar een eigen indexinstantie. MiniLM draait lokaal; Codex verwerkt de vraag en gevonden tekst via de modeldienst. [`Index::load_for_corpus`](../src/lib.rs) en [`CorpusId`](../src/corpus.rs) leggen de bronset vast. De normale webdemo gebruikt tien fictieve documenten met zestig secties; oudere CLI-regressies houden hun afzonderlijke vijfbronnenscope.

De taalwissel staat alleen in de demo op `/`, met één knop die de huidige vlag toont. Die opent een native `details`-keuze met **🇳🇱 Nederlands** en **🇬🇧 English**. De knop gebruikt `aria-expanded`/`aria-controls` en een toegankelijke taalnaam; de keuzeknoppen behouden `aria-pressed` en toetsenbordbediening. Kiezen of Escape sluit de keuze en herstelt focus naar de vlagknop; buiten klikken sluit zonder focus van de aangeklikte control af te nemen. Openen start geen request of taalwissel. [`language.js`](../static/site/language.js) bewaart de voorkeur onder `localStorage['coa-demo-language']`; ontbrekende of onbekende voorkeuren worden Nederlands. Als browseropslag is geblokkeerd, werkt de keuze voor het huidige bezoek zonder persistentie. De gedeelde navigatie en footer volgen de voorkeur ook op de andere routes; de oorspronkelijke pagina-inhoud heeft daar `lang="nl"`. In de demo zijn activiteitnamen en sectiekoppen schermlabels. Bronparagrafen en citaatpassages blijven exacte Nederlandse brontekst met `lang="nl"` en een herkenbaar bronlabel.

`POST /api/runs` accepteert optioneel `answer_language: "nl" | "en"`. Een ontbrekend veld betekent Nederlands; ongeldige waarden worden door de bestaande JSON-validatie afgewezen met HTTP 422, zonder run. De `activity-demo-v7`-context bewaart de antwoordtaal en zoekmethode; beide webtalen krijgen promptversie `web-activity-answer-only-v1`. De replay-API hergebruikt de taal uit de oorspronkelijke context en controleert daarmee dezelfde invoer. De UI-taal kan tijdens een lopende uitvoering veranderen, maar de antwoordtaal van die uitvoering staat al vast. [`demo.js`](../static/site/demo.js) bewaart status als vertaalsleutels en hertekent schermlabels in de huidige taal. Wisselen verstuurt geen request, bewaart vraagtekst en geopende bronnen, en verandert geen bestaand antwoord, `unknowns` of citaten.

`POST /api/runs` accepteert daarnaast optioneel `retrieval_method: "dense" | "bm25"`, standaard `dense`; onbekende waarden worden met HTTP 422 geweigerd. De context bewaart de gevraagde methode vóór de uitvoering. `run_for_corpus_with_method` zet de MCP-child vast met `mcp --corpus ... --method dense|bm25`; de gebruiker of het model kan de methode niet tussen zoekacties wijzigen. De bestaande grenzen blijven gelden: top-k 1 tot 6, maximaal vier zoekacties en 120 seconden. Dit gebruikt de gewone demo-route, zonder labfilters of de contextlimiet van het lab. Replay behoudt de oorspronkelijke methode en antwoordtaal.

[`Index::search_for_method`](../src/lib.rs) kiest bij `dense` de bestaande MiniLM-/cosinusroute en bij `bm25` de bestaande BM25-berekening op documenttitel, sectiekop en volledige sectietekst (k1=1.2, b=0.75, Unicodewoorden, geen stemming). Een BM25-query embedt de vraag niet en gebruikt geen vectorrangschikking. De MCP-index initialiseert wel nog het lokale model en hetzelfde corpus-/vectormanifest om de vastgelegde bronidentiteit te behouden. BM25-hits hebben `bm25_score`; dense-hits behouden `similarity`. Geen van beide scores staat in de demo-interface.

Iedere succesvolle MCP-call bevat de werkelijk gebruikte `retrieval_method`. De runner weigert resultaten met een andere of gemengde methode. De weblaag vult `run.result.retrieval_method` pas na die controle uit de werkelijke toolresultaten. De rustige methoderegel bij het antwoord gebruikt uitsluitend dat veld; een lopende of oudere uitvoering zonder dit veld krijgt geen verzonnen methodenaam. De selector boven de zoekknop bepaalt alleen nieuwe vragen: wijzigen start geen call en verandert geen bestaand antwoord of de vastgelegde methode van een lopende uitvoering.

Elke echte zoekactie krijgt een eigen blok met een genummerde kop, de exacte query en de aangeboden passages, in de oorspronkelijke volgorde. Gelijke passages in meerdere acties worden niet samengevoegd. Hittitels tonen activiteit en menselijk sectiekopje; één rustige datum komt rechtstreeks uit `hit.section.source_date`. IDs, top-k, referentiedatum, cosinusovereenkomst en voltooide statussen worden niet weergegeven. Mislukte of onafgeronde zoekacties houden een zichtbare fout-/wachtmelding. Letterlijke citaatmarkeringen blijven gekoppeld aan dezelfde bron-ID, brondatum en sectie-ID. De aparte `unknowns`-weergave is verwijderd; de backenddata blijft intact. Er is geen tekstfilter of regex die antwoordinhoud, haakjes of datums verwijdert.

Een geaccepteerde run moet aan de bron-/citaatvalidatie voldoen en bij de contextsnapshot passen. Letterlijke citaatcontrole bewijst herkomst, geen volledige antwoordcorrectheid. De website heeft geen dossiermutatie, berichten- of aanmeldfunctie. De bestaande interne beoordelings-API's worden niet als demo-flow aangeboden.

## Opgeslagen resultaten zijn geen nieuwe modeluitvoering

De resultatenpagina selecteert alleen bewaarde gegevens. Een ander model of een andere herhaling kiezen start geen model-, zoek- of beoordelaarscall en verandert het live-demomodel niet.

De weergegeven proef gebruikt de dataset `strengthened-10-20261106`, twee modellen en drie herhalingen. Beide modellen gebruikten de methode `dense` op dezelfde zestig secties. Deze pagina vergelijkt dus geen vier zoekmethoden onder beide modellen. De zes eerdere afgebroken pogingen blijven buiten de zestig vergelijkingssessies en worden als afzonderlijke geschiedenis vermeld. De geschreven antwoorden hebben geen inhoudelijke kwaliteitsscore.

[`export_site_results.py`](../scripts/export_site_results.py) controleert de verzegelde bronartefacten, de recordhashes en de native beoordelingen. Vervolgens schrijft het uitsluitend:

- tien vraagteksten en de volledige geldige combinaties van verwachte bronstukjes;
- de tien fictieve documenten, hun datums en zestig exacte secties;
- model/herhaling, bronvolledigheid en werkelijke zoekactietelling;
- zichtbare zoekopdrachten, daadwerkelijk aangeboden bronidentiteiten en het eerste moment waarop de verwachte bronset compleet was.

De export neemt geen ruwe reasoning, prompts, runtimepaden, aanmeldgegevens of willekeurige recordvelden over. Een whitelist bepaalt de projectie. Geheime sentinelvelden en gewijzigde broninhoud worden in de exporttests gecontroleerd. Complete broncombinaties worden per geldig alternatief beoordeeld; tijd van activiteit A en locatie van activiteit B vormen niet samen een geldige optie.

De browser haalt de kleine, gecureerde `results.json` op. Alle bronquotes verschijnen als tekstnodes, niet als HTML uit de data. Alleen werkelijk opgehaalde verwachte passages krijgen groene markering; andere passages worden niet automatisch als onjuist gelabeld. De voorafgaande bronverwachtingen blijven neutraal. Bij een geopende vraag staan ze boven de resultaten. Het aantal zoekacties omvat alle werkelijk uitgevoerde acties; een eerder compleet moment verschijnt alleen aanvullend in de details.

## Opnieuw exporteren en controleren

Vanaf een eigen bestaande experimentmap:

```sh
python3 scripts/export_site_results.py --experiment-dir /pad/naar/de/bevroren/proef
python3 -m unittest discover -s scripts -p 'test_export_site_results.py'
cargo test --locked learning_site_routes_share_navigation
```

Dit exporteren gebruikt geen model. Een nieuwe live vraag in de demo doet dat wel. De bronverwachtingen uit de publieke resultaatasset worden niet aan de Codex-prompt, MCP-index of stopbeslissing toegevoegd.
