# Vastgelegde RAG-evaluatie — 23–26 oktober 2026

Deze map bewaart een echte, kleine uitvoering op de vijf fictieve activiteiten. De bron- en referentiedatum blijft **20 oktober 2026**; uitvoeringsdatums staan afzonderlijk in ieder record. Dit is verkennend bewijs van de evaluatieketen, geen algemene kwaliteitsgarantie.

## Uitvoering

Er zijn **40 generatiesessies** gereserveerd: de oorspronkelijke vier smoke-uitvoeringen, twee verklaarde technische vervolgproeven, tien overige pilotvragen en 24 vergelijkingsuitvoeringen. Alle pogingen blijven bewaard. Er zijn geen inhoudelijke retries gedaan.

| Fase | Pogingen | Technisch voltooid | Technische fout |
| --- | ---: | ---: | ---: |
| Oorspronkelijke smoke | 4 | 2 | 2 |
| Technische contextdiagnose | 1 | 0 | 1 |
| Contextvalidatie na gereviewde correctie | 1 | 1 | 0 |
| Overige pilot | 10 | 9 | 1 |
| Zes vragen × vier aanpakken | 24 | 24 | 0 |

Technisch voltooid betekent dat de uitvoering en het artifact geldig zijn. Het zegt nog niet dat het antwoord inhoudelijk goed of volledig is.

De twee oorspronkelijke directe-contextpogingen mislukten (één runtimefout en één time-out). Een aanvullende diagnose faalde op de strikte eventvalidatie; de gerichte validatie na de correctie slaagde. Pilotvraag P08 werd geweigerd door de tool-/zoekprotocolvalidatie (`unexpected_tool`). De bewaarde metadata maakt de precieze oorzaak niet zichtbaar en bewijst geen concrete verboden actie. P08 heeft geen geldig antwoord of positieve kwaliteitsuitkomst.

## Voorlopige beoordelaar

De acht vooraf gemaakte kandidaatantwoorden zijn alle acht afzonderlijk beoordeeld: overeenkomst **4/4 ontwikkelvoorbeelden** en **4/4 controlevoorbeelden** met de AI-referentie. Geen van deze acht gaf een onbeslist totaaloordeel of uitvoeringsfout. De kandidaten zijn geen extra gegenereerde RAG-antwoorden uit de veertig pogingen. Referentielabels en notities werden pas achteraf lokaal vergeleken en gingen niet naar de beoordelaar. Deze kleine overeenkomst is geen menselijke ijking of algemene betrouwbaarheidsscore.

<!-- Definitieve beoordelingsaantallen, bestanden en uitkomsten volgen na de begrensde uitvoering. -->

De definitieve CI-controle telt **125 geslaagde technische tests en twee expliciet overgeslagen tests**, met groene formatterings- en clippychecks. Deze tests controleren het meetinstrument, de grenzen en de berekeningen; ze zijn geen 125 bewijzen van inhoudelijk goede modelantwoorden. Iedere implementatie-PR is onafhankelijk gereviewd; [verification.json](verification.json) verwijst naar de exacte reviews en de echte artifactcontroles.

## Wat is gemeten?

Bewijsdekking, antwoordbeoordeling en efficiëntie blijven apart. Bewijsdekking controleert of de aangeboden passages de vooraf beschreven feiten kunnen onderbouwen. Het model beoordeelt daarnaast het opgeslagen antwoord. Een letterlijk geldig citaat bewijst op zichzelf geen inhoudelijke juistheid. Een technische fout kan geen positieve kwaliteitsuitkomst krijgen.

De zes vergelijkingsvragen staan per vraag naast vier aanpakken: Codex met echte MCP-zoekacties, alle dertig secties vooraf, één lokale MiniLM-selectie en één lokale BM25-selectie. Meer context en vervolgzoeken zijn onderdeel van de verschillende methodes. Zij bewijzen geen beter embeddingmodel. De directe-contextaanpak heeft geen retrievalscore; de aangeboden bewijsdekking is wel meetbaar.

Een concreet verschil: bij vergelijkingsvraag T02 bood de eerste agentzoekactie **0/2 complete bewijsdoelen**; na twee zoekacties **2/2**. De eerste selectie bevatte al de fietstijd 14–16 uur, maar nog niet de volledige benodigde combinatie met binnenlocatie/adres. **0/2 betekent dus niet nul nuttige informatie.** De eenmalige MiniLM- en BM25-selecties bleven op **0/2 complete bewijsdoelen**. Dit is bewijsdekking, geen antwoordnauwkeurigheid. Bij T06, waarvan de regenafspraken ontbreken, is bewijsdekking **niet van toepassing**; onbekende informatie krijgt geen verzonnen positieve score.

### Een betwistbaar modeloordeel om zelf te bekijken

Bij T01 kreeg directe context **4/4 complete bewijsdoelen**, maar het AI-oordeel was negatief: **9/11** criteria goed. Het antwoord zegt: “Je hoeft geen eigen tuingereedschap, fietsgereedschap of kookingrediënten mee te nemen: die zijn aanwezig.” Het gebruikte citaat uit **ACT-FIETS-002:S5, 18 oktober 2026** zegt: “Gereedschap en reparatiemateriaal zijn aanwezig.”

De beoordelaar vond dat *reparatiemateriaal* expliciet in de antwoordzin moest staan en keurde daarom `completeness` én `fact:bicycle-provided` af. Dat is één inhoudelijk discussiepunt dat twee criteria raakt, geen bewijs van twee onafhankelijke fouten. Het materiaal staat immers wel in het getoonde citaat. Dit is een **betwistbaar voorlopig oordeel**, geen bewezen inhoudelijke mislukking van directe context. Open T01 en vergelijk vraag, antwoord, citaat en criterium zelf. Gold, rubric en scores zijn na deze uitkomst ongewijzigd gebleven.

## Protocol en herkomst

[protocol.json](protocol.json) legt de vastgezette programmaversie, invoerhashes, modelroute, sessieplafonds en beperkingen vast. De oorspronkelijke smoke gebruikte een eerdere, geregistreerde programmaversie. De oorspronkelijke afspraak om het hele traject binnen één vieruursvenster te doen is **niet gehaald**. Grote onderbrekingsgaten tussen OS-timestamps zijn zichtbaar; de oorzaak is onbekend. De runner weigerde nieuwe sessies buiten het venster. Eerst bleven veertien, daarna tien antwoorden onbeoordeeld. Afzonderlijk toegestane voortzettingen houden alle tellers, fouten en oudere vensters bij.

Voor uitsluitend de laatste tien nog nooit beoordeelde antwoorden is de operationele tijdafspraak verruimd naar maximaal 24 uur, maximaal tien nieuwe beoordelingssessies en hoogstens vijf verdere vieruursverlengingen via de bestaande CLI. Concurrency, sessietime-out, generatieplafond en overige implementatielimieten zijn ongewijzigd. Invalids en time-outs worden niet herhaald. De ruimere tijdafspraak staat apart in [continuation-24h-authorization.json](continuation-24h-authorization.json). Iedere poging houdt zijn eigen manifest en identiteit. Tijdens de vergelijking zijn brondata, vragen, criteria, generatorprompt en rubric ongewijzigd gebleven.

Een aangetoonde technische fout is apart onderzocht: een cosmetische waarschuwing over een experimentele isolatieoptie werd door de strikte parser afgewezen. De gereviewde correctie onderdrukt alleen die bekende melding per geïsoleerde opdracht. Andere foutmeldingen en verboden tools blijven afgewezen. De oorspronkelijke mislukte pogingen blijven bewaard; de private melding is verwijderd en staat nergens in deze map.

## Grenzen van de conclusie

- De zes doelbewust gekozen vergelijkingsvragen combineren reeds bekende fictieve bronfeiten. Dit is geen onafhankelijk domein, representatieve gebruikerssteekproef of statistisch bewijs van een winnaar.
- Per vraag en aanpak is één uitvoering gepland. Modelvariatie is daardoor niet geschat.
- De acht referentieantwoorden zijn apart beoordeeld door een AI-agent en nagekeken door een tweede agent. Dit is **geen menselijke ijking**. De vier ontwikkel- en vier controlevoorbeelden blijven afzonderlijk; na de controle is niet afgestemd om de overeenstemming te verhogen. Menselijke ijking volgt later in [issue 46](https://github.com/Daanvdplas/meedoenbalie-rag-learning/issues/46).
- Generator en beoordelaar gebruiken dezelfde modelroute/familie en kunnen dezelfde fouten maken. Afzonderlijke sessies bewijzen geen onafhankelijke inhoudelijke beoordeling. Alle modeloordelen blijven voorlopig.
- Een modelalias en de lokaal vastgelegde CLI-identiteit maken providergewichten niet volledig observeerbaar. Dezelfde invoer garandeert geen identieke toekomstige antwoorden.
- Het budget telt generatiesessies en beoordelingssessies. Interne providerrequests en exacte kosten zijn niet volledig beschikbaar; onbekende usage betekent niet nul.
- De gedeelde initialisatie van de lokale dense index valt buiten de tijd per vraag; de koude MCP-start van de agent valt er wel binnen. Tijdsverschillen hebben dus deze praktische beperking.
- Uitsluitend fictieve gegevens zijn gebruikt. Lokale embeddings betekenen niet dat generatie lokaal is: passages en beoordelingen gaan via Codex naar OpenAI. Deze oplevering publiceert geen website en geeft geen toestemming voor echte persoonsgegevens.

## Zelf bekijken

Volg de [reproduceerhandleiding](../../reproduce.md) voor de opdrachten en voorwaarden. Maak vanuit de repository een rapport uit deze opgeslagen resultaten, zonder modelaanroepen:

```sh
COA_EVIDENCE=docs/evaluation/results/2026-10-26
mkdir -p /tmp/coa-rag-results
cargo run --locked -- experiment report --input "$COA_EVIDENCE/smoke-graded.jsonl,$COA_EVIDENCE/pilot-graded.jsonl,$COA_EVIDENCE/heldout-final-graded.jsonl" --output /tmp/coa-rag-results/resultaten.html
```

Open `/tmp/coa-rag-results/resultaten.html` in je browser. [answer-hashes.json](answer-hashes.json) bewaart hashes van antwoord, aangeboden context en zoektrace voor ieder van de veertig pogingen. [verification.json](verification.json) legt vast of de afzonderlijke beoordeling die generatiegegevens ongewijzigd heeft behouden. Voeg een generatiebestand en zijn beoordeelde kopie niet samen: dezelfde run-ID mag slechts één keer in een rapport staan. Diagnoses en extra validatiepogingen blijven daarom afzonderlijk zichtbaar.
