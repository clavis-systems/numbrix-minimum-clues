//! Programma A (PIANO_NUMBRIX_48H.md): soluzioni di Numbrix m×n e insiemi definenti di 1, 2 e 3 indizi.
//!
//! - Una soluzione assegna 1..N alle caselle (c = riga*n + colonna) con k e k+1 in caselle vicine.
//! - Un elemento è (casella c, numero v), indice c*N + (v-1). Un insieme di indizi è definente se esattamente una
//!   soluzione lo contiene.
//! - Metodo: per ogni elemento a si enumerano (con una ricerca vincolata) le soluzioni che contengono a, e si
//!   contano, saturando a 2, le coppie (b, d) di altri elementi e i singoli b. Conteggio 1 = insieme definente.
//!   Ogni terna è trovata 3 volte (una per elemento), ogni coppia 2.
//! - Modo "simmetrie": solo un rappresentante per orbita di elementi (simmetrie della griglia e rovesciamento
//!   v -> N+1-v), pesato con la grandezza dell'orbita. Serve da controllo contro il modo completo.
//!
//! Uso:
//!   numbrix_a conta M N                        numero di soluzioni
//!   numbrix_a definenti M N [simmetrie]        JSON su stdout
//!   numbrix_a verifica M N c1:v1 c2:v2 ...     numero di soluzioni con quegli indizi (si ferma a 3)
use std::env;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::Instant;

struct Griglia {
    m: usize,
    n: usize,
    nn: usize,
    vicini: Vec<Vec<usize>>,
    vic_mask: Vec<u64>,
    pieno: u64,
    no_col0: u64,
    no_colult: u64,
}

impl Griglia {
    fn new(m: usize, n: usize) -> Griglia {
        let nn = m * n;
        assert!(nn >= 1 && nn <= 64);
        let mut vicini = vec![Vec::new(); nn];
        let mut vic_mask = vec![0u64; nn];
        for r in 0..m {
            for c in 0..n {
                let i = r * n + c;
                let mut v = Vec::new();
                if r > 0 { v.push(i - n); }
                if r + 1 < m { v.push(i + n); }
                if c > 0 { v.push(i - 1); }
                if c + 1 < n { v.push(i + 1); }
                for &x in &v { vic_mask[i] |= 1u64 << x; }
                vicini[i] = v;
            }
        }
        let pieno = if nn == 64 { u64::MAX } else { (1u64 << nn) - 1 };
        let mut col0 = 0u64;
        let mut colult = 0u64;
        for r in 0..m {
            col0 |= 1u64 << (r * n);
            colult |= 1u64 << (r * n + n - 1);
        }
        Griglia { m, n, nn, vicini, vic_mask, pieno, no_col0: pieno & !col0, no_colult: pieno & !colult }
    }

    fn espandi(&self, s: u64) -> u64 {
        let n = self.n as u32;
        let mut t = s;
        t |= (s << 1) & self.no_col0;
        t |= (s >> 1) & self.no_colult;
        t |= s << n;
        t |= s >> n;
        t & self.pieno
    }

    fn dist(&self, a: usize, b: usize) -> usize {
        let (ra, ca) = (a / self.n, a % self.n);
        let (rb, cb) = (b / self.n, b % self.n);
        ra.abs_diff(rb) + ca.abs_diff(cb)
    }
}

/// Vincoli: lista di (casella, numero). La ricerca restituisce solo le soluzioni che li rispettano.
struct Ricerca<'a, F: FnMut(&[u8]) -> bool> {
    g: &'a Griglia,
    vincoli: &'a [(usize, usize)],
    val: Vec<u8>,
    cb: F,
    stop: bool,
}

impl<'a, F: FnMut(&[u8]) -> bool> Ricerca<'a, F> {
    /// Controlla i vincoli per il numero k messo nella casella x, con l'insieme delle libere `liberi` dopo la mossa.
    fn vincoli_ok(&self, x: usize, k: usize, liberi: u64) -> bool {
        for &(c, v) in self.vincoli {
            if v == k {
                if c != x { return false; }
            } else if v > k {
                if (liberi >> c) & 1 == 0 { return false; }
                let d = self.g.dist(x, c);
                if d > v - k || (v - k - d) % 2 != 0 { return false; }
            } else if self.val[c] as usize != v {
                return false;
            }
        }
        true
    }

    /// Potatura: nel grafo H = libere ∪ {testa} serve un percorso hamiltoniano che parte dalla testa.
    fn potatura_ok(&self, testa: usize, liberi: u64) -> bool {
        if liberi == 0 { return true; }
        let h = liberi | (1u64 << testa);
        let mut bassi = 0;
        let mut l = liberi;
        while l != 0 {
            let u = l.trailing_zeros() as usize;
            l &= l - 1;
            let d = (self.g.vic_mask[u] & h).count_ones();
            if d == 0 { return false; }
            if d == 1 {
                bassi += 1;
                if bassi >= 2 { return false; }
            }
        }
        let mut r = 1u64 << testa;
        loop {
            let r2 = self.g.espandi(r) & h;
            if r2 == r { break; }
            r = r2;
        }
        r == h
    }

    fn dfs(&mut self, testa: usize, k: usize, liberi: u64) {
        if self.stop { return; }
        if k == self.g.nn {
            if !(self.cb)(&self.val) { self.stop = true; }
            return;
        }
        let g = self.g;
        for &x in &g.vicini[testa] {
            if (liberi >> x) & 1 == 0 { continue; }
            let l2 = liberi & !(1u64 << x);
            if !self.vincoli_ok(x, k + 1, l2) { continue; }
            if !self.potatura_ok(x, l2) { continue; }
            self.val[x] = (k + 1) as u8;
            self.dfs(x, k + 1, l2);
            self.val[x] = 0;
            if self.stop { return; }
        }
    }

    /// Avvia da una casella iniziale (numero 1).
    fn da(&mut self, s: usize) {
        let liberi = self.g.pieno & !(1u64 << s);
        if !self.vincoli_ok(s, 1, liberi) { return; }
        if !self.potatura_ok(s, liberi) { return; }
        self.val[s] = 1;
        self.dfs(s, 1, liberi);
        self.val[s] = 0;
    }
}

fn enumera<F: FnMut(&[u8]) -> bool>(g: &Griglia, vincoli: &[(usize, usize)], partenze: &[usize], cb: F) {
    let mut r = Ricerca { g, vincoli, val: vec![0u8; g.nn], cb, stop: false };
    for &s in partenze {
        r.da(s);
        if r.stop { break; }
    }
}

fn thread_n() -> usize {
    thread::available_parallelism().map(|x| x.get()).unwrap_or(4)
}

fn conta(g: &Griglia) -> u64 {
    let prossimo = AtomicUsize::new(0);
    let totale = Mutex::new(0u64);
    thread::scope(|sc| {
        for _ in 0..thread_n() {
            sc.spawn(|| {
                let mut mio = 0u64;
                loop {
                    let s = prossimo.fetch_add(1, Ordering::Relaxed);
                    if s >= g.nn { break; }
                    enumera(g, &[], &[s], |_| { mio += 1; true });
                }
                *totale.lock().unwrap() += mio;
            });
        }
    });
    let t = *totale.lock().unwrap();
    t
}

/// Simmetrie delle caselle: 8 per il quadrato, 4 per il rettangolo.
fn simmetrie_caselle(g: &Griglia) -> Vec<Vec<usize>> {
    let (m, n) = (g.m, g.n);
    let mut out = Vec::new();
    let mut aggiungi = |f: &dyn Fn(usize, usize) -> (usize, usize)| {
        let mut p = vec![0usize; g.nn];
        for r in 0..m {
            for c in 0..n {
                let (r2, c2) = f(r, c);
                p[r * n + c] = r2 * n + c2;
            }
        }
        out.push(p);
    };
    aggiungi(&|r, c| (r, c));
    aggiungi(&|r, c| (r, n - 1 - c));
    aggiungi(&|r, c| (m - 1 - r, c));
    aggiungi(&|r, c| (m - 1 - r, n - 1 - c));
    if m == n {
        aggiungi(&|r, c| (c, r));
        aggiungi(&|r, c| (n - 1 - c, m - 1 - r));
        aggiungi(&|r, c| (c, m - 1 - r));
        aggiungi(&|r, c| (n - 1 - c, r));
    }
    out
}

struct Esito {
    gruppi: usize,
    somma_gruppi: u64,
    singoli: u64,
    coppie_inc: u64,
    terne_inc: u64,
    esempi: Vec<(usize, usize, usize)>,
}

fn definenti(g: &Griglia, simmetrie: bool) -> Esito {
    let nn = g.nn;
    let ni = nn * nn;
    // elementi da elaborare, con il peso (grandezza dell'orbita)
    let mut lavori: Vec<(usize, u64)> = Vec::new();
    if simmetrie {
        let sim = simmetrie_caselle(g);
        let mut visto = vec![false; ni];
        for it in 0..ni {
            if visto[it] { continue; }
            let (c, v) = (it / nn, it % nn + 1);
            let mut orbita = Vec::new();
            for p in &sim {
                for &rev in &[false, true] {
                    let v2 = if rev { nn + 1 - v } else { v };
                    let j = p[c] * nn + (v2 - 1);
                    if !orbita.contains(&j) { orbita.push(j); }
                }
            }
            for &j in &orbita { visto[j] = true; }
            lavori.push((it, orbita.len() as u64));
        }
    } else {
        for it in 0..ni { lavori.push((it, 1)); }
    }
    let prossimo = AtomicUsize::new(0);
    let esito = Mutex::new(Esito { gruppi: lavori.len(), somma_gruppi: 0, singoli: 0, coppie_inc: 0, terne_inc: 0, esempi: Vec::new() });
    let partenze: Vec<usize> = (0..nn).collect();
    thread::scope(|sc| {
        for _ in 0..thread_n() {
            sc.spawn(|| {
                let mut c2 = vec![0u8; ni * ni];
                let mut c1 = vec![0u8; ni];
                let mut el = vec![0usize; nn];
                let (mut som, mut sing, mut cop, mut ter) = (0u64, 0u64, 0u64, 0u64);
                let mut esempi = Vec::new();
                loop {
                    let k = prossimo.fetch_add(1, Ordering::Relaxed);
                    if k >= lavori.len() { break; }
                    let (a, peso) = lavori[k];
                    let (ca, va) = (a / nn, a % nn + 1);
                    let mut quante = 0u64;
                    enumera(g, &[(ca, va)], &partenze, |val| {
                        quante += 1;
                        let mut q = 0;
                        for x in 0..nn {
                            if x == ca { continue; }
                            el[q] = x * nn + (val[x] as usize - 1);
                            q += 1;
                        }
                        for i in 0..q {
                            let b = el[i];
                            if c1[b] < 2 { c1[b] += 1; }
                            let base = b * ni;
                            for j in (i + 1)..q {
                                let p = &mut c2[base + el[j]];
                                if *p < 2 { *p += 1; }
                            }
                        }
                        true
                    });
                    som += quante * peso;
                    if quante == 1 { sing += peso; }
                    let mut coppie_qui = 0u64;
                    for b in 0..ni {
                        if c1[b] == 1 { coppie_qui += 1; }
                    }
                    let mut terne_qui = 0u64;
                    for b in 0..ni {
                        if c1[b] == 0 { continue; }
                        let base = b * ni;
                        for d in (b + 1)..ni {
                            if c2[base + d] == 1 {
                                terne_qui += 1;
                                // con le simmetrie a è il rappresentante dell'orbita: la terna resta un esempio valido
                                if esempi.len() < 5 && (simmetrie || a < b) {
                                    esempi.push((a, b, d));
                                }
                            }
                        }
                    }
                    cop += coppie_qui * peso;
                    ter += terne_qui * peso;
                    c1.iter_mut().for_each(|x| *x = 0);
                    c2.iter_mut().for_each(|x| *x = 0);
                }
                let mut e = esito.lock().unwrap();
                e.somma_gruppi += som;
                e.singoli += sing;
                e.coppie_inc += cop;
                e.terne_inc += ter;
                for x in esempi {
                    if e.esempi.len() < 5 { e.esempi.push(x); }
                }
            });
        }
    });
    esito.into_inner().unwrap()
}

fn verifica(g: &Griglia, vincoli: &[(usize, usize)], limite: u64) -> (u64, Vec<u8>) {
    let mut n = 0u64;
    let mut prima = Vec::new();
    let partenze: Vec<usize> = (0..g.nn).collect();
    enumera(g, vincoli, &partenze, |val| {
        n += 1;
        if prima.is_empty() { prima = val.to_vec(); }
        n < limite
    });
    (n, prima)
}

fn main() {
    let a: Vec<String> = env::args().collect();
    let modo = a[1].as_str();
    let m: usize = a[2].parse().unwrap();
    let n: usize = a[3].parse().unwrap();
    let g = Griglia::new(m, n);
    let t0 = Instant::now();
    match modo {
        "conta" => {
            let s = conta(&g);
            println!("{{\"m\": {}, \"n\": {}, \"soluzioni\": {}, \"secondi\": {:.2}}}", m, n, s, t0.elapsed().as_secs_f64());
        }
        "verifica" => {
            let vinc: Vec<(usize, usize)> = a[4..].iter().map(|s| {
                let mut p = s.split(':');
                (p.next().unwrap().parse().unwrap(), p.next().unwrap().parse().unwrap())
            }).collect();
            let (k, sol) = verifica(&g, &vinc, 3);
            println!("{{\"m\": {}, \"n\": {}, \"indizi\": {:?}, \"soluzioni_fino_a_3\": {}, \"prima\": {:?}}}", m, n, vinc, k, sol);
        }
        "definenti" => {
            let sim = a.get(4).map(|s| s == "simmetrie").unwrap_or(false);
            let s = conta(&g);
            let e = definenti(&g, sim);
            let nn = g.nn;
            let mut esempi_txt = Vec::new();
            for &(x, y, z) in &e.esempi {
                let vinc: Vec<(usize, usize)> = [x, y, z].iter().map(|&it| (it / nn, it % nn + 1)).collect();
                let (k, sol) = verifica(&g, &vinc, 3);
                esempi_txt.push(format!("{{\"indizi\": [[{}, {}], [{}, {}], [{}, {}]], \"soluzioni_ricontate\": {}, \"soluzione\": {:?}}}",
                    vinc[0].0, vinc[0].1, vinc[1].0, vinc[1].1, vinc[2].0, vinc[2].1, k, sol));
            }
            let controllo = e.somma_gruppi == s * nn as u64;
            println!("{{\"m\": {}, \"n\": {}, \"modo\": \"{}\", \"soluzioni\": {}, \"gruppi_elaborati\": {}, \"somma_gruppi\": {}, \"controllo_somma_gruppi\": {}, \"definenti_1\": {}, \"definenti_2\": {}, \"definenti_3\": {}, \"resto_coppie\": {}, \"resto_terne\": {}, \"esempi_3\": [{}], \"secondi\": {:.2}}}",
                m, n, if sim { "simmetrie" } else { "completo" }, s, e.gruppi, e.somma_gruppi, controllo,
                e.singoli, e.coppie_inc / 2, e.terne_inc / 3, e.coppie_inc % 2, e.terne_inc % 3,
                esempi_txt.join(", "), t0.elapsed().as_secs_f64());
        }
        "salva" => {
            // tutte le soluzioni, N byte ciascuna (numero di ogni casella), nel file a[4]
            use std::io::Write;
            let f = std::fs::File::create(&a[4]).unwrap();
            let mut w = std::io::BufWriter::with_capacity(1 << 24, f);
            let mut k = 0u64;
            let partenze: Vec<usize> = (0..g.nn).collect();
            enumera(&g, &[], &partenze, |val| { w.write_all(val).unwrap(); k += 1; true });
            w.flush().unwrap();
            println!("{{\"m\": {}, \"n\": {}, \"soluzioni_salvate\": {}, \"secondi\": {:.2}}}", m, n, k, t0.elapsed().as_secs_f64());
        }
        "gruppo" => {
            // un solo elemento a = (c, v): quante soluzioni lo contengono e quanti insiemi definenti di 2 e 3 indizi
            // lo contengono, con fino a 10 esempi di terne
            let c: usize = a[4].parse().unwrap();
            let v: usize = a[5].parse().unwrap();
            let nn = g.nn;
            let ni = nn * nn;
            let mut c2 = vec![0u8; ni * ni];
            let mut c1 = vec![0u8; ni];
            let mut el = vec![0usize; nn];
            let mut quante = 0u64;
            let partenze: Vec<usize> = (0..nn).collect();
            enumera(&g, &[(c, v)], &partenze, |val| {
                quante += 1;
                let mut q = 0;
                for x in 0..nn {
                    if x == c { continue; }
                    el[q] = x * nn + (val[x] as usize - 1);
                    q += 1;
                }
                for i in 0..q {
                    let b = el[i];
                    if c1[b] < 2 { c1[b] += 1; }
                    for j in (i + 1)..q {
                        let p = &mut c2[b * ni + el[j]];
                        if *p < 2 { *p += 1; }
                    }
                }
                true
            });
            let coppie = c1.iter().filter(|&&x| x == 1).count();
            let mut terne = 0u64;
            let mut esempi = Vec::new();
            for b in 0..ni {
                for d in (b + 1)..ni {
                    if c2[b * ni + d] == 1 {
                        terne += 1;
                        if esempi.len() < 10 {
                            esempi.push(format!("[[{}, {}], [{}, {}], [{}, {}]]", c, v, b / nn, b % nn + 1, d / nn, d % nn + 1));
                        }
                    }
                }
            }
            println!("{{\"m\": {}, \"n\": {}, \"elemento\": [{}, {}], \"soluzioni_con_elemento\": {}, \"definenti_2_con_elemento\": {}, \"definenti_3_con_elemento\": {}, \"esempi\": [{}], \"secondi\": {:.2}}}",
                m, n, c, v, quante, coppie, terne, esempi.join(", "), t0.elapsed().as_secs_f64());
        }
        _ => panic!("modo sconosciuto"),
    }
}
