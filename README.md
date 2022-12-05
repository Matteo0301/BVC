# BVC Market

Here we will store the library files of our Market.

## BVC Market Strategy

### Notazioni:

- Prezzo buy = trader compra dal market
- Prezzo sell = market compra dal trader
- Prezzo = tasso di conversione/exchange rate

**Premessa**: tutte le regole di prezzo specificate valgono solo per le risorse diverse dagli EUR, questi ultimi si cambiano sempre 1:1

## Allocazione risorse iniziale:

- tra 30% e 40% del MAX_CAPITALE per l'euro
- il restante 70-60% lo allochiamo in risorse con formula tra 30% e 40% del rimanente ogni volta

## Inizializzazione prezzi:

Converto tutte le risorse che non sono euro in euro, sottraggo da quella quantità il numero di euro che ho (quello non convertito), poi divido quella quantità per 3 e la chiamo "media".

Applico le seguenti regole:

- Se una risorsa è sotto la media allora avrà un prezzo più alto del default (incrementale), secondo questa regola:   
```((((media-qta_risorsa)/media)*0,50)+1)*prezzo_default```  

    **NB**: significa che se avessimo 0 di quella risorsa( anche se impossibile ) allora la pagheresti il 50% in più rispetto al default.

- Se una risorsa è tra la media e al più sopra di essa del 10% applico i prezzi di default.

- Se una risorsa supera la media del 10% applico un prezzo vantaggioso leggermente inferiore, cioè il 5% in meno rispetto al prezzo di default.

Le regole scritte sopra valgono per i prezzi di buy, il prezzo di sell è sempre ( nell'inizializzazione e in casi descritti dopo ) il 10% superiore al prezzo di buy.

## Elaborazione:

Applico le seguenti regole:

- Rifiuto le lock buy che mi lascerebbero con meno del 25% rispetto alla quantità iniziale di una certa risorsa.

- Rifiuto le lock sell che mi lascerebbero con meno del 20% degli EUR che avevo inizialmente.

- Se ho una quantità di una certa risorsa inferiore alla "media"(definita prima), si applica la regola definita nell'inizializzazione.

- Se supero del 0% ma meno del 10% la quantità iniziale di una risorsa, applico il prezzo di default per quella risorsa sia per il buy che per il sell.

- Se supero del 10% ma meno del 30% la quantità iniziale di una risorsa, allora applico un buy price pari al 99% del default price per quella risorsa. Il sell price lo metto al 15% sopra del prezzo di default.

- Se supero del 30% ma meno del 60% la quantità iniziale di una risorsa, allora applico un buy price pari al 98% del default price per quella risorsa. Il sell price lo metto al 25% sopra del prezzo di default.

- Se supero del 60% ma meno del 100% la quantità iniziale di una risorsa, allora applico un buy price pari al 97% del default price per quella risorsa. Il sell price lo metto al 35% sopra del prezzo di default.

- Se supero del 100% la quantità iniziale di una risorsa applico un buy price del 95% del default price di quella risorsa. Il sell price lo metto del 50% sopra al prezzo di default.

- Questa regola vale solo se il buy price della risorsa presa in considerazione supera o eguaglia il prezzo di default: se il trader vuole acquistare tra il 30% e il 50% della quantità di una mia risorsa, applico il 5% di sconto sul buy price; se invece vuole acquistare più del 50%, applico uno sconto del 10% sul buy price.

## Conversione di risorse:

- Usare un enum per ricordarmi se sono importer o exporter per ogni risorsa, se non sono nessuno dei due perchè non ho ancora deciso, mettiamo un valore flag.

- Se ho meno del 30% degli EUR che avevo inizialmente, allora prendo la risorsa che convertita in euro vale di più, a patto che valga di più della quantità di euro già posseduta, la segno come exporter se posso, altrimenti ne cerco un altra con gli stessi vincoli( se non c'è fa lo stesso ) , e con probabilità 50% ne devolvo il ```min(10000eur,40% risorsa scelta)``` in EUR altrimenti non faccio niente.

- Se ho una risorsa sofferente con meno del 30% della quantità iniziale, allora prendo la risorsa che convertita in euro vale di più, a patto che valga di più della quantità di euro che vale la risorsa sofferente ; segno la risorsa sofferente come importer e se la risorsa scelta per lo scambio non è l'euro, la segno come exporter se posso, altrimenti ne cerco un altra con gli stessi vincoli( se non c'è fa lo stesso ). Se la ho trovata con probabilità 60% ne devolvo il ```min(10000eur,30% risorsa scelta)``` in risorsa sofferente.


## Reazione agli eventi ( TBD )