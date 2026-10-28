# Het lokale evaluatierapport

Het rapport is voor de maker en mensen die zonder technische logs willen begrijpen wat retrieval wel en niet vindt. De vorm volgt de activiteiten-demo: rustig Nederlands, compacte vergelijking, kleuren voor antwoord en bronbewijs, technische details pas na uitklappen.

`experiment report --input resultaten.jsonl --output rapport.html` maakt één zelfstandig HTML-bestand. Meerdere JSONL-bestanden kunnen als kommagescheiden lijst mee. Deze opdracht gebruikt geen model of netwerk. Open het HTML-bestand daarna rechtstreeks in je browser. Voor een preview kun je alleen de map met HTML serveren; budgetregistraties en logs horen buiten die map.

`fixtures/experiments/report-example.jsonl` bevat zes handgemaakte rendererchecks, geen gemeten RAG-resultaten. De banner blijft zichtbaar. De echte generatie- en beoordelaarstatus en hun herkomst blijven afzonderlijk zichtbaar. Een voorbeeldbeoordeling telt nooit als een echt positief kwaliteitsoordeel. Menselijke ijking is voor nu uitgesteld.

Bewijsdekking, voorlopig AI-oordeel en efficiëntie blijven apart. `1 van 2 (0,50)` betekent dat bewijs voor één van twee benodigde feiten aanwezig is. De antwoordbeoordelaar controleert of de feiten ook juist gebruikt worden. Zijn oordeel kan een harde fout niet herstellen. Bij onbesliste criteria toont het rapport zowel de beoordeelde noemer als het aantal onbesliste criteria.

Voorbeelden, ontwikkeling, pilot en vooraf apart gehouden vragen worden niet samengevoegd in één vergelijking. Alleen opgeslagen uitvoeringen tellen mee; een ontbrekend artifact wordt geen nulmeting. Time-outs, annuleringen en overgeslagen uitvoeringen blijven zichtbaar. Een directe-contextaanpak zoekt niet: nul zoekacties en ‘context vooraf aangeboden’ zijn daar normaal.

De renderer valideert dataset-, vraag- en criteriumhashes en berekent meetwaarden opnieuw. Lege, foutieve of dubbele artifacts leveren een foutmelding op en vervangen geen bestaand rapport. Tekst wordt als tekst ge-escaped; het HTML bevat geen externe fonts, CDN of fetch-code. Filters werken ook als het bestand rechtstreeks vanaf schijf wordt geopend.
