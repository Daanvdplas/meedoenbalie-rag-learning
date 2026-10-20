# Meedoenbalie: activiteiten verkennen

Een kleine Rust-leerproef met twee verzonnen activiteiten, echte lokale embeddings en Codex. Je beoordeelt zelf het antwoord en de gebruikte bronpassages. Dit is een eigen prototype, geen intern COA-plan.

## Starten

Benodigd: Rust 1.94.1 en een aangemelde Codex-client. De demo zoekt lokaal naar de bij de Codex-app horende runtime. Een losse CLI moet een bijpassende `codex-code-mode-host` hebben. Zo nodig kun je het juiste uitvoerbare bestand instellen met `COA_CODEX_BIN`; bestaande aanmeldgegevens blijven buiten de runverslagen.

```sh
cargo run --locked
```

Open **http://127.0.0.1:8766/**. De eerste start downloadt ongeveer 235 MB aan modelbestanden en aanvullende bestanden naar `.model-cache/`. Voor embeddings is geen API-sleutel nodig; Codex gebruikt je bestaande aanmelding. De server start nadat het lokale model is geladen.

Een andere lokale poort kan met `COA_PORT=8767 cargo run --locked`. `COA_CODEX_MODEL` kan het expliciet gekozen antwoordmodel veranderen; de gebruikte versie en instellingen worden per run vastgelegd.

## Zelf testen

1. Schrijf een eigen vraag in het lege vraagveld, of kies een optionele voorbeeldvraag. Probeer bijvoorbeeld “Welk werk is mogelijk op dinsdag?” Beide documenten beschrijven vrijwilligersactiviteiten, geen betaalde banen. De twee brondocumenten staan afzonderlijk in groene en blauwe panelen.
2. Laat Codex zoeken en antwoorden. Bekijk de exacte citaten, brondatum en gebruikte zoekvragen/passages.
3. Je kunt direct een nieuwe vraag stellen, zonder eerst te beoordelen. **Geef je oordeel** is optioneel: **Goed**, **Aandacht nodig** of **Weet ik niet**. Alleen bij een herkende vaste testvraag zijn daarna voorbereide criteria beschikbaar.
4. Vergelijk verwachte en gevonden bronsecties. Het eerste oordeel blijft staan; herhaalde beoordelingen en uitvoeringen zijn als voorkennis gemarkeerd.
5. Bekijk eerdere runs, download het testverslag of voer een vraag opnieuw uit. Een nieuwe generatie krijgt een eigen run. Gewijzigde of ontbrekende bron/runtime-instellingen worden niet stilzwijgend hergebruikt.

Een echt Codex-antwoord duurt in de lokale proef vaak ongeveer 45–50 seconden; de pagina toont ondertussen de voortgang en de bron blijft leesbaar. Er kan één modeluitvoering tegelijk lopen. Fouten blijven zichtbaar en tellen niet als geslaagde tests. Runs en oordelen staan uitsluitend in begrensd servergeheugen en verdwijnen bij herstart. De browser onthoudt welke criteria je al zag, ook na een nieuwe testsessie; voor onafhankelijke beoordeling gebruik je een apart browserprofiel/context.

## Rust-zoekfunctie en evaluatie

```sh
cargo run --locked -- search 'Waar en wanneer is het tuinieren?'
cargo run --locked -- ask 'Zijn er nog plekken vrij?'
cargo run --locked -- eval > /tmp/meedoen-eval.json
cargo test --locked
```

`fixtures/document.md` (tuinieren, brondatum 2026-10-20) en `fixtures/bicycle-workshop.md` (fietsen opknappen, brondatum 2026-10-18) zijn de twee geïndexeerde bronnen. Beide activiteiten vinden op dinsdag plaats, op verschillende tijden en locaties. De twaalf secties krijgen embeddings van het lokale, gekwantiseerde meertalige MiniLM-model via fastembed 7.1.2. Iedere embedding bevat documenttitel, sectiekop en tekst; teruggegeven bewijs behoudt de exacte originele tekst. CLI en MCP gebruiken dezelfde Rust-zoekfunctie. Iedere hit en ieder citaat behouden document-ID, sectie-ID en de eigen brondatum. Standaard worden drie secties gerangschikt; `--top-k 1..6` is instelbaar, ook bij twaalf secties. Overeenkomst is geen zekerheid en er is geen gekalibreerde grens voor “geen match”.

`fixtures/questions.json` bevat vijf vragen; `fixtures/expectations.json` is uitsluitend voor evaluatie en menselijke beoordeling. Verwachte antwoorden gaan niet naar de index, zoekresultaten of Codex. Codex krijgt alleen de vraag en daadwerkelijk opgehaalde passages via de beperkte MCP-tool.

Het [huidige zoekexperiment](reports/two-document-eval.json) vergelijkt embeddings met een woordoverlap-baseline op dezelfde titel, sectiekop en tekst. De vijf vaste vragen blijven als regressie- en menselijke testset beschikbaar via `fixtures/questions.json` en de API; ze vormen geen verplichte reeks in de pagina. Ze gaan expliciet over tuinieren; labels gebruiken document-ID én sectie-ID. Dit is bronsectiedekking, geen antwoordnauwkeurigheid of bewijs voor algemene kwaliteit. Q5 heeft geen relevante bronsectie; historische beschikbaarheid geeft geen actuele plekken.

De [titelcontextvergelijking](reports/two-document-context-comparison.json) toont drie echte zoekvragen op hetzelfde tweedocumentencorpus, met en zonder titelcontext. Voor “waar en wanneer tuinieren” stijgt de tijdpassage S2 naar de tweede plek, maar de locatie S3 blijft vijfde. De standaard drie hits vormen dus nog geen volledig antwoord. Codex kan aanvullende gerichte zoekvragen stellen. De dinsdagvraag haalt beide tijdpassages op. De rangorde is niet op deze vragen vastgezet; scores meten verwantschap en vormen geen zekerheid. Het eerdere [één-documentexperiment](reports/one-document-eval.json) blijft als historisch resultaat bewaard; corpus, vraagteksten en embeddinginvoer zijn sindsdien veranderd, dus de scores zijn geen zuivere vergelijking van alleen titelcontext.

CI controleert formattering, clippy, deterministische tests en build zonder modelaanroepen. De echte modelcontrole staat apart: `cargo test --locked live_report_retains -- --ignored`. De titelcontextvergelijking kan met `cargo test --locked live_two_document -- --ignored --nocapture` opnieuw worden gemeten (JSON in de tijdelijke systeemmap). [Besluiten](decisions.md) beschrijft zoekinstellingen, brondata en beperkingen van reproduceerbaarheid.

## Grenzen en werkwijze

Alle gegevens zijn verzonnen. Suggesties behouden bron en datum; ontbrekende of oude feiten blijven onbekend. Deelname is vrijwillig en een medewerker bespreekt opties met de bewoner. Geen dossierwijzigingen, berichten of aanmeldingen. De demo luistert alleen op localhost.

Alle codewijzigingen gaan via branches en pull requests, met onafhankelijke beoordeling. Niet rechtstreeks naar `main` committen; alleen mergen met toestemming van de gebruiker.

Bron: [fastembed 7.1.2](https://github.com/anush008/fastembed-rs/releases/tag/v7.1.2), gepubliceerd **19 oktober 2026**. Zie verder [projectinstructies](AGENTS.md), [use case](user_journey.md) en [2026-oriëntatie](agent-harness-landscape.md). Het langetermijndoel blijft een kleine, uitlegbare agent harness; deze proef houdt bewust twee bronnen en vijf vaste tuinvragen plus vrije vragen.
