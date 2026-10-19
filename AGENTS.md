# Project instructions

Lees eerst [README.md](README.md) voor de huidige status en besluiten. Lees voor de use case [user_journey.md](user_journey.md). Gebruik [agent-harness-landscape.md](agent-harness-landscape.md) als oriëntatie; verifieer platformkeuzes opnieuw.

## Doel en startpunt

Het langetermijndoel is een kleine, uitlegbare agent harness voor het vinden van passende activiteiten. De eerste leerstap is een RAG-pipeline met embeddings: eerst retrieval op synthetische documenten beoordelen, daarna eventueel een model koppelen. Dit vervangt het langetermijndoel niet.

**Begin niet met bouwen totdat de gebruiker dit expliciet zegt.** Model, embeddingmodel, opslag, documentverwerking, chunking, retrieval-aanpak en runtime zijn nog niet gekozen.

## Bronnen

Voor keuzes over agentplatforms en AI-engineering: gebruik bronnen die in 2026 zijn gepubliceerd of aantoonbaar bijgewerkt. Vermeld bij iedere bron de publicatie- of updatedatum; laat oudere bronnen weg.

## Grenzen

- Gebruik uitsluitend verzonnen bewoners- en activiteitendata; gebruik alleen openbare COA-informatie als achtergrond. Presenteer het voorstel niet als een bestaand of intern COA-plan; uitvoering kan per locatie verschillen.
- Toon bij iedere suggestie bron en datum. Laat ontbrekende of verouderde informatie als onbekend zien.
- Meedoen blijft vrijwillig; de medewerker bespreekt suggesties met de bewoner.
- De demo schrijft niets naar dossiers, verstuurt geen berichten en meldt niemand aan.

## Repo-werkwijze

Alle wijzigingen gaan via een branch en pull request; commit nooit rechtstreeks naar `main`. Merge geen PR zonder expliciete toestemming. Branch protection is met de huidige private repo/accountconfiguratie niet af te dwingen, dus volg de PR-werkwijze handmatig.
