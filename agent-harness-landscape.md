# Agent harness: landschap en architectuuroriëntatie

**Onderzocht op 13 oktober 2026.** Dit is een compact startpunt voor de architect, geen platformkeuze of volledige catalogus. Agent- en platformtermen lopen door elkaar; vergelijk concrete mogelijkheden en grenzen, niet alleen productnamen.

## Is dit nuttig?

Ja, als gedeelde kaart van begrippen en primaire bronnen. Dat helpt projectagents dezelfde onderdelen te herkennen en ontwerpkeuzes te onderbouwen. Een document kan actuele kennis echter niet garanderen: interfaces, SDK-ondersteuning en aanbevolen patronen veranderen snel. De bekende Anthropic-uitleg over simpele, samengestelde patronen is van december 2024; de auteur waarschuwt zelf dat het beschreven tool-landschap sindsdien veranderde. Gebruik die post dus voor het onderscheid tussen vaste workflows en modelgestuurde agents, niet als actuele productlijst. Controleer vóór iedere keuze de officiële documentatie en compatibiliteit die op dat moment gelden. [Anthropic: Building effective agents](https://www.anthropic.com/engineering/building-effective-agents)

## Onderdelen in deze demo

Een harness is de softwarelaag die invoer verwerkt, een model en tools aanstuurt, stappen en state beheert en een resultaat teruggeeft. Bij evaluatie telt het model samen met die harness; een goed eindantwoord alleen bewijst niet dat de hele keten goed werkte. [Anthropic: agent evals](https://www.anthropic.com/engineering/demystifying-evals-for-ai-agents)

| Onderdeel | Rol in de Meedoenbalie-demo |
| --- | --- |
| Taakafhandeling en loop | Zet de vraag om in een kleine, begrensde taak: relevante wensen vinden, activiteiten ophalen, matches onderbouwen en stoppen of om verduidelijking vragen. Kies expliciet welke stappen vaste code zijn en waar het model beslist. |
| Context en state | Bepaal welke fictieve notities, voorkeuren, eerdere stappen en beperkingen het model per stap ziet; houd taakstate apart van blijvende gegevens. Context is een beperkte, samengestelde invoer, geen onbegrensd geheugen. [Anthropic: context engineering](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents) |
| Tools | Geef alleen duidelijk afgebakende functies, bijvoorbeeld fictieve activiteitrecords zoeken of een activiteit-ID en velden uitlezen. Een toolprotocol maakt een koppeling mogelijk, maar bepaalt niet zelf of een tool veilig of passend is. |
| RAG en bewijs | Haal relevante passages op uit de synthetische notities en activiteitencatalogus. Bewaar per passage bron-ID en datum naast de tekst, zodat de uitvoer kan tonen wat een match ondersteunt en wat onbekend is. |
| Grenzen en fouten | Leg vast wat er gebeurt bij ontbrekende of verouderde gegevens, lege zoekresultaten, toolfouten, ongeldige modeluitvoer en een verzoek buiten de taak. De productgrenzen blijven leidend volgens [AGENTS.md](AGENTS.md). |
| Traces en logging | Registreer genoeg van een uitvoering om invoer, modelstappen, toolaanroepen, opgehaalde bronnen, fouten en resultaat te volgen. Bepaal welke velden nodig zijn om gedrag te begrijpen en hoe lang die bewaard worden. |
| Evals | Beoordeel de hele taak en trace: taakafhandeling, toolkeuze, relevante retrieval, brongetrouwe claims, correcte onbekenden en foutafhandeling. Test zowel een goede match als geen match en ontbrekende of verouderde informatie. Evals maken gewenst gedrag expliciet en helpen regressies vinden. [Anthropic: agent evals](https://www.anthropic.com/engineering/demystifying-evals-for-ai-agents) |

## Platformcategorieën om te vergelijken

Dit zijn voorbeelden van verschillende lagen, geen ranglijst en geen gelijkwaardige producten.

- **Model-API en runtime.** De [OpenAI Agents-documentatie](https://developers.openai.com/api/docs/guides/agents) onderscheidt de gehoste Agents API, de Agents SDK die de applicatie zelf aanstuurt en de Responses API voor directe modelintegratie of een eigen loop. Deze opties verschillen onder meer in waar de loop draait, wie state beheert en wie tools uitvoert. Verwissel bijvoorbeeld een Agents API-sessie, SDK-sessie en Responses-conversatie niet met elkaar.
- **Agent-SDK's.** Een SDK biedt bouwblokken zoals agentloop, toolaanroepen, overdrachten, sessies, guardrails en tracing. De [OpenAI Agents SDK](https://openai.github.io/openai-agents-python/) is een voorbeeld; [Google ADK](https://google.github.io/adk-docs/) documenteert onder meer agents, workflows, sessies en evaluaties. Controleer per SDK taal, modelaanbieders, state-opslag en de volwassenheid van functies.
- **Workflow- en graph-runtime.** [LangGraph](https://docs.langchain.com/oss/python/langgraph/overview) laat vaste stappen en modelgestuurde stappen combineren en richt zich op stateful orkestratie, hervatten en menselijke tussenkomst. Dat kan nuttig zijn als de taak echte vertakkingen of herstel nodig heeft, maar voegt concepten toe om te leren en beheren. [De persistence-documentatie](https://docs.langchain.com/oss/python/langgraph/persistence) onderscheidt threadgebonden kortetermijnstate (met checkpoints) van langdurige stores.
- **Beheerde agentdiensten.** Hierbij beheert een aanbieder een deel van de uitvoering, state of sandbox. [Anthropic Managed Agents](https://www.anthropic.com/engineering/managed-agents) is een voorbeeld van die categorie. Vergelijk waar uitvoering en data plaatsvinden, hoe tools en state gekoppeld zijn, welke observability beschikbaar is en hoe afhankelijkheid en kosten zich ontwikkelen.
- **Tool- en data-interoperabiliteit.** [MCP](https://modelcontextprotocol.io/specification/2026-07-28) standaardiseert hoe applicaties context en tools beschikbaar maken aan modellen; het is geen complete agent-runtime. `2026-07-28` was op 13 oktober 2026 de actuele definitieve revisie. De [toolspecificatie](https://modelcontextprotocol.io/specification/2026-07-28/server/tools) beschrijft onder meer tools ontdekken en aanroepen. Controleer vóór gebruik of zowel de gekozen client als server deze protocolversie ondersteunen; SDK's en integraties kunnen achterlopen of migratie vereisen.

Voor een leerproject is de relevante vraag niet hoeveel agentfuncties een platform aanbiedt, maar welke onderdelen je zelf wilt begrijpen en zichtbaar kunnen maken. Een gewone lokale functie kan toolgebruik al leren; MCP wordt vooral relevant als protocolinteroperabiliteit zelf een leerdoel of integratie-eis is. Een graph-runtime is alleen leerzaam als de taak branches, hervatten of expliciete menselijke tussenkomst nodig maakt. Een extra agentlaag is geen doel op zich.

## Keuzevragen voor de architect

Vergelijk kandidaten aan de hand van:

1. **Leerdoel:** blijft de loop zichtbaar genoeg om te begrijpen en uit te leggen?
2. **Taakvorm:** volstaat één eenvoudige loop, of zijn vaste routes, retries, hervatten of meerdere stappen met menselijke invoer nodig?
3. **State en context:** waar worden sessies en taakstate bewaard, en zijn die eenvoudig te inspecteren en wissen?
4. **Bronbewijs:** kan de implementatie opgehaalde passage, bron en versheid betrouwbaar tot aan iedere suggestie bewaren?
5. **Controle en uitvoering:** welke functies beheert het platform, waar draaien tools en hoe kan de uitvoering worden begrensd en gevolgd?
6. **Evals en traces:** zijn toolkeuze, opgehaalde bronnen en tussenstappen toegankelijk genoeg om fouten en regressies te verklaren?
7. **Praktische fit:** taal en ecosysteem, lokale ontwikkelervaring, documentatie, upgradepad, operationele inspanning, kosten en afhankelijkheid van één aanbieder.

Leg na de vergelijking de gekozen architectuur, alternatieven, beslisredenen en open onzekerheden vast. Begin klein en voeg alleen complexiteit toe die het leerdoel of de taak aantoonbaar vereist; Anthropic beschrijft die eenvoudsregel in 2024, en LangGraph waarschuwt zelf dat het een lage-abstractielaag voor orkestratie is. [Anthropic](https://www.anthropic.com/engineering/building-effective-agents) · [LangGraph](https://docs.langchain.com/oss/python/langgraph/overview)

## Bronnen en actualiteit

De links hierboven zijn primaire documentatie of engineering-uitleg van de betrokken project- of platformmakers. Gebruik leveranciersdocumentatie voor actuele werking en ondersteuning, niet als onafhankelijke vergelijking. De [Anthropic-post over context engineering](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents) is gepubliceerd op 29 september 2025 en de [post over evals](https://www.anthropic.com/engineering/demystifying-evals-for-ai-agents) op 9 januari 2026. Controleer vóór platformkeuze en implementatie opnieuw de officiële docs, changelogs, versiecompatibiliteit, prijs- en dataverwerkingsvoorwaarden.
