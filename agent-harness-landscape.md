# Agent engineering: 2026-orientatie

**Bronnen gecontroleerd op 19 oktober 2026.** Alle bronnen hieronder zijn in 2026 gepubliceerd of expliciet in 2026 bijgewerkt; de datum staat bij de verwijzing. Veel bronnen komen van platformmakers: ze zijn bruikbaar om hun eigen aanpak te begrijpen, maar vormen geen onafhankelijke vergelijking. Controleer actuele documentatie opnieuw voordat je een technische keuze maakt.

## Huidige leerstap

Euan vond de volledige richting redelijk, maar breed voor een eerste project. Zijn advies was om één onderdeel te kiezen en met een RAG-pipeline te beginnen: retrieval is op zichzelf al een uitdaging. Daarom is de eerste beoogde leerstap een kleine pipeline met embeddings en synthetische documenten. Eerst beoordelen we welke passages de zoekfunctie vindt; daarna kan een taalmodel die passages gebruiken om antwoorden te maken. Dit is een eerste mijlpaal, geen vervanging van het langetermijndoel: leren hoe een kleine agent harness werkt.

Er is nog geen toestemming om te bouwen. Er is ook nog geen keuze gemaakt voor embeddingmodel, documentverwerking, chunking, zoekopslag, retrieval-aanpak, taalmodel, runtime of MCP-client.

## Concepten voor deze demo

| Concept | Wat het doet | Wat we ermee leren |
| --- | --- | --- |
| RAG-pipeline | Verwerkt documenten, splitst ze in passages, maakt embeddings en een index. Bij een vraag haalt de applicatie relevante passages op en geeft die als context mee aan een taalmodel. De [Google Cloud RAG-referentiearchitectuur](https://docs.cloud.google.com/architecture/gen-ai-rag-vertex-ai-vector-search) (laatst beoordeeld op **16 oktober 2026**) beschrijft deze stappen in een Google-implementatie. | Kunnen we de relevante passages vinden en de bijbehorende bron en datum behouden? |
| Embeddings en vectorzoeken | Embeddings zijn numerieke representaties van tekst die vectorvergelijking gebruikt om inhoud met verwante betekenis te vinden. Googles [BigQuery-voorbeeld](https://cloud.google.com/blog/products/data-analytics/introducing-bigquery-autonomous-embedding-generation), gepubliceerd op **20 februari 2026**, laat één beheerde implementatie zien. Het is geen advies om BigQuery te kiezen. | Welke invloed hebben documentopmaak, chunking, embeddingmodel en zoekinstellingen op gevonden passages? |
| Retrieval-evaluatie | Beoordeel eerst welke passages bovenaan verschijnen voor vaste vragen. Beoordeel daarna apart of het model die passages correct gebruikt. Anthropic adviseert in [Demystifying evals for AI agents](https://www.anthropic.com/engineering/demystifying-evals-for-ai-agents) (**9 januari 2026**) om de hele uitvoering en tussenstappen te inspecteren, niet alleen het eindantwoord. | Kunnen we een goede zoekhit onderscheiden van een antwoord dat alleen overtuigend klinkt? |
| Traces | Een trace legt de stappen van één uitvoering vast, zoals opgehaalde passages, toolaanroepen, fouten en antwoord. De [OpenAI-agentverbeteringslus](https://developers.openai.com/cookbook/examples/agents_sdk/agent_improvement_loop) (**12 mei 2026**) laat zien hoe traces en evals samen een verbetercyclus vormen. | Kunnen we verklaren waarom een relevante passage wel of niet werd gebruikt? |
| Agent harness en runtime | De harness stuurt model, taak, state, tools en uitvoering aan. Een applicatie kan een SDK gebruiken om die loop te beheren; een beheerde dienst kan meer uitvoering uit handen nemen. OpenAI beschrijft beide kanten in [de Agents SDK-update](https://openai.com/index/the-next-evolution-of-the-agents-sdk/) (**15 april 2026**) en [de Agents API-aankondiging](https://openai.com/index/introducing-the-agents-api/) (**10 september 2026**). Anthropic beschrijft een beheerde aanpak in [Managed Agents](https://www.anthropic.com/engineering/managed-agents) (**8 april 2026**). Dit zijn productbeschrijvingen van de aanbieders zelf, geen onafhankelijke ranglijst. | Later vergelijken wie de loop, tools, state, uitvoering en observability beheert. Voor de eerste RAG-stap is die keuze uitgesteld. |
| Workflow en agentstappen | Een eenvoudige taak kan vaste code gebruiken; een complexere taak kan modelkeuzes of expliciete vertakkingen toevoegen. LangChain beschrijft graph-gebaseerde orkestratie in [3 Years of Graph Engineering with LangGraph](https://www.langchain.com/blog/3-years-of-graph-engineering-with-langgraph) (**22 juli 2026**). Dit is de visie van de maker van LangGraph. | Pas een graph-runtime onderzoeken als de taak vertakkingen of hervatten nodig maakt; die is geen voorwaarde om RAG te leren. |
| MCP | Het [Model Context Protocol](https://modelcontextprotocol.io/specification/2026-07-28) heeft als actuele, gedateerde revisie **28 juli 2026**. Het beschrijft hoe een client met externe tools en context kan werken; het is op zichzelf geen volledige agent-runtime. Een lokale RAG-zoekfunctie zou later via MCP beschikbaar gemaakt kunnen worden. | Een MCP-koppeling met Codex of Claude Desktop is een mogelijkheid om te onderzoeken, geen gekozen architectuur. Controleer de actuele ondersteuning van de specifieke client voordat je daarop bouwt. |

De [Google Cloud-gids voor productiegerichte agents](https://cloud.google.com/blog/products/ai-machine-learning/a-devs-guide-to-production-ready-ai-agents) (**25 februari 2026; bijgewerkt in september 2026**) geeft een vendor-specifieke oriëntatie op agentloop, state, geheugen, RAG, tools, evaluatie, beveiliging en observability. Gebruik die als begrippenkaart, niet als neutraal platformadvies.

## Open ontwerpvragen

Als de gebruiker expliciet zegt te beginnen, werk dan eerst de RAG-leerstap af en kies alleen wat daarvoor nodig is:

1. Hoe worden synthetische notities en activiteiten naar passages verwerkt, met bron-ID en datum behouden?
2. Welk embeddingmodel en welke zoekopslag passen bij het leerdoel, de kosten en de gewenste lokale of gehoste uitvoering?
3. Welke vaste vragen en verwachte relevante passages maken retrievalkwaliteit zichtbaar?
4. Welke factoren worden gecontroleerd aangepast, zoals parsing, chunking, metadatafilters of rangschikking?
5. Wanneer voegen we het taalmodel toe, en hoe controleren we dat antwoorden op de opgehaalde passages steunen?

De volledige agent harness blijft een latere ontwerpstap. Accounts, server-side toolrechten, agentloop, state, UI, volledige traces/evals en externe koppelingen zijn nog niet gekozen. Een platform met extra functies is geen doel op zichzelf; kies later pas een aanpak als het leerdoel of de taak die nodig maakt.

## Projectgrenzen

De demo gebruikt uitsluitend verzonnen bewoners- en activiteitendata. Houd openbare informatie over de COA-werkwijze duidelijk gescheiden van dit eigen prototype-idee. Toon bij suggesties bron en datum; vermeld ontbrekende of verouderde informatie als onbekend. Meedoen blijft vrijwillig en de medewerker bespreekt opties met de bewoner. De demo wijzigt geen dossiers, verstuurt geen berichten en meldt niemand aan. Zie [AGENTS.md](AGENTS.md) en [user_journey.md](user_journey.md) voor de volledige afspraken en use case.
