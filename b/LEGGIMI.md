# Programma B (Numbrix): il metodo

Scritto leggendo solo `../SPEC_B.md`, senza guardare il programma A né i suoi risultati.
Rust, solo libreria standard, parallelismo con `std::thread`.

## Idea in breve

1. **Soluzioni.** Si enumerano tutti i cammini hamiltoniani orientati con una ricerca in profondità su maschere di
   bit (griglie fino a 64 caselle). Potature, tutte condizioni *necessarie*: parità a scacchiera, caselle libere
   connesse, ogni casella libera con almeno 2 vicini disponibili tranne al più una (la fine). Il lavoro si divide
   tra i fili per prefissi del cammino (lunghezza 7); le soluzioni escono in ordine lessicografico. Ogni soluzione è
   una "riga" `r[t]` = casella che contiene il numero `t+1`.
2. **Conteggio per proiezioni su coppie di posizioni.** Un insieme di indizi definente è sempre un sottoinsieme
   degli elementi di una soluzione (altrimenti nessuna soluzione lo contiene). Ordinando i suoi elementi per numero,
   una terna è `(c1,t1),(c2,t2),(c3,t3)` con `t1<t2<t3`. Per ogni **coppia di posizioni** `(t1,t2)` (un compito per
   filo, 1.176 compiti sul 7x7):
   - tutte le soluzioni si raggruppano per la chiave `(r[t1], r[t2])` con un counting sort (lettura in sequenza di
     due colonne della tabella trasposta);
   - un gruppo di dimensione 1 è una **coppia definente** (e allora ogni terna che la contiene è definente);
   - in un gruppo più grande si conta, per ogni `t3 > t2`, quante soluzioni del gruppo hanno ciascuna casella in
     posizione `t3` (tabella piccola `[t3][c3]`, sta nella cache L1); le voci uguali a 1 sono le **terne
     definenti**.
   - 1 indizio: elementi `(c,t)` presenti in una sola soluzione (istogramma di ogni colonna).

   Ogni insieme non ordinato si conta una volta sola, perché è fissato dai suoi due numeri più piccoli.

**Perché è diverso dallo schema escluso** ("per ogni elemento a, le soluzioni che contengono a, e una matrice di
conteggio delle coppie di altri elementi"): qui non si fissa mai un singolo elemento e non c'è nessuna matrice di
coppie. Si fissano due *posizioni* (numeri), si raggruppa per *coppia di elementi* con un ordinamento per
conteggio, e dentro il gruppo si usa solo un contatore a una dimensione sul terzo elemento. È la variante
"raggruppare per coppie di elementi" suggerita dalla specifica.

## Costo e memoria sul 7x7 (stima, il 7x7 NON è stato eseguito)

- Incrementi totali = soluzioni × C(N,3) = 27.070.560 × 18.424 ≈ 5,0·10^11.
- Prova misurata sul 6x7 (3.240.068 soluzioni, 861 compiti), con **2 fili** e la macchina occupata da un altro
  calcolo: conteggio 49,6 s, cioè circa 3,7·10^8 incrementi al secondo per filo. Proiettato sul 7x7: circa 22 minuti
  con 1 filo, pochi minuti con 16 fili (il limite vero sarà la banda della memoria). Ben dentro "qualche ora".
- Memoria sul 7x7: righe 1,33 GB + tabella trasposta 1,33 GB + indici 108 MB per filo (1,7 GB con 16 fili), più
  1,33 GB temporanei durante l'enumerazione: picco intorno a 4,5 GB.

## Controlli automatici

Ogni esecuzione (`griglia`, `cancello1`) controlla, e **non scrive il JSON** se uno fallisce:
- ogni soluzione è una permutazione con numeri consecutivi in caselle vicine (controllo con le coordinate, non con
  le maschere della ricerca) e le righe sono strettamente crescenti, quindi tutte diverse;
- sulle griglie n×n con n ≤ 6, il numero di soluzioni coincide con OEIS A096969 (1, 8, 40, 552, 8.648, 458.696);
- simmetria del rovescio: i conteggi per posizioni `(t1,t2,t3)` coincidono con quelli di `(N-1-t3, N-1-t2,
  N-1-t1)` (vengono da compiti diversi, quindi è un controllo vero); lo stesso per 1 e 2 indizi;
- ogni esempio è verificato cercando su *tutte* le soluzioni: una sola le contiene, ed è quella scritta.

`verifica` fa in più un confronto con una **forza bruta indipendente** su 1x1, 1x2, 2x1, 1x5, 2x2, 2x3, 3x2, 3x3,
2x5, 3x4, 4x3, 4x4, 4x5, 5x4, 5x5:
- soluzioni rifatte con una ricerca semplice senza potature (e, per N ≤ 9, provando tutte le N! permutazioni):
  stesso insieme;
- conteggi rifatti passando **tutti** gli insiemi di 1, 2, 3 elementi con caselle diverse e numeri diversi (anche
  quelli che nessuna soluzione contiene), contando le soluzioni compatibili con insiemi di bit: stessi numeri.

## Come si usa

Compilare (da dentro questa cartella):

```
cargo build --release
```

Eseguibile: `target\release\numbrix_b.exe`

```
numbrix_b.exe verifica                 # forza bruta sulle griglie piccole + valori OEIS
numbrix_b.exe cancello1                # 1x1, 2x2, 3x3, 3x4, 4x4, 4x5, 5x5, 6x6 -> JSON
numbrix_b.exe griglia 7 7              # il 7x7, quando sarà richiesto (scrive b_7x7.json)
opzioni: --fili K (predefinito: tutti), --uscita CARTELLA, --senza-file (solo stampa)
```

I JSON vanno in `strumenti\risultati\numbrix\b\b_{m}x{n}.json` con i campi `m`, `n`, `soluzioni`, `definenti_1`,
`definenti_2`, `definenti_3`, `esempi_3` (fino a 5, ognuno `{"indizi": [[casella, numero], ...], "soluzione":
[numero di ogni casella]}`), `secondi` (tempo totale della griglia: enumerazione, conteggio e controlli).
Gli esempi sono i primi nell'ordine (t1, t2, c1, c2, t3, c3), quindi in genere contengono i numeri 1 e 2.

Test di cargo (stessa forza bruta su griglie fino a 3x4 e valori noti fino a 5x5):
`cargo test --release --offline --target-dir ...`
