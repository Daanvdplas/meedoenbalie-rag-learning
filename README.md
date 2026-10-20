# Meedoenbalie: activiteiten verkennen

Een kleine Rust-leerproef met vijf verzonnen activiteiten, echte lokale embeddings en Codex. Je stelt een vraag en kunt de gebruikte bronpassages controleren. Dit is een eigen prototype, geen intern COA-plan.

Bekijk de [interactieve codegids](docs/code-tour.html) voor het systeemoverzicht, de route van één vraag en verwijzingen naar de echte functies. De HTML werkt ook lokaal zonder CDN.

## Starten

Benodigd: Rust 1.94.1 en een aangemelde Codex-client. De demo zoekt lokaal naar de bij de Codex-app horende runtime. Een losse CLI moet een bijpassende `codex-code-mode-host` hebben. Zo nodig kun je het juiste uitvoerbare bestand instellen met `COA_CODEX_BIN`; bestaande aanmeldgegevens blijven buiten de runverslagen.

```sh
cargo run --locked
```

Open **http://127.0.0.1:8766/**. De eerste start downloadt ongeveer 235 MB aan modelbestanden en aanvullende bestanden naar `.model-cache/`. Voor embeddings is geen API-sleutel nodig; Codex gebruikt je bestaande aanmelding. De server start nadat het lokale model is geladen.

Een andere lokale poort kan met `COA_PORT=8767 cargo run --locked`. `COA_CODEX_MODEL` kan het expliciet gekozen antwoordmodel veranderen; de gebruikte versie en instellingen worden per run vastgelegd.

## Zelf testen

1. Bovenaan staan vijf compacte activiteiten: **Tuinieren**, **Fietsen opknappen**, **Taalcafé**, **Samen koken** en **Wandelen**. Klik een titel om het volledige fictieve document met bron en datum te lezen.
2. Schrijf één eigen vraag, of kies een optioneel voorbeeld. Alle documenten beschrijven vrijwillige activiteiten, geen betaalde banen.
3. Codex maakt één antwoord. Onder **Bekijk zoekvragen en gevonden passages** staan de echte zoekacties van Codex. Codex kan meerdere keren zoeken om ontbrekende informatie voor dat ene antwoord te verzamelen; dit zijn geen extra gebruikersvragen.
4. Exact geciteerde tekst is gemarkeerd in de werkelijk gevonden passages. Bron, datum en een klik naar de juiste activiteit staan erbij. Er is geen afzonderlijk bewijsblok of beoordelingsflow.
5. Stel direct een nieuwe vraag, bekijk een eerdere uitvoering of voer die opnieuw uit. Een nieuwe generatie krijgt een eigen run; gewijzigde bron/runtime-instellingen worden niet stilzwijgend hergebruikt.

Een Codex-antwoord kan ongeveer een minuut duren; de bron blijft ondertussen leesbaar. Er kan één modeluitvoering tegelijk lopen. Fouten blijven zichtbaar. Vraaggeschiedenis staat in begrensd servergeheugen en verdwijnt bij herstart. Er is geen beoordelings- of criteriaflow; cosinusovereenkomsten staan alleen bij de technische zoekresultaten.

## Rust-zoekfunctie en evaluatie

```sh
cargo run --locked -- search 'Waar en wanneer is het tuinieren?'
cargo run --locked -- ask 'Zijn er nog plekken vrij?'
cargo run --locked -- eval > /tmp/meedoen-eval.json
cargo test --locked
```

Vijf documenten met elk zes secties zijn daadwerkelijk geïndexeerd:

| Bestand | Activiteit | Brondatum | Moment |
| --- | --- | --- | --- |
| `fixtures/document.md` | Tuinieren | 2026-10-20 | dinsdag 10–12 |
| `fixtures/bicycle-workshop.md` | Fietsen opknappen | 2026-10-18 | dinsdag 14–16 |
| `fixtures/language-cafe.md` | Taalcafé | 2026-10-19 | woensdag 14–15.30 |
| `fixtures/community-cooking.md` | Samen koken | 2026-10-17 | vrijdag 11–13 |
| `fixtures/walking.md` | Wandelen | 2026-10-20 | donderdag 9.30–10.30 |

De parser accepteert maximaal vijf documenten. De dertig secties krijgen lokale meertalige MiniLM-embeddings via fastembed 7.1.2: documenttitel, sectiekop en exacte tekst. CLI en MCP delen dezelfde Rust-zoekfunctie. Iedere hit en ieder citaat behouden document-ID, sectie-ID en eigen brondatum. De manifest bevat per-documenthashes, corpus- en embeddinginvoerhashes. Standaard top-k is 3, instelbaar van 1 tot 6 per zoekactie; Codex kan maximaal vier gerichte zoekacties doen. Dit garandeert geen volledige lijst of volledig antwoord. Cosinusovereenkomst is geen zekerheid; een grens voor “geen match” is niet gekalibreerd.

De [huidige evaluatie](reports/five-activity-eval.json) vergelijkt echte embeddings met woordoverlap op dezelfde titel/kop/tekst. De vijf vaste tuinvragen en beschermde verwachtingen blijven uitsluitend intern beschikbaar voor regressie en menselijke evaluatie via `fixtures/questions.json`, `fixtures/expectations.json` en de bestaande server-API. De demo toont deze reeks niet. Gold wordt nooit geïndexeerd of naar Codex/zoekresultaten gestuurd. Gemiddelde bronsectiedekking op de vier gelabelde tuinvragen is in dit snapshot 0,625 voor beide methoden. Dit is geen antwoordnauwkeurigheid; actuele plekken en ontbrekend vervoer blijven onbekend.

De rapporten voor [één document](reports/one-document-eval.json) en [twee documenten](reports/two-document-eval.json), inclusief de [oude titelcontextvergelijking](reports/two-document-context-comparison.json), zijn historische snapshots. Documenttitels en corpus zijn nu gewijzigd; vergelijk die scores niet alsof alleen één zoekinstelling veranderde. De [vijf verkennende zoekvragen](reports/five-activity-searches.json) tonen ook echte missers: bij “waar en wanneer tuinieren” ontbreken tijd en locatie in de eerste zes hits. Codex kan gericht verderzoeken, maar een volledig antwoord is niet gegarandeerd. Er wordt geen rangschikking op vaste vragen afgedwongen.

CI controleert formattering, clippy, deterministische tests en build zonder modelaanroepen. Live checks: `cargo test --locked live_report_retains -- --ignored` en `cargo test --locked live_five_activity -- --ignored --nocapture`. De laatste schrijft een kleine titelcontextvergelijking naar de tijdelijke systeemmap, geen grotere benchmark. Zie [besluiten](decisions.md) voor instellingen en beperkingen.

## Grenzen en werkwijze

Alle gegevens zijn verzonnen. Suggesties behouden bron en datum; ontbrekende of oude feiten blijven onbekend. Deelname is vrijwillig en een medewerker bespreekt opties met de bewoner. Geen dossierwijzigingen, berichten of aanmeldingen. De demo luistert alleen op localhost.

Alle codewijzigingen gaan via branches en pull requests, met onafhankelijke beoordeling. Niet rechtstreeks naar `main` committen; alleen mergen met toestemming van de gebruiker.

Bron: [fastembed 7.1.2](https://github.com/anush008/fastembed-rs/releases/tag/v7.1.2), gepubliceerd **19 oktober 2026**. Zie verder [projectinstructies](AGENTS.md), [use case](user_journey.md) en [2026-oriëntatie](agent-harness-landscape.md). Het langetermijndoel blijft een kleine, uitlegbare agent harness; deze proef houdt bewust vijf activiteiten en vrije vragen, met vijf interne tuinvragen als regressieset.
