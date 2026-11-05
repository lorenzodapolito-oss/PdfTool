# PdfTool

Programma per Windows (scritto in Rust) che unisce due strumenti in uno:

- **Libri e fumetti in PDF**: EPUB (copertina + capitoli su A4, margini 2,5 cm, titoli, citazioni, elenchi e immagini) e fumetti CBZ / CBR / CB7 (una pagina per immagine, 150 dpi).
- **Immagini in un PDF unico** (A4, margine 1,5 cm, immagini centrate e mai ingrandite).
- **Immagini in un PDF per immagine**.
- **PDF in immagini** (JPEG, PNG o TIFF, risoluzione a scelta, file `nome_pag001.jpg`).

Si possono aggiungere file singoli o intere cartelle (cercate anche nelle sottocartelle), trascinarli sulla finestra, scegliere la cartella di destinazione e seguire l'avanzamento totale e del file corrente; il tasto **Annulla** ferma il lavoro. I file già esistenti non vengono mai sovrascritti (`nome (2).pdf`). Un file difettoso viene segnalato nel registro e gli altri continuano.

## Requisiti
- Per scrivere il testo degli EPUB servono caratteri di sistema (su Windows Arial o Segoe UI, già presenti).
- I file **CBR** (RAR) richiedono **7-Zip** installato (oppure `unrar`); CBZ e CB7 funzionano da soli.
- I PDF protetti da password non sono supportati.

## Compilare
Con l'ambiente portatile: metti `costruisci-pdftool.cmd` in `AmbienteRust` e il progetto in `Progetti\PdfTool`, poi avvia lo script. In alternativa `cargo build --release`.

## Sviluppo
`cargo test` e `cargo clippy --all-targets -- -D warnings`. Niente `unwrap`/`expect`/`panic` nel codice di produzione.
