//! Programma B (Numbrix): controllo indipendente.
//!
//! Metodo (vedi LEGGIMI.md): ogni soluzione e' salvata come "riga" r[t] = casella che contiene il
//! numero t+1. Un insieme di 3 indizi definente, ordinato per numero (t1 < t2 < t3), si conta cosi':
//! per ogni coppia di POSIZIONI (t1, t2) si raggruppano tutte le soluzioni secondo la coppia di caselle
//! (r[t1], r[t2]) con un ordinamento per conteggio (counting sort); dentro ogni gruppo si conta, per
//! ogni t3 > t2, quante soluzioni hanno ciascuna casella in posizione t3. Le voci uguali a 1 sono le
//! terne definenti. I gruppi di dimensione 1 sono le coppie definenti.
//!
//! Solo libreria standard. Parallelismo con std::thread.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread;
use std::time::Instant;

const MAX_ESEMPI: usize = 5;
const ANTICIPO: usize = 8; // quante righe in anticipo si precaricano nella raccolta
const LUNGHEZZA_PREFISSI: usize = 7; // prefissi dei cammini usati per dividere l'enumerazione tra i fili

/// Griglie del cancello 1 (il 7x7 NON e' in lista: si lancia a parte con `griglia 7 7`).
const CANCELLO_1: [(usize, usize); 8] = [(1, 1), (2, 2), (3, 3), (3, 4), (4, 4), (4, 5), (5, 5), (6, 6)];

/// Valori noti (OEIS A096969) per le griglie n x n, n = 1..6.
const NOTI_NXN: [u64; 6] = [1, 8, 40, 552, 8648, 458696];

// ------------------------------------------------------------------------------------------------
// Griglia e maschere di bit
// ------------------------------------------------------------------------------------------------

#[derive(Clone)]
struct Griglia {
    n: usize,
    nn: usize,
    tutte: u64,
    no_ultima_col: u64,
    no_prima_col: u64,
    pari: u64,
    vicini: Vec<u64>,
}

#[inline(always)]
fn shr(a: u64, k: usize) -> u64 {
    if k >= 64 {
        0
    } else {
        a >> k
    }
}

#[inline(always)]
fn shl(a: u64, k: usize) -> u64 {
    if k >= 64 {
        0
    } else {
        a << k
    }
}

impl Griglia {
    fn nuova(m: usize, n: usize) -> Griglia {
        let nn = m * n;
        assert!((1..=64).contains(&nn), "servono da 1 a 64 caselle");
        let tutte = if nn == 64 { u64::MAX } else { (1u64 << nn) - 1 };
        let mut no_ultima_col = 0u64;
        let mut no_prima_col = 0u64;
        let mut pari = 0u64;
        let mut vicini = vec![0u64; nn];
        for r in 0..m {
            for c in 0..n {
                let k = r * n + c;
                if c + 1 < n {
                    no_ultima_col |= 1u64 << k;
                    vicini[k] |= 1u64 << (k + 1);
                }
                if c > 0 {
                    no_prima_col |= 1u64 << k;
                    vicini[k] |= 1u64 << (k - 1);
                }
                if r + 1 < m {
                    vicini[k] |= 1u64 << (k + n);
                }
                if r > 0 {
                    vicini[k] |= 1u64 << (k - n);
                }
                if (r + c) % 2 == 0 {
                    pari |= 1u64 << k;
                }
            }
        }
        Griglia { n, nn, tutte, no_ultima_col, no_prima_col, pari, vicini }
    }

    /// Per l'insieme di caselle `a`, restituisce quattro maschere: le caselle che hanno in `a`
    /// il vicino a destra, a sinistra, sotto, sopra.
    #[inline(always)]
    fn direzioni(&self, a: u64) -> [u64; 4] {
        [
            shr(a, 1) & self.no_ultima_col,
            shl(a, 1) & self.no_prima_col,
            shr(a, self.n),
            shl(a, self.n) & self.tutte,
        ]
    }
}

/// Condizioni NECESSARIE perche' un cammino con testa `testa` e caselle visitate `visitate`
/// si possa completare in un cammino hamiltoniano. Se una fallisce, si pota.
#[inline]
fn ammissibile(g: &Griglia, testa: usize, visitate: u64) -> bool {
    let u = g.tutte & !visitate;
    if u == 0 {
        return true;
    }
    // la testa deve toccare almeno una casella libera
    if g.vicini[testa] & u == 0 {
        return false;
    }
    // parita' (scacchiera): dopo la testa i colori si alternano partendo dall'altro colore
    let r = u.count_ones();
    let opposte = if (g.pari >> testa) & 1 == 1 { u & !g.pari } else { u & g.pari };
    if opposte.count_ones() != (r + 1) / 2 {
        return false;
    }
    // gradi: ogni casella libera ha bisogno di 2 vicini disponibili, tranne al massimo una (la fine)
    let a = u | (1u64 << testa);
    let [d1, d2, d3, d4] = g.direzioni(a);
    let almeno1 = d1 | d2 | d3 | d4;
    let almeno2 = (d1 & d2) | (d3 & d4) | ((d1 | d2) & (d3 | d4));
    if u & !almeno1 != 0 {
        return false;
    }
    if (u & !almeno2).count_ones() > 1 {
        return false;
    }
    // le caselle libere devono essere connesse
    let mut f = u & u.wrapping_neg();
    loop {
        let [e1, e2, e3, e4] = g.direzioni(f);
        let nf = (f | e1 | e2 | e3 | e4) & u;
        if nf == f {
            break;
        }
        f = nf;
    }
    f == u
}

// ------------------------------------------------------------------------------------------------
// Enumerazione delle soluzioni (cammini hamiltoniani orientati)
// ------------------------------------------------------------------------------------------------

fn dfs(g: &Griglia, testa: usize, visitate: u64, prof: usize, cammino: &mut [u8], out: &mut Vec<u8>) {
    if prof + 1 == g.nn {
        out.extend_from_slice(&cammino[..g.nn]);
        return;
    }
    let mut lib = g.vicini[testa] & !visitate;
    while lib != 0 {
        let nb = lib.trailing_zeros() as usize;
        lib &= lib - 1;
        let nv = visitate | (1u64 << nb);
        if ammissibile(g, nb, nv) {
            cammino[prof + 1] = nb as u8;
            dfs(g, nb, nv, prof + 1, cammino, out);
        }
    }
}

fn prefissi_rec(g: &Griglia, cam: &mut Vec<u8>, vis: u64, l: usize, res: &mut Vec<Vec<u8>>) {
    if cam.len() == l {
        res.push(cam.clone());
        return;
    }
    let testa = *cam.last().unwrap() as usize;
    let mut lib = g.vicini[testa] & !vis;
    while lib != 0 {
        let nb = lib.trailing_zeros() as usize;
        lib &= lib - 1;
        let nv = vis | (1u64 << nb);
        if ammissibile(g, nb, nv) {
            cam.push(nb as u8);
            prefissi_rec(g, cam, nv, l, res);
            cam.pop();
        }
    }
}

/// Tutte le soluzioni, in ordine lessicografico del cammino. Restituisce le righe concatenate:
/// riga s, posizione t = casella che contiene il numero t+1.
fn enumera(g: &Griglia, fili: usize) -> Vec<u8> {
    let nn = g.nn;
    let l = nn.min(LUNGHEZZA_PREFISSI);
    let mut pref = Vec::new();
    for s in 0..nn {
        let v = 1u64 << s;
        if ammissibile(g, s, v) {
            let mut cam = vec![s as u8];
            prefissi_rec(g, &mut cam, v, l, &mut pref);
        }
    }
    let prossimo = AtomicUsize::new(0);
    let mut parti: Vec<Vec<u8>> = vec![Vec::new(); pref.len()];
    thread::scope(|sc| {
        let pref = &pref;
        let prossimo = &prossimo;
        let mut hs = Vec::new();
        for _ in 0..fili.max(1) {
            hs.push(sc.spawn(move || {
                let mut mie: Vec<(usize, Vec<u8>)> = Vec::new();
                let mut cam = vec![0u8; nn];
                loop {
                    let i = prossimo.fetch_add(1, Ordering::Relaxed);
                    if i >= pref.len() {
                        break;
                    }
                    let p = &pref[i];
                    cam[..p.len()].copy_from_slice(p);
                    let vis = p.iter().fold(0u64, |a, &c| a | (1u64 << c));
                    let mut out = Vec::new();
                    dfs(g, *p.last().unwrap() as usize, vis, p.len() - 1, &mut cam, &mut out);
                    out.shrink_to_fit();
                    mie.push((i, out));
                }
                mie
            }));
        }
        for h in hs {
            for (i, out) in h.join().unwrap() {
                parti[i] = out;
            }
        }
    });
    let tot: usize = parti.iter().map(|p| p.len()).sum();
    let mut righe = Vec::with_capacity(tot);
    for p in parti.iter_mut() {
        righe.extend_from_slice(p);
        *p = Vec::new();
    }
    righe
}

/// Controllo indipendente da `ammissibile`: ogni riga e' una permutazione delle caselle, numeri
/// consecutivi in caselle vicine (con le coordinate), e righe in ordine strettamente crescente
/// (quindi tutte diverse).
fn controlla_soluzioni(g: &Griglia, righe: &[u8], fili: usize) -> bool {
    let nn = g.nn;
    let n = g.n;
    if righe.len() % nn != 0 {
        return false;
    }
    let s = righe.len() / nn;
    let fili = fili.max(1);
    let blocco = s.div_ceil(fili).max(1);
    let ok = AtomicBool::new(true);
    thread::scope(|sc| {
        let ok = &ok;
        for k in 0..fili {
            let lo = k * blocco;
            let hi = s.min(lo + blocco);
            if lo >= hi {
                continue;
            }
            sc.spawn(move || {
                for i in lo..hi {
                    let r = &righe[i * nn..(i + 1) * nn];
                    let mut visti = 0u64;
                    for &c in r {
                        let c = c as usize;
                        if c >= nn || (visti >> c) & 1 == 1 {
                            ok.store(false, Ordering::Relaxed);
                            return;
                        }
                        visti |= 1u64 << c;
                    }
                    for t in 0..nn.saturating_sub(1) {
                        let (a, b) = (r[t] as usize, r[t + 1] as usize);
                        let dr = (a / n).abs_diff(b / n);
                        let dc = (a % n).abs_diff(b % n);
                        if dr + dc != 1 {
                            ok.store(false, Ordering::Relaxed);
                            return;
                        }
                    }
                    if i > 0 && righe[(i - 1) * nn..i * nn] >= *r {
                        ok.store(false, Ordering::Relaxed);
                        return;
                    }
                }
            });
        }
    });
    ok.load(Ordering::Relaxed)
}

/// Trasposta: colonne[t][s] = righe[s][t]. Serve per leggere in sequenza le posizioni t1, t2.
fn colonne(righe: &[u8], nn: usize, fili: usize) -> Vec<Vec<u8>> {
    let s = righe.len() / nn;
    let mut col: Vec<Vec<u8>> = (0..nn).map(|_| vec![0u8; s]).collect();
    if s == 0 {
        return col;
    }
    let blocco = s.div_ceil(fili.max(1)).max(1);
    let nblocchi = s.div_ceil(blocco);
    let mut per_filo: Vec<Vec<&mut [u8]>> = Vec::new();
    per_filo.resize_with(nblocchi, Vec::new);
    for c in col.iter_mut() {
        for (k, ch) in c.chunks_mut(blocco).enumerate() {
            per_filo[k].push(ch);
        }
    }
    thread::scope(|sc| {
        for (k, mut fette) in per_filo.into_iter().enumerate() {
            sc.spawn(move || {
                let inizio = k * blocco;
                let len = fette[0].len();
                for i in 0..len {
                    let r = &righe[(inizio + i) * nn..(inizio + i + 1) * nn];
                    for (t, f) in fette.iter_mut().enumerate() {
                        f[i] = r[t];
                    }
                }
            });
        }
    });
    col
}

/// Numero -> casella in casella -> numero (0-based).
fn inversa(r: &[u8]) -> Vec<u8> {
    let mut g = vec![0u8; r.len()];
    for (t, &c) in r.iter().enumerate() {
        g[c as usize] = t as u8;
    }
    g
}

// ------------------------------------------------------------------------------------------------
// Conteggio degli insiemi definenti
// ------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
struct Esempio {
    indizi: [(usize, usize); 3], // (casella, t) con numero = t + 1
    sol: usize,                  // indice della soluzione unica
}

struct EsitoCoppia {
    t1: usize,
    t2: usize,
    d2: u64,
    d3: Vec<u64>, // per t3
    esempi: Vec<Esempio>,
}

struct Risultato {
    soluzioni: u64,
    d1: u64,
    d2: u64,
    d3: u64,
    d1_t: Vec<u64>,
    d2_tt: Vec<u64>,
    d3_ttt: Vec<u64>,
    esempi: Vec<Esempio>,
}

#[inline(always)]
fn precarica(righe: &[u8], da: usize, lung: usize) {
    #[cfg(target_arch = "x86_64")]
    {
        let p = righe.as_ptr().wrapping_add(da) as *const i8;
        let q = righe.as_ptr().wrapping_add(da + lung.saturating_sub(1)) as *const i8;
        #[allow(unused_unsafe)]
        unsafe {
            std::arch::x86_64::_mm_prefetch::<{ std::arch::x86_64::_MM_HINT_T0 }>(p);
            std::arch::x86_64::_mm_prefetch::<{ std::arch::x86_64::_MM_HINT_T0 }>(q);
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        let _ = (righe, da, lung);
    }
}

/// Lavoro per una coppia di posizioni (t1 < t2).
#[allow(clippy::too_many_arguments)]
fn lavora_coppia(
    nn: usize,
    righe: &[u8],
    a: &[u8],
    b: &[u8],
    t1: usize,
    t2: usize,
    indici: &mut [u32],
    inizio: &mut [u32],
    scrivi: &mut [u32],
    cnt: &mut [u32],
) -> EsitoCoppia {
    let s = a.len();
    let q = nn * nn;
    // 1) counting sort delle soluzioni per chiave (r[t1], r[t2])
    inizio.fill(0);
    for i in 0..s {
        inizio[a[i] as usize * nn + b[i] as usize + 1] += 1;
    }
    for k in 0..q {
        inizio[k + 1] += inizio[k];
    }
    scrivi.copy_from_slice(&inizio[..q]);
    for i in 0..s {
        let k = a[i] as usize * nn + b[i] as usize;
        indici[scrivi[k] as usize] = i as u32;
        scrivi[k] += 1;
    }
    // 2) gruppo per gruppo
    let primo = t2 + 1;
    let larg = nn - primo;
    let mut d2 = 0u64;
    let mut d3 = vec![0u64; nn];
    let mut esempi = Vec::new();
    for k in 0..q {
        let lo = inizio[k] as usize;
        let hi = inizio[k + 1] as usize;
        if lo == hi {
            continue;
        }
        let (c1, c2) = (k / nn, k % nn);
        let gruppo = &indici[lo..hi];
        if hi - lo == 1 {
            // coppia definente: anche ogni terna che la contiene e' definente
            d2 += 1;
            let si = gruppo[0] as usize;
            for (t3, d) in d3.iter_mut().enumerate().skip(primo) {
                *d += 1;
                if esempi.len() < MAX_ESEMPI {
                    esempi.push(Esempio { indizi: [(c1, t1), (c2, t2), (righe[si * nn + t3] as usize, t3)], sol: si });
                }
            }
            continue;
        }
        if larg == 0 {
            continue;
        }
        let cc = &mut cnt[primo * nn..];
        for j in 0..gruppo.len() {
            if j + ANTICIPO < gruppo.len() {
                precarica(righe, gruppo[j + ANTICIPO] as usize * nn + primo, larg);
            }
            let base = gruppo[j] as usize * nn + primo;
            let coda = &righe[base..base + larg];
            for (o, &c3) in coda.iter().enumerate() {
                cc[o * nn + c3 as usize] += 1;
            }
        }
        for o in 0..larg {
            let t3 = primo + o;
            let fila = &mut cc[o * nn..(o + 1) * nn];
            for (c3, v) in fila.iter_mut().enumerate() {
                if *v == 1 {
                    d3[t3] += 1;
                    if esempi.len() < MAX_ESEMPI {
                        let si = gruppo
                            .iter()
                            .map(|&x| x as usize)
                            .find(|&x| righe[x * nn + t3] as usize == c3)
                            .unwrap();
                        esempi.push(Esempio { indizi: [(c1, t1), (c2, t2), (c3, t3)], sol: si });
                    }
                }
                *v = 0;
            }
        }
    }
    EsitoCoppia { t1, t2, d2, d3, esempi }
}

fn conta(nn: usize, righe: &[u8], col: &[Vec<u8>], fili: usize, progressi: bool) -> Risultato {
    let s = righe.len() / nn;
    assert!(s < u32::MAX as usize);
    // 1 indizio: elementi (casella, numero) presenti in esattamente una soluzione
    let mut d1_t = vec![0u64; nn];
    let mut ist = vec![0u64; nn];
    for t in 0..nn {
        ist.fill(0);
        for &c in col[t].iter() {
            ist[c as usize] += 1;
        }
        d1_t[t] = ist.iter().filter(|&&x| x == 1).count() as u64;
    }
    // 2 e 3 indizi: compiti = coppie di posizioni (t1 < t2), le piu' pesanti (t2 piccolo) prima
    let mut compiti = Vec::new();
    for t2 in 1..nn {
        for t1 in 0..t2 {
            compiti.push((t1, t2));
        }
    }
    let q = nn * nn;
    let prossimo = AtomicUsize::new(0);
    let fatti = AtomicUsize::new(0);
    let t0 = Instant::now();
    let mut esiti: Vec<EsitoCoppia> = Vec::with_capacity(compiti.len());
    thread::scope(|sc| {
        let compiti = &compiti;
        let prossimo = &prossimo;
        let fatti = &fatti;
        let t0 = &t0;
        let mut hs = Vec::new();
        for _ in 0..fili.max(1) {
            hs.push(sc.spawn(move || {
                let mut miei: Vec<EsitoCoppia> = Vec::new();
                let mut indici: Vec<u32> = Vec::new();
                let mut inizio = vec![0u32; q + 1];
                let mut scrivi = vec![0u32; q];
                let mut cnt = vec![0u32; q];
                loop {
                    let id = prossimo.fetch_add(1, Ordering::Relaxed);
                    if id >= compiti.len() {
                        break;
                    }
                    if indici.len() != s {
                        indici = vec![0u32; s];
                    }
                    let (t1, t2) = compiti[id];
                    miei.push(lavora_coppia(
                        nn, righe, &col[t1], &col[t2], t1, t2, &mut indici, &mut inizio, &mut scrivi, &mut cnt,
                    ));
                    let f = fatti.fetch_add(1, Ordering::Relaxed) + 1;
                    let tot = compiti.len();
                    if progressi && f * 20 / tot != (f - 1) * 20 / tot {
                        eprintln!(
                            "  coppie di posizioni: {}/{} ({:.0}%), {:.1} s",
                            f,
                            tot,
                            100.0 * f as f64 / tot as f64,
                            t0.elapsed().as_secs_f64()
                        );
                    }
                }
                miei
            }));
        }
        for h in hs {
            esiti.extend(h.join().unwrap());
        }
    });
    esiti.sort_by_key(|e| (e.t1, e.t2));
    let mut d2_tt = vec![0u64; q];
    let mut d3_ttt = vec![0u64; q * nn];
    let mut esempi = Vec::new();
    for e in &esiti {
        d2_tt[e.t1 * nn + e.t2] = e.d2;
        for t3 in 0..nn {
            d3_ttt[(e.t1 * nn + e.t2) * nn + t3] = e.d3[t3];
        }
        for x in &e.esempi {
            if esempi.len() < MAX_ESEMPI {
                esempi.push(*x);
            }
        }
    }
    Risultato {
        soluzioni: s as u64,
        d1: d1_t.iter().sum(),
        d2: d2_tt.iter().sum(),
        d3: d3_ttt.iter().sum(),
        d1_t,
        d2_tt,
        d3_ttt,
        esempi,
    }
}

/// Simmetria del rovescio: se una soluzione e' valida lo e' anche il cammino al contrario
/// (numero v -> N+1-v). Quindi i conteggi per posizioni devono coincidere con quelli delle
/// posizioni rovesciate. Le due meta' vengono da compiti diversi del calcolo: e' un controllo vero.
fn controlla_rovescio(nn: usize, r: &Risultato) -> bool {
    let rv = |t: usize| nn - 1 - t;
    for t in 0..nn {
        if r.d1_t[t] != r.d1_t[rv(t)] {
            return false;
        }
    }
    for t1 in 0..nn {
        for t2 in t1 + 1..nn {
            if r.d2_tt[t1 * nn + t2] != r.d2_tt[rv(t2) * nn + rv(t1)] {
                return false;
            }
            for t3 in t2 + 1..nn {
                if r.d3_ttt[(t1 * nn + t2) * nn + t3] != r.d3_ttt[(rv(t3) * nn + rv(t2)) * nn + rv(t1)] {
                    return false;
                }
            }
        }
    }
    true
}

/// Ogni esempio: caselle diverse, numeri diversi, ed esattamente una soluzione (quella indicata)
/// li contiene tutti, cercando su tutte le soluzioni.
fn controlla_esempi(nn: usize, righe: &[u8], esempi: &[Esempio]) -> bool {
    let s = righe.len() / nn;
    for e in esempi {
        let [(c1, t1), (c2, t2), (c3, t3)] = e.indizi;
        if c1 == c2 || c1 == c3 || c2 == c3 || t1 == t2 || t1 == t3 || t2 == t3 {
            return false;
        }
        let trovate: Vec<usize> = (0..s)
            .filter(|&si| e.indizi.iter().all(|&(c, t)| righe[si * nn + t] as usize == c))
            .collect();
        if trovate != vec![e.sol] {
            return false;
        }
    }
    true
}

// ------------------------------------------------------------------------------------------------
// Forza bruta per le griglie piccole (indipendente dal metodo principale)
// ------------------------------------------------------------------------------------------------

fn valida_griglia(m: usize, n: usize, gr: &[u8]) -> bool {
    let nn = m * n;
    let mut pos = vec![usize::MAX; nn];
    for (c, &v) in gr.iter().enumerate() {
        let v = v as usize;
        if v >= nn || pos[v] != usize::MAX {
            return false;
        }
        pos[v] = c;
    }
    (0..nn.saturating_sub(1)).all(|k| {
        let (a, b) = (pos[k], pos[k + 1]);
        (a / n).abs_diff(b / n) + (a % n).abs_diff(b % n) == 1
    })
}

/// Tutte le permutazioni casella -> numero (algoritmo di Heap), tenendo quelle valide. Solo N <= 9.
fn permutazioni(m: usize, n: usize) -> Vec<Vec<u8>> {
    let nn = m * n;
    assert!(nn <= 9);
    let mut a: Vec<u8> = (0..nn as u8).collect();
    let mut res = Vec::new();
    if valida_griglia(m, n, &a) {
        res.push(a.clone());
    }
    let mut c = vec![0usize; nn];
    let mut i = 1;
    while i < nn {
        if c[i] < i {
            if i % 2 == 0 {
                a.swap(0, i);
            } else {
                a.swap(c[i], i);
            }
            if valida_griglia(m, n, &a) {
                res.push(a.clone());
            }
            c[i] += 1;
            i = 1;
        } else {
            c[i] = 0;
            i += 1;
        }
    }
    res
}

/// Ricerca semplice senza potature, con le coordinate. Restituisce griglie casella -> numero.
fn dfs_semplice(m: usize, n: usize) -> Vec<Vec<u8>> {
    #[allow(clippy::too_many_arguments)]
    fn rec(m: usize, n: usize, r: usize, c: usize, t: usize, gr: &mut Vec<u8>, res: &mut Vec<Vec<u8>>) {
        gr[r * n + c] = t as u8;
        if t + 1 == m * n {
            res.push(gr.clone());
        } else {
            let mosse: [(isize, isize); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];
            for (dr, dc) in mosse {
                let r2 = r as isize + dr;
                let c2 = c as isize + dc;
                if r2 >= 0 && c2 >= 0 && (r2 as usize) < m && (c2 as usize) < n && gr[r2 as usize * n + c2 as usize] == u8::MAX {
                    rec(m, n, r2 as usize, c2 as usize, t + 1, gr, res);
                }
            }
        }
        gr[r * n + c] = u8::MAX;
    }
    let mut gr = vec![u8::MAX; m * n];
    let mut res = Vec::new();
    for r in 0..m {
        for c in 0..n {
            rec(m, n, r, c, 0, &mut gr, &mut res);
        }
    }
    res
}

/// Conta gli insiemi definenti di 1, 2, 3 indizi passando TUTTI gli insiemi possibili di elementi
/// con caselle diverse e numeri diversi (caselle in ordine crescente, numeri liberi), e contando
/// per ciascuno le soluzioni compatibili con insiemi di bit.
fn bruto(nn: usize, griglie: &[Vec<u8>], fili: usize) -> (u64, u64, u64) {
    let s = griglie.len();
    let w = s.div_ceil(64).max(1);
    let ne = nn * nn;
    let mut bs = vec![0u64; ne * w];
    for (si, gr) in griglie.iter().enumerate() {
        for c in 0..nn {
            let e = c * nn + gr[c] as usize;
            bs[e * w + si / 64] |= 1u64 << (si % 64);
        }
    }
    let bs = &bs;
    let el = move |e: usize| &bs[e * w..(e + 1) * w];
    let d1 = (0..ne).filter(|&e| el(e).iter().map(|x| x.count_ones()).sum::<u32>() == 1).count() as u64;
    let prossimo = AtomicUsize::new(0);
    let mut d2 = 0u64;
    let mut d3 = 0u64;
    thread::scope(|sc| {
        let prossimo = &prossimo;
        let mut hs = Vec::new();
        for _ in 0..fili.max(1) {
            hs.push(sc.spawn(move || {
                let mut a2 = 0u64;
                let mut a3 = 0u64;
                let mut tmp = vec![0u64; w];
                loop {
                    let c1 = prossimo.fetch_add(1, Ordering::Relaxed);
                    if c1 >= nn {
                        break;
                    }
                    for t1 in 0..nn {
                        let e1 = el(c1 * nn + t1);
                        for c2 in c1 + 1..nn {
                            for t2 in 0..nn {
                                if t2 == t1 {
                                    continue;
                                }
                                let e2 = el(c2 * nn + t2);
                                let mut p = 0u32;
                                for k in 0..w {
                                    tmp[k] = e1[k] & e2[k];
                                    p += tmp[k].count_ones();
                                }
                                if p == 1 {
                                    a2 += 1;
                                }
                                if p == 0 {
                                    continue;
                                }
                                for c3 in c2 + 1..nn {
                                    for t3 in 0..nn {
                                        if t3 == t1 || t3 == t2 {
                                            continue;
                                        }
                                        let e3 = el(c3 * nn + t3);
                                        let mut p3 = 0u32;
                                        for k in 0..w {
                                            p3 += (tmp[k] & e3[k]).count_ones();
                                        }
                                        if p3 == 1 {
                                            a3 += 1;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                (a2, a3)
            }));
        }
        for h in hs {
            let (x, y) = h.join().unwrap();
            d2 += x;
            d3 += y;
        }
    });
    (d1, d2, d3)
}

/// Confronto completo su una griglia piccola. Restituisce true se tutto torna.
fn verifica_griglia(m: usize, n: usize, fili: usize) -> bool {
    let g = Griglia::nuova(m, n);
    let nn = g.nn;
    let righe = enumera(&g, fili);
    let ok_valide = controlla_soluzioni(&g, &righe, fili);
    let mut indip = dfs_semplice(m, n);
    indip.sort();
    let ok_perm = if nn <= 9 {
        let mut p = permutazioni(m, n);
        p.sort();
        p == indip
    } else {
        true
    };
    let mut mie: Vec<Vec<u8>> = righe.chunks(nn).map(inversa).collect();
    mie.sort();
    let ok_insieme = mie == indip;
    let col = colonne(&righe, nn, fili);
    let r = conta(nn, &righe, &col, fili, false);
    let (b1, b2, b3) = bruto(nn, &indip, fili);
    let ok_conti = (r.d1, r.d2, r.d3) == (b1, b2, b3);
    let ok_rov = controlla_rovescio(nn, &r);
    let ok_es = controlla_esempi(nn, &righe, &r.esempi);
    let tutto = ok_valide && ok_perm && ok_insieme && ok_conti && ok_rov && ok_es;
    println!(
        "{}x{}: soluzioni B={} indip={} | B: d1={} d2={} d3={} | forza bruta: d1={} d2={} d3={} | valide={} perm={} stesso_insieme={} conti={} rovescio={} esempi={} => {}",
        m,
        n,
        r.soluzioni,
        indip.len(),
        r.d1,
        r.d2,
        r.d3,
        b1,
        b2,
        b3,
        ok_valide,
        if nn <= 9 { if ok_perm { "ok" } else { "NO" } } else { "-" },
        ok_insieme,
        ok_conti,
        ok_rov,
        ok_es,
        if tutto { "OK" } else { "ERRORE" }
    );
    tutto
}

fn verifica(fili: usize) -> bool {
    let griglie = [
        (1, 1),
        (1, 2),
        (2, 1),
        (1, 5),
        (2, 2),
        (2, 3),
        (3, 2),
        (3, 3),
        (2, 5),
        (3, 4),
        (4, 3),
        (4, 4),
        (4, 5),
        (5, 4),
        (5, 5),
    ];
    let mut ok = true;
    for &(m, n) in &griglie {
        ok &= verifica_griglia(m, n, fili);
    }
    for k in 1..=6 {
        let g = Griglia::nuova(k, k);
        let righe = enumera(&g, fili);
        let s = (righe.len() / (k * k)) as u64;
        let buono = s == NOTI_NXN[k - 1] && controlla_soluzioni(&g, &righe, fili);
        println!("{}x{}: soluzioni {} (atteso {}) => {}", k, k, s, NOTI_NXN[k - 1], if buono { "OK" } else { "ERRORE" });
        ok &= buono;
    }
    println!("VERIFICA COMPLESSIVA: {}", if ok { "OK" } else { "ERRORE" });
    ok
}

// ------------------------------------------------------------------------------------------------
// Esecuzione su una griglia e scrittura del JSON
// ------------------------------------------------------------------------------------------------

fn json(m: usize, n: usize, r: &Risultato, righe: &[u8], secondi: f64) -> String {
    let nn = m * n;
    let mut o = String::new();
    o.push_str("{\n");
    o.push_str(&format!("  \"m\": {},\n", m));
    o.push_str(&format!("  \"n\": {},\n", n));
    o.push_str(&format!("  \"soluzioni\": {},\n", r.soluzioni));
    o.push_str(&format!("  \"definenti_1\": {},\n", r.d1));
    o.push_str(&format!("  \"definenti_2\": {},\n", r.d2));
    o.push_str(&format!("  \"definenti_3\": {},\n", r.d3));
    o.push_str("  \"esempi_3\": [");
    for (i, e) in r.esempi.iter().enumerate() {
        if i > 0 {
            o.push(',');
        }
        let ind: Vec<String> = e.indizi.iter().map(|&(c, t)| format!("[{}, {}]", c, t + 1)).collect();
        let gr = inversa(&righe[e.sol * nn..(e.sol + 1) * nn]);
        let sol: Vec<String> = gr.iter().map(|&t| (t as usize + 1).to_string()).collect();
        o.push_str(&format!(
            "\n    {{\"indizi\": [{}], \"soluzione\": [{}]}}",
            ind.join(", "),
            sol.join(", ")
        ));
    }
    if !r.esempi.is_empty() {
        o.push_str("\n  ");
    }
    o.push_str("],\n");
    o.push_str(&format!("  \"secondi\": {:.3}\n", secondi));
    o.push_str("}\n");
    o
}

fn esegui(m: usize, n: usize, fili: usize, uscita: &Path, scrivi_file: bool) -> bool {
    let t0 = Instant::now();
    let g = Griglia::nuova(m, n);
    let nn = g.nn;
    eprintln!("== {}x{} ({} caselle), {} fili", m, n, nn, fili);
    let righe = enumera(&g, fili);
    let s = righe.len() / nn;
    eprintln!("  enumerazione: {} soluzioni in {:.2} s", s, t0.elapsed().as_secs_f64());
    let grande = s >= 1_000_000;
    if grande {
        let mb = |x: usize| x as f64 / 1e6;
        eprintln!(
            "  memoria stimata: righe {:.0} MB + colonne {:.0} MB + indici {:.0} MB",
            mb(s * nn),
            mb(s * nn),
            mb(s * 4 * fili)
        );
    }
    let ok_valide = controlla_soluzioni(&g, &righe, fili);
    let ok_noto = if m == n && m <= 6 { s as u64 == NOTI_NXN[m - 1] } else { true };
    let col = colonne(&righe, nn, fili);
    let t1 = Instant::now();
    let r = conta(nn, &righe, &col, fili, grande);
    eprintln!("  conteggio: {:.2} s", t1.elapsed().as_secs_f64());
    drop(col);
    let ok_rov = controlla_rovescio(nn, &r);
    let ok_es = controlla_esempi(nn, &righe, &r.esempi);
    let secondi = t0.elapsed().as_secs_f64();
    let ok = ok_valide && ok_noto && ok_rov && ok_es;
    println!(
        "{}x{}: soluzioni={} definenti_1={} definenti_2={} definenti_3={} esempi={} secondi={:.3} | controlli: valide={} valore_noto={} rovescio={} esempi={} => {}",
        m,
        n,
        r.soluzioni,
        r.d1,
        r.d2,
        r.d3,
        r.esempi.len(),
        secondi,
        ok_valide,
        if m == n && m <= 6 { if ok_noto { "ok" } else { "NO" } } else { "-" },
        ok_rov,
        ok_es,
        if ok { "OK" } else { "ERRORE" }
    );
    if !ok {
        eprintln!("  un controllo e' fallito: il JSON NON viene scritto");
        return false;
    }
    if scrivi_file {
        fs::create_dir_all(uscita).expect("impossibile creare la cartella di uscita");
        let p = uscita.join(format!("b_{}x{}.json", m, n));
        fs::write(&p, json(m, n, &r, &righe, secondi)).expect("impossibile scrivere il JSON");
        eprintln!("  scritto {}", p.display());
    }
    true
}

fn uso() {
    eprintln!(
        "uso:\n  numbrix_b griglia M N [--fili K] [--uscita CARTELLA] [--senza-file]\n  numbrix_b cancello1 [--fili K]\n  numbrix_b verifica [--fili K]"
    );
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut fili = thread::available_parallelism().map(|x| x.get()).unwrap_or(4);
    // strumenti/scoperta/numbrix/b  ->  strumenti/risultati/numbrix/b
    let mut uscita = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("cartella strumenti non trovata")
        .join("risultati")
        .join("numbrix")
        .join("b");
    let mut scrivi_file = true;
    let mut pos: Vec<String> = Vec::new();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--fili" => {
                fili = args[i + 1].parse().expect("--fili vuole un numero");
                i += 2;
            }
            "--uscita" => {
                uscita = PathBuf::from(&args[i + 1]);
                i += 2;
            }
            "--senza-file" => {
                scrivi_file = false;
                i += 1;
            }
            x => {
                pos.push(x.to_string());
                i += 1;
            }
        }
    }
    let ok = match pos.first().map(|s| s.as_str()) {
        Some("griglia") if pos.len() == 3 => {
            let m: usize = pos[1].parse().expect("M non valido");
            let n: usize = pos[2].parse().expect("N non valido");
            esegui(m, n, fili, &uscita, scrivi_file)
        }
        Some("cancello1") => {
            let mut ok = true;
            for &(m, n) in CANCELLO_1.iter() {
                ok &= esegui(m, n, fili, &uscita, scrivi_file);
            }
            ok
        }
        Some("verifica") => verifica(fili),
        _ => {
            uso();
            false
        }
    };
    std::process::exit(if ok { 0 } else { 2 });
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn forza_bruta_griglie_piccole() {
        for &(m, n) in &[(1, 1), (1, 3), (2, 2), (2, 3), (3, 3), (3, 4)] {
            assert!(verifica_griglia(m, n, 4), "{}x{}", m, n);
        }
    }

    #[test]
    fn valori_noti() {
        for k in 1..=5 {
            let g = Griglia::nuova(k, k);
            assert_eq!((enumera(&g, 4).len() / (k * k)) as u64, NOTI_NXN[k - 1]);
        }
    }
}
