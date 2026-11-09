# Architectuurgids: de webdemo en de evaluatieresultaten

Deze gids beschrijft de actuele weblaag en de grenzen tussen live uitvoering en opgeslagen evaluaties. De beschrijving is op 9 november 2026 aan de broncode gecontroleerd. De bestaande [README](../README.md), [Codex-uitleg](retrieval-codex.md), [evaluatiehandleiding](evaluation/reproduce.md) en [privacygrenzen](privacy-boundaries.md) geven verdere context. De oudere [interactieve code-tour](code-tour.html) is een historische vijfbronnenmomentopname; gebruik de getallen daarin niet als huidige corpusomvang.

## Recente architectuurwijzigingen

| Datum | Verandering | Implementatie |
| --- | --- | --- |
| 2026-11-09 | Eén webserver verbindt startpagina, bestaande live demo, opgeslagen modelvergelijking en technische uitleg. De live modeluitvoering blijft dezelfde. | [`web::router`](../src/web.rs), [`static/site/`](../static/site/) |
| 2026-11-09 | Een aparte, controleerbare export maakt van de zestig vastgelegde sessies een openbare presentatieprojectie. Ruwe uitvoer en lokale bestandspaden worden niet geserveerd. | [`export_site_results.py`](../scripts/export_site_results.py) |

## Eén origin, drie onderdelen

De executable start met `serve` of zonder argumenten. [`web::serve`](../src/web.rs) laadt de bronnen/index, maakt `WebState` en bindt op localhost. De standaardpoort blijft 8766; `COA_PORT` kan een andere lokale poort kiezen.

| Route | Wat de bezoeker krijgt |
| --- | --- |
| `/` | De startpagina met toegang tot de drie onderdelen. |
| `/demo/` | De bestaande vrije vraag, brondocumenten, antwoord en echte zoekacties. |
| `/resultaten/` | Zestig opgeslagen sessies: tien vragen, twee modellen, drie herhalingen. |
| `/techniek/` | Een leesbare samenvatting van de bestaande bron-, zoek-, Codex- en evaluatielogica. |
| `/assets/site.css`, `/assets/results.js`, `/assets/results.json` | Expliciet ingebouwde presentatiebestanden. Andere bestanden worden niet via deze routes aangeboden. |

Deze pagina's zijn `include_str!`-assets in de Rust-binary. Er is geen algemene fileserver die de checkout of experimentele outputmap ontsluit. Een gewijzigde asset vraagt opnieuw bouwen en starten. De webserver houdt zijn bestaande localhostbinding; deze wijziging voegt geen publieke hosting toe.

## De vrije demo blijft live

[`static/index.html`](../static/index.html) gebruikt absolute `/api/...`-routes. De verplaatsing naar `/demo/` verandert die requests daarom niet. `POST /api/session` maakt een begrensde sessie in [`RunStore`](../src/runs.rs). `POST /api/runs` legt een contextsnapshot vast, start een achtergrondtaak en geeft een pending run terug. De browser pollt daarna `GET /api/runs/{id}`.

[`CodexRunner::run_for_corpus`](../src/codex.rs) start de bestaande native Codex-route met read-only MCP-zoeken. De MCP-child gebruikt dezelfde Rust-zoekimplementatie, maar een eigen indexinstantie. MiniLM draait lokaal; Codex verwerkt de vraag en gevonden tekst via de modeldienst. [`Index::load_for_corpus`](../src/lib.rs) en [`CorpusId`](../src/corpus.rs) leggen de bronset vast. De normale webdemo gebruikt tien fictieve documenten met zestig secties; oudere CLI-regressies houden hun afzonderlijke vijfbronnenscope.

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
