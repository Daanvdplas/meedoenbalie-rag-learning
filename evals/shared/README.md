# Gedeelde vraagsets

`datasets/` bevat één canonieke kopie van de vragen en beschermde verwachtingen die zowel het retrieval-lab als de volledige-assistentproeven gebruiken:

| Map | Vraagset | Bronversie |
| --- | --- | --- |
| `legacy/` | P01–P12 en T01–T06 | Vijf activiteiten |
| `independent-20261103/` | IND26-01–IND26-12 | Vijf activiteiten |
| `expanded-10-20261105/` | Tien geselecteerde ontwikkelvragen | Tien activiteiten |
| `strengthened-10-20261106/` | RAG26-01–RAG26-10 | Tien activiteiten |

De vragen, verwachtingen, freezes en onafhankelijk opgestelde AUTHORING-notities zijn bytevast verplaatst. Oude padvelden in de manifesten identificeren de oorspronkelijke bestanden. De loaders vergelijken die historische sleutels en hashes met de huidige canonieke bestanden; de inhoud of herkomst is niet opnieuw gelabeld. Verwachtingen worden alleen voor beoordeling geladen, nooit als modelinstructies of zoekindexinhoud.

De actuele twaalfbronnendemo heeft nog geen eigen vaste kwaliteitsbenchmark. De oorspronkelijke vijf tuinvragen hebben alleen de retrievalregressie als eigenaar en staan daarom in `../rag/datasets/garden/`.
