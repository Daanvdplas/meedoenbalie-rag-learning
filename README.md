# Meedoenbalie RAG learning project

Een leerproject rond retrieval, embeddings en later een kleine agent harness, geïnspireerd op openbare COA-informatie. Dit is een eigen prototype-idee, geen bestaand COA-project en geen weergave van interne plannen.

## Waar het project nu staat

- De private GitHub-repo bevat alleen documentatie; er is nog geen applicatiecode.
- **Begin niet met bouwen totdat de gebruiker dat expliciet zegt.** De eerstvolgende leerstap is een kleine RAG-pipeline: retrieval opzetten en beoordelen met synthetische documenten. Een model eraan koppelen volgt daarna.
- Dit is de eerste mijlpaal, niet het nieuwe einddoel. Het langetermijndoel blijft leren hoe een kleine, uitlegbare agent harness voor het vinden van passende activiteiten werkt.
- Euan vond de richting redelijk, maar breed voor een eerste project. Zijn advies was om één onderdeel te kiezen en met RAG te beginnen; retrieval is op zichzelf al een uitdaging met veel optimalisatiemogelijkheden.

## Richting en open keuzes

- Embeddings horen bij de beoogde RAG-leerstap. Embeddingmodel, opslag, documentverwerking, chunking, retrieval-aanpak en evaluatiemethode zijn nog niet gekozen.
- Een model kan later de opgehaalde passages gebruiken om antwoorden te formuleren. Codex of Claude Desktop als MCP-client zijn mogelijke manieren om een lokale zoektool uit te proberen; geen van beide is gekozen of gekoppeld.
- Accounts, toolrechten, agentloop, UI, traces/evals voor de volledige harness en eventuele externe koppelingen horen bij de latere scope. Herbekijk die scope voordat je eraan begint.

## Blijvende grenzen

Gebruik alleen verzonnen bewoners- en activiteitendata. Toon bij suggesties bron en datum; laat ontbrekende of verouderde informatie als onbekend zien. Deelname blijft vrijwillig en de medewerker bespreekt suggesties met de bewoner. De demo wijzigt geen dossiers, verstuurt geen berichten en meldt niemand aan. Zie [AGENTS.md](AGENTS.md) en [user_journey.md](user_journey.md) voor de projectafspraken en use case.

## Werkwijze

Ontwikkelingen gaan via branches en pull requests; commit niet rechtstreeks naar `main` en merge een PR pas na expliciete toestemming. Branch protection is in de huidige private repo/accountconfiguratie niet technisch af te dwingen, dus volg dit proces handmatig. Er is nog geen applicatie om te starten of te testen.

## Projectdocumenten

- [AGENTS.md](AGENTS.md): korte instructies en veiligheidsgrenzen voor agents.
- [user_journey.md](user_journey.md): onderbouwde use case en voorgestelde AI-laag.
- [agent-harness-landscape.md](agent-harness-landscape.md): gedateerde oriëntatie op agentconcepten en platformkeuzes; controleer actuele documentatie voordat je keuzes maakt.
