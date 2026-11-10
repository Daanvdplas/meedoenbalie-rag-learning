# Architectuurgids: de webdemo en de evaluatieresultaten

Deze gids beschrijft de actuele weblaag en de grenzen tussen live uitvoering en opgeslagen evaluaties. De beschrijving is op 10 november 2026 aan de broncode gecontroleerd. De bestaande [README](../README.md), [Codex-uitleg](retrieval-codex.md), [evaluatiehandleiding](evaluation/reproduce.md) en [privacygrenzen](privacy-boundaries.md) geven verdere context. De oudere [interactieve code-tour](code-tour.html) is een historische vijfbronnenmomentopname; gebruik de getallen daarin niet als huidige corpusomvang.

## Recente architectuurwijzigingen

| Datum | Verandering | Implementatie |
| --- | --- | --- |
| 2026-11-10 | De webdemo gebruikt directe OpenAI Responses API-calls met Sol, tien gelijktijdige bezoekersvragen, veilige begrensde sessies en één gedeelde lokale zoekindex. Codex-CLI, MCP en bevroren evaluaties blijven apart behouden. | [`responses.rs`](../src/responses.rs), [`web.rs`](../src/web.rs), [`runs.rs`](../src/runs.rs) |
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

[`ResponsesRunner::run`](../src/responses.rs) verstuurt de vraag via HTTPS naar `https://api.openai.com/v1/responses` met `model: gpt-6.1-sol`, reasoning `low`, service tier `default` en `store: false`. De bestaande taal-, bron- en promptinjectieregels worden gedeeld met de webprompt in [`codex.rs`](../src/codex.rs); de gebruikersvraag staat als afzonderlijk data-item in `input`. Het antwoord blijft direct en kort, met noodzakelijke onzekerheid in de tekst en bronmetadata/letterlijke citaten in de gestructureerde velden. Het model krijgt uitsluitend de function tool `search_document(query, top_k)`, geen shell, bestanden, webzoekfunctie, Codex-proces of code-mode-host. Dit is lokale function calling; de webroute heeft geen MCP-transport. De vaste zoekmethode is servercontext, geen door het model wijzigbaar argument.

De applicatie stuurt de oorspronkelijke input en ontvangen outputitems bij iedere vervolgaanroep opnieuw mee, inclusief encrypted reasoning en assistant phases, gevolgd door de juiste `function_call_output`. Dat gebeurt alleen binnen het RAM van deze uitvoering, zonder `previous_response_id`; raw/encrypted reasoning wordt niet in publieke runrecords of bestanden opgenomen. Een response moet `completed` zijn. Refusals, onbekende tools, ongeldige argumenten en ongeldige eind-JSON worden geweigerd. Commentary-items blijven voor de vervolgcontext behouden; alleen het uiteindelijke antwoord wordt inhoudelijk verwerkt. De bestaande exacte bron-/citaatcontrole uit `validate_retrieved_answer` blijft bindend, inclusief gelijke brondata en manifesten. Dit bewijst herkomst, geen volledige antwoordjuistheid.

De eerste API-call verplicht een echte bronzoekactie; daarna kan het model gericht verderzoeken. Maximaal vier zoekacties en vijf providerrequests zijn toegestaan, zonder automatische retries. Na de vierde zoekactie staat tool choice op `none`. De hele loop, inclusief HTTP-body lezen en zoeken, heeft één deadline van 120 seconden. Per response gelden 4096 outputtokens (inclusief reasoning) en 512 KiB HTTP-body; de geserialiseerde requestcontext is maximaal 1 MiB. Zoekvragen zijn 1–1000 tekens, top-k is 1–6; toolargumenten zijn maximaal 16 KiB en weigeren onbekende velden. HTTP-redirects staan uit. Publieke fouten bevatten vaste meldingen, geen providerbody, sleutel of pad.

[`web::serve`](../src/web.rs) bewaart één `Index` achter een mutex, via een vaste zoekcallback. Alleen de korte lokale zoekactie houdt die mutex vast in `spawn_blocking`; de tien model-HTTP-requests wachten onafhankelijk op de provider. Er worden geen tien kopieën van MiniLM of Codex-processen gestart. [`Index::load_for_corpus`](../src/lib.rs) en [`CorpusId`](../src/corpus.rs) leggen de tien documenten en zestig secties vast. CLI-, lab- en experimentroutes houden hun bestaande Codex/MCP-uitvoering en afzonderlijke corpusscope.

Een centrale semaphore in `WebState` laat tien actieve vragen toe. Nummer elf krijgt HTTP 429; er is geen wachtrij. Eén sessie mag één pending run hebben (HTTP 409 bij overlap). Het permit leeft tot afronding of fout/time-out en wordt daarna vrijgegeven. `RunStore` gebruikt 256-bit willekeurige sessietokens, maximaal 256 sessies, 40 runs per sessie en 256 runs totaal. Een run is maximaal 256 KiB geserialiseerd, inclusief reserve voor beoordelingen. Bij capaciteit schuiven uitsluitend inactieve sessies/afgeronde runs op; alle invoercontrole vindt vóór deze eviction plaats. Sessies vervallen 30 minuten na aanmaken of de laatste gestarte vraag; cleanup gebeurt bij API-toegang en beschermt pending runs. Polling verlengt deze termijn niet. Alle dynamische API-responses, ook fouten en rapporten, krijgen `Cache-Control: no-store`. Het RAM verdwijnt bij herstart, geen garantie over swap, downloads of providerkopieën.

`ResponsesRunner::from_env` leest `OPENAI_API_KEY` of expliciet `COA_OPENAI_API_KEY_FILE`. Een sleutelbestand moet regulier zijn, van de huidige gebruiker, zonder symlink/hardlink of groep-/anderrechten en maximaal 4096 bytes na trimmen. Alleen de server heeft de sleutel; configuratie en gezondheid publiceren geen sleutel/pad. Ontbrekende configuratie laat website en bronnen starten met een duidelijke niet-beschikbaarmelding, zonder Codex-fallback. `runtime_ready` betekent geconfigureerd, niet dat providerquota of tegoed zijn bewezen. `store: false` is geen ZDR-belofte. De debug-only loopbackmock gebruikt altijd een vaste fake key en leest de echte sleutel niet; een release weigert die override.

De creditclaimdialoog, gecontroleerd op 10 november 2026, waarschuwt dat activering van het Pro-API-voordeel Codex via API-keys van de gekozen organisatie uitschakelt, ook bij gekocht tegoed. Gewone Responses API-calls en Codex via het abonnement blijven volgens die dialoog werken. Dit project gebruikt de gewone API; activatie en saldo zijn niet door deze implementatie gecontroleerd.

Nieuwe runcontext gebruikt `model_configuration`, met `provider: openai` en `transport: responses_api`. Nieuwe resultaten behouden die herkomst, het aantal providerrequests en op succes de som van de beschikbare usage over alle calls. Reasoningtokens zijn onderdeel van outputtokens. Als een usage ontbreekt, blijft het totaal onbekend. Fouten kunnen na een betaalde call ontstaan en hebben geen volledig kostenoverzicht; dit is geen kostenledger. Een clientdeadline garandeert evenmin annulering of kostenstop bij de provider. De bevroren Codex-resultaten blijven ongewijzigd met hun eigen runtime; hun gemeten retrievalkwaliteit is geen nieuwe API-kwaliteitsclaim.

De taalwissel staat alleen in de demo op `/`, met alleen zichtbare vlaggen, toegankelijke taalnamen via `aria-label`/`title`, `aria-pressed` en toetsenbordbediening. [`language.js`](../static/site/language.js) bewaart de voorkeur onder `localStorage['coa-demo-language']`; ontbrekende of onbekende voorkeuren worden Nederlands. Als browseropslag is geblokkeerd, werkt de keuze voor het huidige bezoek zonder persistentie. De gedeelde navigatie en footer volgen de voorkeur ook op de andere routes; de oorspronkelijke pagina-inhoud heeft daar `lang="nl"`. In de demo zijn activiteitnamen en sectiekoppen schermlabels. Bronparagrafen en citaatpassages blijven exacte Nederlandse brontekst met `lang="nl"` en een herkenbaar bronlabel.

`POST /api/runs` accepteert optioneel `answer_language: "nl" | "en"`. Een ontbrekend veld betekent Nederlands; ongeldige waarden worden door de bestaande JSON-validatie afgewezen met HTTP 422, zonder run. De `activity-demo-responses-v1`-context bewaart de antwoordtaal en zoekmethode; beide webtalen krijgen promptversie `responses-activity-v1`. De replay-API hergebruikt de taal uit de oorspronkelijke context en controleert daarmee dezelfde invoer. De UI-taal kan tijdens een lopende uitvoering veranderen, maar de antwoordtaal van die uitvoering staat al vast. [`demo.js`](../static/site/demo.js) bewaart status als vertaalsleutels en hertekent schermlabels in de huidige taal. Wisselen verstuurt geen request, bewaart vraagtekst en geopende bronnen, en verandert geen bestaand antwoord, `unknowns` of citaten.

`POST /api/runs` accepteert daarnaast optioneel `retrieval_method: "dense" | "bm25"`, standaard `dense`; onbekende waarden worden met HTTP 422 geweigerd. De context bewaart de gevraagde methode vóór de uitvoering. `ResponsesRunner::run` legt de methode vast in de lokale zoekcallback; de gebruiker of het model kan de methode niet tussen zoekacties wijzigen. De bestaande grenzen blijven gelden: top-k 1 tot 6, maximaal vier zoekacties en 120 seconden. Dit gebruikt de gewone demo-route, zonder labfilters of de contextlimiet van het lab. Replay behoudt de oorspronkelijke methode en antwoordtaal.

[`Index::search_for_method`](../src/lib.rs) kiest bij `dense` de bestaande MiniLM-/cosinusroute en bij `bm25` de bestaande BM25-berekening op documenttitel, sectiekop en volledige sectietekst (k1=1.2, b=0.75, Unicodewoorden, geen stemming). Een BM25-query embedt de vraag niet en gebruikt geen vectorrangschikking. De gedeelde demo-index initialiseert bij starten nog het lokale model en hetzelfde corpus-/vectormanifest om de vastgelegde bronidentiteit te behouden. BM25-hits hebben `bm25_score`; dense-hits behouden `similarity`. Geen van beide scores staat in de demo-interface.

Iedere succesvolle zoekactie bevat de werkelijk gebruikte `retrieval_method`. De runner weigert resultaten met een andere of gemengde methode. De weblaag vult `run.result.retrieval_method` pas na die controle uit de werkelijke toolresultaten. De rustige methoderegel bij het antwoord gebruikt uitsluitend dat veld; een lopende of oudere uitvoering zonder dit veld krijgt geen verzonnen methodenaam. De selector boven de zoekknop bepaalt alleen nieuwe vragen: wijzigen start geen call en verandert geen bestaand antwoord of de vastgelegde methode van een lopende uitvoering.

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

Dit exporteren gebruikt geen model. Een nieuwe live vraag in de demo doet dat wel. De bronverwachtingen uit de publieke resultaatasset worden niet aan de nieuwe API-prompt, zoekindex of stopbeslissing toegevoegd. De historische Codex-evaluaties blijven eveneens gescheiden van hun verwachtingen tijdens generatie.

De nieuwe deterministische checks in `responses::tests`, `runs::tests` en `web::tests` gebruiken alleen lokale HTTP-mocks en fictieve fixtures. De tien-vragen-test wacht op tien daadwerkelijk overlappende providerrequests, controleert afzonderlijke antwoorden/traces en weigering van de elfde vraag. Twee toolrondes met encrypted reasoning en commentary/final phases, bodygrenzen, time-outs, foutredactie, sessieverval en afgewezen requests worden afzonderlijk gecontroleerd. Een echte tien-Sol-providerproef is nog niet uitgevoerd; quota, live latency en actuele API-billing zijn niet gemeten.

Providercontract: [function calling](https://developers.openai.com/api/docs/guides/function-calling), [reasoning/stateless replay](https://developers.openai.com/api/docs/guides/reasoning) en [API-data controls](https://developers.openai.com/api/docs/guides/your-data), officiële pagina's opgehaald 10 november 2026; function-calling- en reasoningpagina hebben HTTP Last-Modified 10 november 2026. Dit is transportdocumentatie, geen bevestiging van accounttegoed of inhoudelijke antwoordkwaliteit.
