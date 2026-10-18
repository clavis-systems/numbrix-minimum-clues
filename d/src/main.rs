//! Programma D (aggiunta 1 al piano): controllo indipendente degli insiemi definenti di 1 e 2 indizi.
//!
//! Metodo diverso da A:
//! - un generatore di soluzioni proprio, con array (niente maschere di bit) e una visita in ampiezza con coda per il
//!   collegamento;
//! - una sola passata su tutte le soluzioni; per ogni coppia di caselle (c1 < c2) e coppia di numeri (v1, v2) un
//!   contatore saturato a 2, più un contatore per ogni (casella, numero);
//! - insiemi definenti di 2 = contatori di coppia uguali a 1; di 1 = contatori singoli uguali a 1.
//! Lavoro diviso fra i thread per prefissi del percorso (casella iniziale e prime due mosse).
//! Uso: numbrix_d M N   (JSON su stdout)
use std::env;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::Instant;

struct G {
    m: usize,
    n: usize,
    nn: usize,
    vic: Vec<Vec<usize>>,
    np: usize,
    indice_coppia: Vec<usize>, // c1*nn + c2 -> numero della coppia (solo c1 < c2)
}

struct Stato<'a> {
    g: &'a G,
    usato: Vec<bool>,
    val: Vec<u8>,
    coda: Vec<usize>,
    segno: Vec<u32>,
    turno: u32,
    soluzioni: u64,
    uno: Vec<u8>,
    due: Vec<u8>,
}

impl<'a> Stato<'a> {
    fn nuovo(g: &'a G) -> Stato<'a> {
        Stato {
            g,
            usato: vec![false; g.nn],
            val: vec![0; g.nn],
            coda: vec![0; g.nn],
            segno: vec![0; g.nn],
            turno: 0,
            soluzioni: 0,
            uno: vec![0; g.nn * g.nn],
            due: vec![0; g.np * g.nn * g.nn],
        }
    }

    /// Le caselle libere, più la testa, possono ancora formare un percorso che parte dalla testa?
    fn possibile(&mut self, testa: usize, liberi: usize) -> bool {
        if liberi == 0 { return true; }
        let g = self.g;
        let mut con_un_vicino = 0;
        for u in 0..g.nn {
            if self.usato[u] { continue; }
            let mut d = 0;
            for &w in &g.vic[u] {
                if !self.usato[w] || w == testa { d += 1; }
            }
            if d == 0 { return false; }
            if d == 1 {
                con_un_vicino += 1;
                if con_un_vicino > 1 { return false; }
            }
        }
        // collegamento: visita in ampiezza dalla testa attraverso le caselle libere
        self.turno += 1;
        let t = self.turno;
        let (mut a, mut b) = (0usize, 0usize);
        self.coda[b] = testa; b += 1;
        self.segno[testa] = t;
        let mut raggiunte = 0;
        while a < b {
            let u = self.coda[a]; a += 1;
            for &w in &g.vic[u] {
                if !self.usato[w] && self.segno[w] != t {
                    self.segno[w] = t;
                    self.coda[b] = w; b += 1;
                    raggiunte += 1;
                }
            }
        }
        raggiunte == liberi
    }

    fn registra(&mut self) {
        let g = self.g;
        let nn = g.nn;
        self.soluzioni += 1;
        for c in 0..nn {
            let i = c * nn + (self.val[c] as usize - 1);
            if self.uno[i] < 2 { self.uno[i] += 1; }
        }
        for c1 in 0..nn {
            let v1 = self.val[c1] as usize - 1;
            for c2 in (c1 + 1)..nn {
                let p = g.indice_coppia[c1 * nn + c2];
                let i = (p * nn + v1) * nn + (self.val[c2] as usize - 1);
                if self.due[i] < 2 { self.due[i] += 1; }
            }
        }
    }

    fn avanti(&mut self, testa: usize, k: usize, liberi: usize) {
        if k == self.g.nn {
            self.registra();
            return;
        }
        let g = self.g;
        for idx in 0..g.vic[testa].len() {
            let x = g.vic[testa][idx];
            if self.usato[x] { continue; }
            self.usato[x] = true;
            self.val[x] = (k + 1) as u8;
            if self.possibile(x, liberi - 1) {
                self.avanti(x, k + 1, liberi - 1);
            }
            self.usato[x] = false;
            self.val[x] = 0;
        }
    }
}

fn main() {
    let a: Vec<String> = env::args().collect();
    let m: usize = a[1].parse().unwrap();
    let n: usize = a[2].parse().unwrap();
    let nn = m * n;
    let mut vic = vec![Vec::new(); nn];
    for r in 0..m {
        for c in 0..n {
            let i = r * n + c;
            if c + 1 < n { vic[i].push(i + 1); }
            if r + 1 < m { vic[i].push(i + n); }
            if c > 0 { vic[i].push(i - 1); }
            if r > 0 { vic[i].push(i - n); }
        }
    }
    let mut indice_coppia = vec![usize::MAX; nn * nn];
    let mut np = 0;
    for c1 in 0..nn {
        for c2 in (c1 + 1)..nn {
            indice_coppia[c1 * nn + c2] = np;
            np += 1;
        }
    }
    let g = G { m, n, nn, vic, np, indice_coppia };
    let t0 = Instant::now();

    // prefissi: (partenza, prima mossa, seconda mossa) oppure percorsi più corti se la griglia è piccola
    let mut prefissi: Vec<Vec<usize>> = Vec::new();
    for s in 0..nn {
        if nn == 1 { prefissi.push(vec![s]); continue; }
        for &x in &g.vic[s] {
            if nn == 2 { prefissi.push(vec![s, x]); continue; }
            for &y in &g.vic[x] {
                if y != s { prefissi.push(vec![s, x, y]); }
            }
        }
    }
    let prossimo = AtomicUsize::new(0);
    let totale = Mutex::new((0u64, vec![0u8; nn * nn], vec![0u8; g.np * nn * nn]));
    let fili = thread::available_parallelism().map(|x| x.get()).unwrap_or(4);
    thread::scope(|sc| {
        for _ in 0..fili {
            sc.spawn(|| {
                let mut st = Stato::nuovo(&g);
                loop {
                    let k = prossimo.fetch_add(1, Ordering::Relaxed);
                    if k >= prefissi.len() { break; }
                    let p = &prefissi[k];
                    let mut ok = true;
                    for (i, &x) in p.iter().enumerate() {
                        st.usato[x] = true;
                        st.val[x] = (i + 1) as u8;
                    }
                    let liberi = nn - p.len();
                    let testa = *p.last().unwrap();
                    if !st.possibile(testa, liberi) { ok = false; }
                    // anche i prefissi intermedi devono essere ammissibili (stesse regole del passo per passo)
                    if ok {
                        st.avanti(testa, p.len(), liberi);
                    }
                    for &x in p { st.usato[x] = false; st.val[x] = 0; }
                }
                let mut t = totale.lock().unwrap();
                t.0 += st.soluzioni;
                for (dst, src) in t.1.iter_mut().zip(st.uno.iter()) { *dst = (*dst + *src).min(2); }
                for (dst, src) in t.2.iter_mut().zip(st.due.iter()) { *dst = (*dst + *src).min(2); }
            });
        }
    });
    let (sol, uno, due) = totale.into_inner().unwrap();
    let def1 = uno.iter().filter(|&&x| x == 1).count();
    let def2 = due.iter().filter(|&&x| x == 1).count();
    let mut esempi = Vec::new();
    'fuori: for c1 in 0..nn {
        for c2 in (c1 + 1)..nn {
            let p = g.indice_coppia[c1 * nn + c2];
            for v1 in 0..nn {
                for v2 in 0..nn {
                    if due[(p * nn + v1) * nn + v2] == 1 {
                        esempi.push(format!("[[{}, {}], [{}, {}]]", c1, v1 + 1, c2, v2 + 1));
                        if esempi.len() >= 5 { break 'fuori; }
                    }
                }
            }
        }
    }
    println!("{{\"m\": {}, \"n\": {}, \"soluzioni\": {}, \"definenti_1\": {}, \"definenti_2\": {}, \"esempi_2\": [{}], \"secondi\": {:.2}}}",
        g.m, g.n, sol, def1, def2, esempi.join(", "), t0.elapsed().as_secs_f64());
}
