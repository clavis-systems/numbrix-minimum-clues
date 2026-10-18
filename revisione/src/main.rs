// Ricontrollo indipendente Numbrix. Solo libreria standard.
// Metodo principale: per OGNI insieme di indizi, ricerca in profondita' ancorata agli indizi
// (si parte dall'indizio di valore minore e si estende l'intervallo [lo,hi] dei numeri gia' posati),
// con potatura su (a) distanza tra indizi, (b) componenti connesse delle caselle libere,
// (c) vicoli ciechi; la ricerca si ferma a 2 soluzioni. Non si elencano mai le soluzioni 7x7.
use std::collections::HashSet;
use std::env;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::Instant;

// ---------------------------------------------------------------- memoria di picco (Windows)
#[repr(C)]
struct Pmc {
    cb: u32,
    page_fault_count: u32,
    peak_working_set: usize,
    working_set: usize,
    quota_peak_paged: usize,
    quota_paged: usize,
    quota_peak_nonpaged: usize,
    quota_nonpaged: usize,
    pagefile: usize,
    peak_pagefile: usize,
}
extern "system" {
    fn GetCurrentProcess() -> isize;
    fn K32GetProcessMemoryInfo(h: isize, p: *mut Pmc, cb: u32) -> i32;
}
fn peak_mb() -> f64 {
    unsafe {
        let mut p: Pmc = std::mem::zeroed();
        p.cb = std::mem::size_of::<Pmc>() as u32;
        if K32GetProcessMemoryInfo(GetCurrentProcess(), &mut p, p.cb) != 0 {
            p.peak_working_set as f64 / 1048576.0
        } else {
            -1.0
        }
    }
}

// ---------------------------------------------------------------- griglia
#[derive(Clone)]
struct Grid {
    h: usize,
    w: usize,
    n: usize,
    full: u64,
    not_c0: u64,
    not_cl: u64,
    nbr: Vec<u64>,
    adj: Vec<Vec<usize>>,
    dist: Vec<u8>, // n*n
    color: Vec<u8>,
    perms: Vec<Vec<usize>>, // gruppo di simmetrie geometriche (permutazioni di caselle)
}

impl Grid {
    fn new(h: usize, w: usize) -> Grid {
        let n = h * w;
        assert!(n <= 64 && n >= 2);
        let full = if n == 64 { !0u64 } else { (1u64 << n) - 1 };
        let mut not_c0 = 0u64;
        let mut not_cl = 0u64;
        let mut nbr = vec![0u64; n];
        let mut adj = vec![Vec::new(); n];
        let mut color = vec![0u8; n];
        let mut dist = vec![0u8; n * n];
        for r in 0..h {
            for c in 0..w {
                let i = r * w + c;
                if c != 0 {
                    not_c0 |= 1 << i;
                }
                if c != w - 1 {
                    not_cl |= 1 << i;
                }
                color[i] = ((r + c) & 1) as u8;
                if r > 0 {
                    nbr[i] |= 1 << (i - w);
                    adj[i].push(i - w);
                }
                if r + 1 < h {
                    nbr[i] |= 1 << (i + w);
                    adj[i].push(i + w);
                }
                if c > 0 {
                    nbr[i] |= 1 << (i - 1);
                    adj[i].push(i - 1);
                }
                if c + 1 < w {
                    nbr[i] |= 1 << (i + 1);
                    adj[i].push(i + 1);
                }
            }
        }
        for a in 0..n {
            for b in 0..n {
                let (ra, ca) = ((a / w) as i32, (a % w) as i32);
                let (rb, cb) = ((b / w) as i32, (b % w) as i32);
                dist[a * n + b] = ((ra - rb).abs() + (ca - cb).abs()) as u8;
            }
        }
        // simmetrie
        let ident: Vec<usize> = (0..n).collect();
        let flip_r: Vec<usize> = (0..n).map(|i| (h - 1 - i / w) * w + i % w).collect();
        let flip_c: Vec<usize> = (0..n).map(|i| (i / w) * w + (w - 1 - i % w)).collect();
        let mut gens = vec![flip_r, flip_c];
        if h == w {
            gens.push((0..n).map(|i| (i % w) * w + i / w).collect());
        }
        let mut set: Vec<Vec<usize>> = vec![ident];
        loop {
            let mut added = false;
            let cur = set.clone();
            for p in &cur {
                for g in &gens {
                    let q: Vec<usize> = (0..n).map(|i| g[p[i]]).collect();
                    if !set.contains(&q) {
                        set.push(q);
                        added = true;
                    }
                }
            }
            if !added {
                break;
            }
        }
        Grid { h, w, n, full, not_c0, not_cl, nbr, adj, dist, color, perms: set }
    }

    #[inline(always)]
    fn expand(&self, x: u64) -> u64 {
        (((x << 1) & self.not_c0) | ((x >> 1) & self.not_cl) | (x << self.w) | (x >> self.w)) & self.full
    }
    #[inline(always)]
    fn flood(&self, seed: u64, f: u64) -> u64 {
        let mut x = seed;
        loop {
            let nx = (x | self.expand(x)) & f;
            if nx == x {
                return x;
            }
            x = nx;
        }
    }
    // parita' (colore + valore); per n dispari serve 1 (colore maggioritario = 0)
    fn par(&self, c: usize, v: usize) -> usize {
        (self.color[c] as usize + v) & 1
    }
}

// ---------------------------------------------------------------- ricerca ancorata agli indizi
struct Search<'a> {
    g: &'a Grid,
    k: usize,
    cv: [usize; 8],
    cc: [usize; 8],
    clue_mask: u64,
    val_cell: [i8; 70],
    next_j: [i8; 70],
    vmax: usize,
    cap: u64,
    count: u64,
    nodes: u64,
    vals: [u8; 64],
    keep: usize,
    kept: Vec<[u8; 64]>,
    mode: u8,
    watch: Vec<([(u8, u8); 3], u8)>,
    wcount: Vec<u64>,
}

impl<'a> Search<'a> {
    // true = si puo' continuare
    #[inline]
    fn prune(&self, lo: usize, hi: usize, cl: usize, ch: usize, occ: u64) -> bool {
        let g = self.g;
        let f = g.full & !occ;
        let l = lo - 1;
        let r = g.n - hi;
        if l + r == 0 {
            return true;
        }
        let hb_lo = if l > 0 { g.nbr[cl] & f } else { 0 };
        let hb_hi = if r > 0 { g.nbr[ch] & f } else { 0 };
        if (l > 0 && hb_lo == 0) || (r > 0 && hb_hi == 0) {
            return false;
        }
        // vicoli ciechi: caselle libere con meno di 2 vicini possibili devono essere i capi (1 o n)
        let mut gs = f;
        if l > 0 {
            gs |= 1u64 << cl;
        }
        if r > 0 {
            gs |= 1u64 << ch;
        }
        let a = (gs << 1) & g.not_c0;
        let b = (gs >> 1) & g.not_cl;
        let c = (gs << g.w) & g.full;
        let d = gs >> g.w;
        let ge2 = (a & b) | (a & c) | (a & d) | (b & c) | (b & d) | (c & d);
        let ends = (f & !ge2).count_ones();
        if ends > (l > 0) as u32 + (r > 0) as u32 {
            return false;
        }
        let k1 = g.flood(f & f.wrapping_neg(), f);
        if k1 != f {
            let rest = f & !k1;
            let k2 = g.flood(rest & rest.wrapping_neg(), rest);
            if k2 != rest {
                return false; // 3 o piu' componenti
            }
            if !(l > 0 && r > 0) {
                return false;
            }
            let s1 = k1.count_ones() as usize;
            let s2 = k2.count_ones() as usize;
            let ok_a = (hb_lo & k1) != 0 && (hb_hi & k2) != 0 && s1 == l && s2 == r;
            let ok_b = (hb_lo & k2) != 0 && (hb_hi & k1) != 0 && s2 == l && s1 == r;
            if !(ok_a || ok_b) {
                return false;
            }
        }
        true
    }

    // ordine "fallisci per primo": a ogni nodo si estende il lato (basso o alto) con meno candidati.
    // La scelta dipende solo dallo stato, quindi ogni soluzione e' generata esattamente una volta.
    fn rec2(&mut self, lo: usize, hi: usize, cl: usize, ch: usize, occ: u64) {
        if self.count >= self.cap {
            return;
        }
        self.nodes += 1;
        let g = self.g;
        let n = g.n;
        if lo == 1 && hi == n {
            self.count += 1;
            if self.kept.len() < self.keep {
                self.kept.push(self.vals);
            }
            for (wi, (ws, wl)) in self.watch.iter().enumerate() {
                let mut ok = true;
                for q in 0..(*wl as usize) {
                    if self.vals[ws[q].0 as usize] != ws[q].1 {
                        ok = false;
                        break;
                    }
                }
                if ok {
                    self.wcount[wi] += 1;
                }
            }
            return;
        }
        let mut up = 0u64;
        let mut x = 0usize;
        if hi < n {
            x = hi + 1;
            let mut cand = g.nbr[ch] & !occ;
            let vc = self.val_cell[x];
            if vc >= 0 {
                cand &= 1u64 << (vc as u32);
            } else {
                cand &= !self.clue_mask;
            }
            let j = self.next_j[x];
            if j >= 0 {
                let jj = j as usize;
                let mut t = cand;
                let mut km = 0u64;
                while t != 0 {
                    let u = t.trailing_zeros() as usize;
                    t &= t - 1;
                    if g.dist[u * n + self.cc[jj]] as usize <= self.cv[jj] - x {
                        km |= 1u64 << u;
                    }
                }
                cand = km;
            }
            up = cand;
        }
        let mut dn = 0u64;
        if lo > 1 {
            dn = g.nbr[cl] & !occ & !self.clue_mask;
        }
        let go_up;
        if hi == n {
            go_up = false;
        } else if lo == 1 {
            go_up = true;
        } else {
            let nu = up.count_ones();
            let nd = dn.count_ones();
            if nu == 0 || nd == 0 {
                return;
            }
            go_up = nu <= nd;
        }
        if go_up {
            let mut cand = up;
            while cand != 0 {
                let u = cand.trailing_zeros() as usize;
                cand &= cand - 1;
                let occ2 = occ | (1u64 << u);
                if self.prune(lo, x, cl, u, occ2) {
                    self.vals[u] = x as u8;
                    self.rec2(lo, x, cl, u, occ2);
                    if self.count >= self.cap {
                        return;
                    }
                }
            }
        } else {
            let y = lo - 1;
            let mut cand = dn;
            while cand != 0 {
                let u = cand.trailing_zeros() as usize;
                cand &= cand - 1;
                let occ2 = occ | (1u64 << u);
                if self.prune(y, hi, u, ch, occ2) {
                    self.vals[u] = y as u8;
                    self.rec2(y, hi, u, ch, occ2);
                    if self.count >= self.cap {
                        return;
                    }
                }
            }
        }
    }

    fn rec(&mut self, lo: usize, hi: usize, cl: usize, ch: usize, occ: u64) {
        if self.count >= self.cap {
            return;
        }
        self.nodes += 1;
        let g = self.g;
        if lo == 1 && hi == g.n {
            self.count += 1;
            if self.kept.len() < self.keep {
                self.kept.push(self.vals);
            }
            return;
        }
        if hi < self.vmax || lo == 1 {
            let x = hi + 1;
            let mut cand = g.nbr[ch] & !occ;
            let vc = self.val_cell[x];
            if vc >= 0 {
                cand &= 1u64 << (vc as u32);
            } else {
                cand &= !self.clue_mask;
            }
            let j = self.next_j[x];
            while cand != 0 {
                let u = cand.trailing_zeros() as usize;
                cand &= cand - 1;
                if j >= 0 {
                    let jj = j as usize;
                    if g.dist[u * g.n + self.cc[jj]] as usize > self.cv[jj] - x {
                        continue;
                    }
                }
                let occ2 = occ | (1u64 << u);
                if self.prune(lo, x, cl, u, occ2) {
                    self.vals[u] = x as u8;
                    self.rec(lo, x, cl, u, occ2);
                    if self.count >= self.cap {
                        return;
                    }
                }
            }
        } else {
            let x = lo - 1;
            let mut cand = g.nbr[cl] & !occ & !self.clue_mask;
            while cand != 0 {
                let u = cand.trailing_zeros() as usize;
                cand &= cand - 1;
                let occ2 = occ | (1u64 << u);
                if self.prune(x, hi, u, ch, occ2) {
                    self.vals[u] = x as u8;
                    self.rec(x, hi, u, ch, occ2);
                    if self.count >= self.cap {
                        return;
                    }
                }
            }
        }
    }
}

// clues: (casella, valore). Restituisce (conteggio fino a cap, nodi, soluzioni tenute)
fn solve(g: &Grid, clues: &[(usize, usize)], cap: u64, keep: usize) -> (u64, u64, Vec<[u8; 64]>) {
    let n = g.n;
    let mut cl: Vec<(usize, usize)> = clues.iter().map(|&(c, v)| (v, c)).collect();
    cl.sort();
    let k = cl.len();
    assert!(k >= 1 && k <= 8);
    for i in 0..k {
        let (v, c) = cl[i];
        if v < 1 || v > n || c >= n {
            return (0, 0, vec![]);
        }
        if i > 0 && (cl[i - 1].0 == v || cl[i - 1].1 == c) {
            return (0, 0, vec![]); // stesso valore o stessa casella
        }
    }
    let p0 = g.par(cl[0].1, cl[0].0);
    for i in 0..k {
        if g.par(cl[i].1, cl[i].0) != p0 {
            return (0, 0, vec![]);
        }
    }
    if n % 2 == 1 && p0 != 1 {
        return (0, 0, vec![]);
    }
    for i in 0..k - 1 {
        if g.dist[cl[i].1 * n + cl[i + 1].1] as usize > cl[i + 1].0 - cl[i].0 {
            return (0, 0, vec![]);
        }
    }
    let mut s = Search {
        g,
        k,
        cv: [0; 8],
        cc: [0; 8],
        clue_mask: 0,
        val_cell: [-1; 70],
        next_j: [-1; 70],
        vmax: cl[k - 1].0,
        cap,
        count: 0,
        nodes: 0,
        vals: [0; 64],
        keep,
        kept: Vec::new(),
        mode: env_mode(),
        watch: Vec::new(),
        wcount: Vec::new(),
    };
    for i in 0..k {
        s.cv[i] = cl[i].0;
        s.cc[i] = cl[i].1;
        s.clue_mask |= 1u64 << cl[i].1;
        s.val_cell[cl[i].0] = cl[i].1 as i8;
    }
    for x in 1..=n {
        for i in 0..k {
            if s.cv[i] > x {
                s.next_j[x] = i as i8;
                break;
            }
        }
    }
    let (v1, c1) = cl[0];
    let occ = 1u64 << c1;
    s.vals[c1] = v1 as u8;
    if s.prune(v1, v1, c1, c1, occ) {
        if s.mode == 0 {
            s.rec(v1, v1, c1, c1, occ);
        } else {
            s.rec2(v1, v1, c1, c1, occ);
        }
    }
    (s.count, s.nodes, s.kept)
}

fn env_mode() -> u8 {
    use std::sync::OnceLock;
    static M: OnceLock<u8> = OnceLock::new();
    *M.get_or_init(|| env::var("MODE").ok().and_then(|x| x.parse().ok()).unwrap_or(1))
}

// come solve ma con osservatori: conta, nel flusso di TUTTE le soluzioni trovate con gli indizi dati, quante contengono ciascun insieme osservato
fn solve_watch(g: &Grid, clues: &[(usize, usize)], watch: &Vec<([(u8, u8); 3], u8)>) -> (u64, Vec<u64>) {
    let n = g.n;
    let mut cl: Vec<(usize, usize)> = clues.iter().map(|&(c, v)| (v, c)).collect();
    cl.sort();
    let k = cl.len();
    let p0 = g.par(cl[0].1, cl[0].0);
    if n % 2 == 1 && p0 != 1 {
        return (0, vec![0; watch.len()]);
    }
    let mut s = Search {
        g,
        k,
        cv: [0; 8],
        cc: [0; 8],
        clue_mask: 0,
        val_cell: [-1; 70],
        next_j: [-1; 70],
        vmax: cl[k - 1].0,
        cap: u64::MAX,
        count: 0,
        nodes: 0,
        vals: [0; 64],
        keep: 0,
        kept: Vec::new(),
        mode: 1,
        watch: watch.clone(),
        wcount: vec![0; watch.len()],
    };
    for i in 0..k {
        s.cv[i] = cl[i].0;
        s.cc[i] = cl[i].1;
        s.clue_mask |= 1u64 << cl[i].1;
        s.val_cell[cl[i].0] = cl[i].1 as i8;
    }
    for x in 1..=n {
        for i in 0..k {
            if s.cv[i] > x {
                s.next_j[x] = i as i8;
                break;
            }
        }
    }
    let (v1, c1) = cl[0];
    let occ = 1u64 << c1;
    s.vals[c1] = v1 as u8;
    if s.prune(v1, v1, c1, c1, occ) {
        s.rec2(v1, v1, c1, c1, occ);
    }
    (s.count, s.wcount)
}

// flusso di tutte le soluzioni con un enumeratore INDIPENDENTE dalla ricerca ancorata:
// parte da 1 e sale; unica potatura = le caselle libere devono restare un solo blocco connesso
struct FS<'a> {
    g: &'a Grid,
    vals: [u8; 64],
    count: u64,
    watch: &'a Vec<([(u8, u8); 3], u8)>,
    wc: Vec<u64>,
    pt: Option<&'a [u8]>,
    bad: u64,
    ones: u64,
    lookups: u64,
    period: u64,
    samples: Vec<[u8; 64]>,
}
fn fs_rec(s: &mut FS, x: usize, cell: usize, occ: u64) {
    let g = s.g;
    let n = g.n;
    if x == n {
        s.count += 1;
        if s.period > 0 && s.count % s.period == 0 && s.samples.len() < 100000 {
            s.samples.push(s.vals);
        }
        if let Some(pt) = s.pt {
            let mut pos = [0usize; 66];
            for c in 0..n {
                pos[s.vals[c] as usize] = c;
            }
            let ssz = n * (n + 1);
            for a in 1..=n {
                let o1 = pos[a] * (n + 1) + a;
                for b in (a + 1)..=n {
                    let o2 = pos[b] * (n + 1) + b;
                    let r = pt[o1 * ssz + o2];
                    s.lookups += 1;
                    if r == 0 {
                        s.bad += 1;
                    } else if r == 1 {
                        s.ones += 1;
                    }
                }
            }
        }
        for (wi, (ws, wl)) in s.watch.iter().enumerate() {
            let mut ok = true;
            for q in 0..(*wl as usize) {
                if s.vals[ws[q].0 as usize] != ws[q].1 {
                    ok = false;
                    break;
                }
            }
            if ok {
                s.wc[wi] += 1;
            }
        }
        return;
    }
    let mut cand = g.nbr[cell] & !occ;
    while cand != 0 {
        let u = cand.trailing_zeros() as usize;
        cand &= cand - 1;
        let occ2 = occ | (1u64 << u);
        if x + 1 < n {
            let f = g.full & !occ2;
            let seed = g.nbr[u] & f;
            if seed == 0 {
                continue;
            }
            let k = g.flood(seed & seed.wrapping_neg(), f);
            if k != f {
                continue;
            }
        }
        s.vals[u] = (x + 1) as u8;
        fs_rec(s, x + 1, u, occ2);
    }
}
fn fwd_stream(g: &Grid, start: usize, watch: &Vec<([(u8, u8); 3], u8)>) -> (u64, Vec<u64>) {
    let mut s = FS { g, vals: [0; 64], count: 0, watch, wc: vec![0; watch.len()], pt: None, bad: 0, ones: 0, lookups: 0, period: 0, samples: vec![] };
    s.vals[start] = 1;
    fs_rec(&mut s, 1, start, 1u64 << start);
    (s.count, s.wc)
}
// (conteggio, coppie con esito 0 nella tabella, coppie con esito 1, ricerche in tabella, campione di soluzioni)
fn fwd_stream_pt(g: &Grid, start: usize, pt: &[u8], period: u64) -> (u64, u64, u64, u64, Vec<[u8; 64]>) {
    let empty: Vec<([(u8, u8); 3], u8)> = Vec::new();
    let mut s = FS { g, vals: [0; 64], count: 0, watch: &empty, wc: vec![], pt: Some(pt), bad: 0, ones: 0, lookups: 0, period, samples: vec![] };
    s.vals[start] = 1;
    fs_rec(&mut s, 1, start, 1u64 << start);
    (s.count, s.bad, s.ones, s.lookups, s.samples)
}

fn verify_solution(g: &Grid, vals: &[u8; 64], clues: &[(usize, usize)]) -> bool {
    let n = g.n;
    let mut pos = vec![usize::MAX; n + 1];
    for c in 0..n {
        let v = vals[c] as usize;
        if v < 1 || v > n || pos[v] != usize::MAX {
            return false;
        }
        pos[v] = c;
    }
    for v in 1..n {
        if g.dist[pos[v] * n + pos[v + 1]] != 1 {
            return false;
        }
    }
    for &(c, v) in clues {
        if vals[c] as usize != v {
            return false;
        }
    }
    true
}

// ---------------------------------------------------------------- secondo risolutore (in avanti da 1), indipendente
struct Fwd<'a> {
    g: &'a Grid,
    clue_cell: Vec<i32>, // per valore
    clue_val: Vec<i32>,  // per casella
    visited: Vec<bool>,
    val: Vec<u8>,
    nxt_val: Vec<usize>, // per valore x: prossimo valore-indizio > x (0 se nessuno)
    count: u64,
    cap: u64,
    sols: Vec<Vec<u8>>,
    nodes: u64,
}
impl<'a> Fwd<'a> {
    fn connected_all(&self, from: usize, x: usize) -> bool {
        // tutte le caselle non visitate devono essere raggiungibili da 'from' (la testa) attraverso non visitate
        let n = self.g.n;
        let remaining = n - x;
        if remaining == 0 {
            return true;
        }
        let mut seen = vec![false; n];
        let mut stack = vec![];
        let mut cnt = 0;
        for &u in &self.g.adj[from] {
            if !self.visited[u] && !seen[u] {
                seen[u] = true;
                stack.push(u);
                cnt += 1;
            }
        }
        while let Some(c) = stack.pop() {
            for &u in &self.g.adj[c] {
                if !self.visited[u] && !seen[u] {
                    seen[u] = true;
                    stack.push(u);
                    cnt += 1;
                }
            }
        }
        cnt == remaining
    }
    fn rec(&mut self, x: usize, cell: usize) {
        if self.count >= self.cap {
            return;
        }
        self.nodes += 1;
        let n = self.g.n;
        if x == n {
            self.count += 1;
            if self.sols.len() < 4 {
                self.sols.push(self.val.clone());
            }
            return;
        }
        let y = x + 1;
        let nb = self.g.adj[cell].clone();
        for u in nb {
            if self.visited[u] {
                continue;
            }
            if self.clue_cell[y] >= 0 {
                if self.clue_cell[y] as usize != u {
                    continue;
                }
            } else if self.clue_val[u] >= 0 {
                continue;
            }
            let nv = self.nxt_val[y];
            if nv != 0 {
                let cc = self.clue_cell[nv] as usize;
                if self.g.dist[u * n + cc] as usize > nv - y {
                    continue;
                }
            }
            self.visited[u] = true;
            self.val[u] = y as u8;
            if self.connected_all(u, y) {
                self.rec(y, u);
            }
            self.visited[u] = false;
            if self.count >= self.cap {
                return;
            }
        }
    }
}
fn fwd_count(g: &Grid, clues: &[(usize, usize)], cap: u64) -> (u64, u64, Vec<Vec<u8>>) {
    let n = g.n;
    let mut f = Fwd {
        g,
        clue_cell: vec![-1; n + 2],
        clue_val: vec![-1; n],
        visited: vec![false; n],
        val: vec![0; n],
        nxt_val: vec![0; n + 2],
        count: 0,
        cap,
        sols: vec![],
        nodes: 0,
    };
    for &(c, v) in clues {
        if f.clue_cell[v] >= 0 && f.clue_cell[v] != c as i32 {
            return (0, 0, vec![]);
        }
        if f.clue_val[c] >= 0 && f.clue_val[c] != v as i32 {
            return (0, 0, vec![]);
        }
        f.clue_cell[v] = c as i32;
        f.clue_val[c] = v as i32;
    }
    for x in 0..=n {
        for y in (x + 1)..=n {
            if f.clue_cell[y] >= 0 {
                f.nxt_val[x] = y;
                break;
            }
        }
    }
    let starts: Vec<usize> = if f.clue_cell[1] >= 0 { vec![f.clue_cell[1] as usize] } else { (0..n).filter(|&c| f.clue_val[c] < 0).collect() };
    for s in starts {
        f.visited[s] = true;
        f.val[s] = 1;
        let nv = f.nxt_val[1];
        let ok = if nv != 0 { g.dist[s * n + f.clue_cell[nv] as usize] as usize <= nv - 1 } else { true };
        if ok && f.connected_all(s, 1) {
            f.rec(1, s);
        }
        f.visited[s] = false;
    }
    (f.count, f.nodes, f.sols)
}

// ---------------------------------------------------------------- forza bruta pura (senza potature)
struct Brute<'a> {
    adj: &'a Vec<Vec<usize>>,
    n: usize,
    vis: Vec<bool>,
    val: Vec<u8>,
    out: Vec<u8>,
    count: u64,
}
impl<'a> Brute<'a> {
    fn rec(&mut self, x: usize, cell: usize) {
        if x == self.n {
            self.out.extend_from_slice(&self.val);
            self.count += 1;
            return;
        }
        let adj = self.adj;
        for &u in &adj[cell] {
            if !self.vis[u] {
                self.vis[u] = true;
                self.val[u] = (x + 1) as u8;
                self.rec(x + 1, u);
                self.vis[u] = false;
            }
        }
    }
}
// restituisce vettore piatto: sols[s*n + casella] = numero
fn brute_all(g: &Grid, nt: usize) -> Vec<u8> {
    let n = g.n;
    let next = AtomicUsize::new(0);
    let parts: Mutex<Vec<(usize, Vec<u8>)>> = Mutex::new(vec![]);
    thread::scope(|sc| {
        for _ in 0..nt {
            sc.spawn(|| loop {
                let s = next.fetch_add(1, Ordering::SeqCst);
                if s >= n {
                    break;
                }
                let mut b = Brute { adj: &g.adj, n, vis: vec![false; n], val: vec![0; n], out: vec![], count: 0 };
                b.vis[s] = true;
                b.val[s] = 1;
                b.rec(1, s);
                parts.lock().unwrap().push((s, b.out));
            });
        }
    });
    let mut p = parts.into_inner().unwrap();
    p.sort_by_key(|x| x.0);
    let mut all = vec![];
    for (_, o) in p {
        all.extend(o);
    }
    all
}

// statistiche per forza bruta "un contatore per valori fissati" (solo per griglie piccole)
fn brute_stats(g: &Grid, sols: &[u8], nt: usize, print_pairs: bool) {
    let n = g.n;
    let ns = sols.len() / n;
    let mut pos = vec![0u8; ns * (n + 1)];
    for s in 0..ns {
        for c in 0..n {
            pos[s * (n + 1) + sols[s * n + c] as usize] = c as u8;
        }
    }
    // k=1
    let mut u1 = 0u64;
    for v in 1..=n {
        let mut cnt = vec![0u32; n];
        for s in 0..ns {
            cnt[pos[s * (n + 1) + v] as usize] += 1;
        }
        u1 += cnt.iter().filter(|&&x| x == 1).count() as u64;
    }
    println!("brute: soluzioni={} ; insiemi da 1 indizio con soluzione unica={}", ns, u1);
    // k=2 : tabelle per riuso
    let np = (n + 1) * (n + 1);
    let pair_tabs: Mutex<Vec<Vec<u32>>> = Mutex::new(vec![vec![]; np]);
    let next = AtomicUsize::new(1);
    let u2tot = AtomicU64::new(0);
    let uniq_pairs: Mutex<Vec<(usize, usize, usize, usize)>> = Mutex::new(vec![]);
    thread::scope(|sc| {
        for _ in 0..nt {
            sc.spawn(|| loop {
                let v1 = next.fetch_add(1, Ordering::SeqCst);
                if v1 >= n {
                    break;
                }
                for v2 in (v1 + 1)..=n {
                    let mut cnt = vec![0u32; n * n];
                    for s in 0..ns {
                        let a = pos[s * (n + 1) + v1] as usize;
                        let b = pos[s * (n + 1) + v2] as usize;
                        cnt[a * n + b] += 1;
                    }
                    for (i, &x) in cnt.iter().enumerate() {
                        if x == 1 {
                            u2tot.fetch_add(1, Ordering::SeqCst);
                            uniq_pairs.lock().unwrap().push((i / n, v1, i % n, v2));
                        }
                    }
                    pair_tabs.lock().unwrap()[v1 * (n + 1) + v2] = cnt;
                }
            });
        }
    });
    let pair_tabs = pair_tabs.into_inner().unwrap();
    println!("brute: insiemi da 2 indizi con soluzione unica={}", u2tot.load(Ordering::SeqCst));
    if print_pairs {
        let mut up = uniq_pairs.into_inner().unwrap();
        up.sort();
        for (c1, v1, c2, v2) in up {
            println!("  coppia unica: ({},{}) ({},{})", c1, v1, c2, v2);
        }
    }
    if n <= 40 {
        // k=3
        let next = AtomicUsize::new(1);
        let u3 = AtomicU64::new(0);
        let u3min = AtomicU64::new(0);
        thread::scope(|sc| {
            for _ in 0..nt {
                sc.spawn(|| {
                    let mut cnt = vec![0u32; n * n * n];
                    loop {
                        let v1 = next.fetch_add(1, Ordering::SeqCst);
                        if v1 >= n {
                            break;
                        }
                        for v2 in (v1 + 1)..n {
                            for v3 in (v2 + 1)..=n {
                                for x in cnt.iter_mut() {
                                    *x = 0;
                                }
                                for s in 0..ns {
                                    let a = pos[s * (n + 1) + v1] as usize;
                                    let b = pos[s * (n + 1) + v2] as usize;
                                    let c = pos[s * (n + 1) + v3] as usize;
                                    cnt[(a * n + b) * n + c] += 1;
                                }
                                for (i, &x) in cnt.iter().enumerate() {
                                    if x == 1 {
                                        u3.fetch_add(1, Ordering::SeqCst);
                                        let c3 = i % n;
                                        let c2 = (i / n) % n;
                                        let c1 = i / (n * n);
                                        let p12 = pair_tabs[v1 * (n + 1) + v2][c1 * n + c2];
                                        let p13 = pair_tabs[v1 * (n + 1) + v3][c1 * n + c3];
                                        let p23 = pair_tabs[v2 * (n + 1) + v3][c2 * n + c3];
                                        if p12 != 1 && p13 != 1 && p23 != 1 {
                                            u3min.fetch_add(1, Ordering::SeqCst);
                                        }
                                    }
                                }
                            }
                        }
                    }
                });
            }
        });
        println!(
            "brute: insiemi da 3 indizi con soluzione unica={} (di cui minimali, cioe' senza nessuna coppia gia' unica={})",
            u3.load(Ordering::SeqCst),
            u3min.load(Ordering::SeqCst)
        );
    }
}

// ---------------------------------------------------------------- coppie e terne con ricerca ancorata
fn opt_id(g: &Grid, c: usize, v: usize) -> usize {
    c * (g.n + 1) + v
}

struct PairStats {
    attempts: u64,
    n0: u64,
    n1: u64,
    n2: u64,
    nodes: u64,
}

// restituisce la tabella pt[(o1)*S+o2] in {0,1,2} (0 = nessuna soluzione, 1 = una, 2 = almeno due)
fn run_pairs(g: &Grid, nt: usize, print_ones: bool) -> (Vec<u8>, PairStats) {
    let n = g.n;
    let s_sz = n * (n + 1);
    let mut items = vec![];
    for v1 in 1..n {
        for c1 in 0..n {
            if n % 2 == 1 && g.par(c1, v1) != 1 {
                continue;
            }
            items.push((v1, c1));
        }
    }
    let next = AtomicUsize::new(0);
    let entries: Mutex<Vec<(u32, u32, u8)>> = Mutex::new(vec![]);
    let attempts = AtomicU64::new(0);
    let c0 = AtomicU64::new(0);
    let c1a = AtomicU64::new(0);
    let c2a = AtomicU64::new(0);
    let nodes = AtomicU64::new(0);
    let ones: Mutex<Vec<(usize, usize, usize, usize)>> = Mutex::new(vec![]);
    thread::scope(|sc| {
        for _ in 0..nt {
            sc.spawn(|| {
                let mut local: Vec<(u32, u32, u8)> = vec![];
                loop {
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    if i >= items.len() {
                        break;
                    }
                    let (v1, c1) = items[i];
                    let p1 = g.par(c1, v1);
                    for v2 in (v1 + 1)..=n {
                        for c2 in 0..n {
                            if c2 == c1 || g.par(c2, v2) != p1 {
                                continue;
                            }
                            if g.dist[c1 * n + c2] as usize > v2 - v1 {
                                continue;
                            }
                            let (cnt, nd, _) = solve(g, &[(c1, v1), (c2, v2)], 2, 0);
                            attempts.fetch_add(1, Ordering::Relaxed);
                            nodes.fetch_add(nd, Ordering::Relaxed);
                            match cnt {
                                0 => {
                                    c0.fetch_add(1, Ordering::Relaxed);
                                }
                                1 => {
                                    c1a.fetch_add(1, Ordering::Relaxed);
                                    ones.lock().unwrap().push((c1, v1, c2, v2));
                                }
                                _ => {
                                    c2a.fetch_add(1, Ordering::Relaxed);
                                }
                            }
                            if cnt > 0 {
                                local.push((opt_id(g, c1, v1) as u32, opt_id(g, c2, v2) as u32, cnt as u8));
                            }
                        }
                    }
                }
                entries.lock().unwrap().extend(local);
            });
        }
    });
    let mut pt = vec![0u8; s_sz * s_sz];
    let entries = entries.into_inner().unwrap();
    for &(a, b, r) in &entries {
        pt[a as usize * s_sz + b as usize] = r;
        pt[b as usize * s_sz + a as usize] = r;
    }
    // autocontrollo di simmetria: l'immagine di ogni coppia sotto le 2*|G| trasformazioni ha lo stesso esito
    let mut bad = 0u64;
    for &(a, b, r) in &entries {
        let (ca, va) = (a as usize / (n + 1), a as usize % (n + 1));
        let (cb, vb) = (b as usize / (n + 1), b as usize % (n + 1));
        for p in &g.perms {
            for rev in [false, true] {
                let (va2, vb2) = if rev { (n + 1 - va, n + 1 - vb) } else { (va, vb) };
                let a2 = opt_id(g, p[ca], va2);
                let b2 = opt_id(g, p[cb], vb2);
                if pt[a2 * s_sz + b2] != r {
                    bad += 1;
                }
            }
        }
    }
    println!("pairs: autocontrollo simmetria (16 o 8 o 4 trasformazioni) - esiti discordanti = {}", bad);
    if print_ones {
        let mut o = ones.into_inner().unwrap();
        o.sort();
        println!("pairs: elenco delle coppie con esattamente 1 soluzione ({}):", o.len());
        for (c1, v1, c2, v2) in o.iter().take(100) {
            println!("   ({},{}) ({},{})", c1, v1, c2, v2);
        }
    }
    let st = PairStats {
        attempts: attempts.load(Ordering::SeqCst),
        n0: c0.load(Ordering::SeqCst),
        n1: c1a.load(Ordering::SeqCst),
        n2: c2a.load(Ordering::SeqCst),
        nodes: nodes.load(Ordering::SeqCst),
    };
    (pt, st)
}

fn transforms(g: &Grid) -> Vec<(usize, bool)> {
    let mut t = vec![];
    for pi in 0..g.perms.len() {
        for rev in [false, true] {
            t.push((pi, rev));
        }
    }
    t
}

// chiave dell'opzione: valore*64 + casella
fn img_triple(g: &Grid, tr: (usize, bool), t: &[(usize, usize); 3]) -> [u32; 3] {
    let n = g.n;
    let mut o = [0u32; 3];
    for i in 0..3 {
        let (c, v) = t[i];
        let c2 = g.perms[tr.0][c];
        let v2 = if tr.1 { n + 1 - v } else { v };
        o[i] = (v2 * 64 + c2) as u32;
    }
    o.sort();
    o
}

fn is_canonical(g: &Grid, trs: &[(usize, bool)], t: &[(usize, usize); 3]) -> bool {
    let orig = img_triple(g, (0, false), t);
    for &tr in trs.iter() {
        let im = img_triple(g, tr, t);
        if im < orig {
            return false;
        }
    }
    true
}
fn orbit_size(g: &Grid, trs: &[(usize, bool)], t: &[(usize, usize); 3]) -> u64 {
    let mut set: HashSet<[u32; 3]> = HashSet::new();
    for &tr in trs.iter() {
        set.insert(img_triple(g, tr, t));
    }
    set.len() as u64
}

fn run_triples(g: &Grid, pt: &[u8], sym: bool, nt: usize, sample: usize) {
    let n = g.n;
    let s_sz = n * (n + 1);
    let trs = transforms(g);
    // primo elemento (v1,c1,v2)
    let mut items = vec![];
    for v1 in 1..(n - 1) {
        for c1 in 0..n {
            if n % 2 == 1 && g.par(c1, v1) != 1 {
                continue;
            }
            for v2 in (v1 + 1)..n {
                items.push((v1, c1, v2));
            }
        }
    }
    let total_items = items.len();
    let next = AtomicUsize::new(0);
    let tot_w = AtomicU64::new(0); // pesato (numero di insiemi, con orbite)
    let tot_min_w = AtomicU64::new(0);
    let cand = AtomicU64::new(0); // terne che superano il filtro delle coppie
    let canon = AtomicU64::new(0);
    let n0 = AtomicU64::new(0);
    let n1 = AtomicU64::new(0);
    let n2 = AtomicU64::new(0);
    let nodes = AtomicU64::new(0);
    let nodes_by = [AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0)];
    let examples: Mutex<Vec<[(usize, usize); 3]>> = Mutex::new(vec![]);
    let dump: Mutex<Vec<(u64, [(usize, usize); 3])>> = Mutex::new(vec![]);
    let started = Instant::now();
    thread::scope(|sc| {
        for _ in 0..nt {
            sc.spawn(|| loop {
                let ii = next.fetch_add(1, Ordering::SeqCst);
                if ii >= total_items {
                    break;
                }
                if sample > 1 && ii % sample != 0 {
                    continue;
                }
                let (v1, c1, v2) = items[ii];
                let p1 = g.par(c1, v1);
                let o1 = opt_id(g, c1, v1);
                for c2 in 0..n {
                    if c2 == c1 || g.par(c2, v2) != p1 {
                        continue;
                    }
                    if g.dist[c1 * n + c2] as usize > v2 - v1 {
                        continue;
                    }
                    let o2 = opt_id(g, c2, v2);
                    let t12 = pt[o1 * s_sz + o2];
                    if t12 == 0 {
                        continue;
                    }
                    for v3 in (v2 + 1)..=n {
                        for c3 in 0..n {
                            if c3 == c1 || c3 == c2 || g.par(c3, v3) != p1 {
                                continue;
                            }
                            if g.dist[c2 * n + c3] as usize > v3 - v2 || g.dist[c1 * n + c3] as usize > v3 - v1 {
                                continue;
                            }
                            let o3 = opt_id(g, c3, v3);
                            let t13 = pt[o1 * s_sz + o3];
                            let t23 = pt[o2 * s_sz + o3];
                            if t13 == 0 || t23 == 0 {
                                continue;
                            }
                            cand.fetch_add(1, Ordering::Relaxed);
                            let tri = [(c1, v1), (c2, v2), (c3, v3)];
                            let w: u64;
                            if sym {
                                if !is_canonical(g, &trs, &tri) {
                                    continue;
                                }
                                w = 0; // calcolato solo se N==1
                            } else {
                                w = 1;
                            }
                            canon.fetch_add(1, Ordering::Relaxed);
                            let (cnt, nd, _) = solve(g, &[(c1, v1), (c2, v2), (c3, v3)], 2, 0);
                            nodes.fetch_add(nd, Ordering::Relaxed);
                            nodes_by[cnt as usize].fetch_add(nd, Ordering::Relaxed);
                            match cnt {
                                0 => {
                                    n0.fetch_add(1, Ordering::Relaxed);
                                }
                                1 => {
                                    n1.fetch_add(1, Ordering::Relaxed);
                                    let ww = if sym { orbit_size(g, &trs, &tri) } else { w };
                                    tot_w.fetch_add(ww, Ordering::Relaxed);
                                    if t12 != 1 && t13 != 1 && t23 != 1 {
                                        tot_min_w.fetch_add(ww, Ordering::Relaxed);
                                    }
                                    let mut ex = examples.lock().unwrap();
                                    if ex.len() < 5 {
                                        ex.push(tri);
                                    }
                                    drop(ex);
                                    dump.lock().unwrap().push((ww, tri));
                                }
                                _ => {
                                    n2.fetch_add(1, Ordering::Relaxed);
                                }
                            }
                        }
                    }
                }
            });
        }
    });
    println!(
        "triples: tempo={:.1}s sym={} sample={} | terne candidate (dopo filtro coppie)={} ; ricerche eseguite={} ; N=0:{} N=1:{} N>=2:{} ; nodi={}",
        started.elapsed().as_secs_f64(),
        sym,
        sample,
        cand.load(Ordering::SeqCst),
        canon.load(Ordering::SeqCst),
        n0.load(Ordering::SeqCst),
        n1.load(Ordering::SeqCst),
        n2.load(Ordering::SeqCst),
        nodes.load(Ordering::SeqCst)
    );
    println!("triples: nodi per esito: N=0:{} N=1:{} N>=2:{}", nodes_by[0].load(Ordering::SeqCst), nodes_by[1].load(Ordering::SeqCst), nodes_by[2].load(Ordering::SeqCst));
    println!(
        "triples: INSIEMI DA 3 INDIZI con soluzione unica (pesati sulle orbite) = {} ; di cui minimali (nessuna coppia gia' unica) = {}",
        tot_w.load(Ordering::SeqCst),
        tot_min_w.load(Ordering::SeqCst)
    );
    for e in examples.into_inner().unwrap() {
        println!("   esempio: {:?}", e);
    }
    {
        let mut d = dump.into_inner().unwrap();
        d.sort();
        let mut out = String::new();
        for (ww, t) in &d {
            out.push_str(&format!("{} {} {} {} {} {} {}
", t[0].0, t[0].1, t[1].0, t[1].1, t[2].0, t[2].1, ww));
        }
        let fname = format!("unique_triples_{}x{}_{}.txt", g.h, g.w, if sym { "reps" } else { "all" });
        let _ = std::fs::write(&fname, out);
        println!("triples: scritte {} terne con soluzione unica in {} (formato: c1 v1 c2 v2 c3 v3 peso_orbita)", d.len(), fname);
    }
}

// ---------------------------------------------------------------- utilita'
fn parse_clues(s: &str) -> Vec<(usize, usize)> {
    s.split(',')
        .filter(|x| !x.trim().is_empty())
        .map(|p| {
            let mut it = p.trim().split(':');
            let c: usize = it.next().unwrap().parse().unwrap();
            let v: usize = it.next().unwrap().parse().unwrap();
            (c, v)
        })
        .collect()
}
fn nthreads() -> usize {
    let nt: usize = env::var("NT").ok().and_then(|x| x.parse().ok()).unwrap_or(4);
    nt.min(4).max(1)
}
fn print_grid(g: &Grid, vals: &[u8]) {
    for r in 0..g.h {
        let row: Vec<String> = (0..g.w).map(|c| format!("{:2}", vals[r * g.w + c])).collect();
        println!("      {}", row.join(" "));
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 4 {
        eprintln!("uso: rev_numbrix <cmd> H W [...]");
        return;
    }
    let cmd = args[1].as_str();
    let h: usize = args[2].parse().unwrap();
    let w: usize = args[3].parse().unwrap();
    let g = Grid::new(h, w);
    let nt = nthreads();
    let t0 = Instant::now();
    match cmd {
        "brute" => {
            // forza bruta pura + statistiche con un contatore per valori fissati (griglie piccole)
            let sols = brute_all(&g, nt);
            let print_pairs = args.get(4).map(|s| s == "print").unwrap_or(false);
            brute_stats(&g, &sols, nt, print_pairs);
        }
        "solcount" => {
            // numero totale di soluzioni con la ricerca ancorata (indizio (c,v) per ogni casella c)
            let v: usize = args.get(4).map(|s| s.parse().unwrap()).unwrap_or(1);
            let mut tot = 0u64;
            let mut nodes = 0u64;
            let next = AtomicUsize::new(0);
            let totA = AtomicU64::new(0);
            let nodesA = AtomicU64::new(0);
            thread::scope(|sc| {
                for _ in 0..nt {
                    sc.spawn(|| loop {
                        let c = next.fetch_add(1, Ordering::SeqCst);
                        if c >= g.n {
                            break;
                        }
                        let (cnt, nd, _) = solve(&g, &[(c, v)], u64::MAX, 0);
                        totA.fetch_add(cnt, Ordering::SeqCst);
                        nodesA.fetch_add(nd, Ordering::SeqCst);
                    });
                }
            });
            tot += totA.load(Ordering::SeqCst);
            nodes += nodesA.load(Ordering::SeqCst);
            println!("solcount {}x{} con un indizio di valore {} su ogni casella: somma = {} (nodi {})", h, w, v, tot, nodes);
        }
        "check" => {
            let clues = parse_clues(&args[4]);
            let (cnt, nd, kept) = solve(&g, &clues, u64::MAX, 3);
            println!("check {}x{} {:?}: soluzioni (ricerca ancorata) = {} (nodi {})", h, w, clues, cnt, nd);
            for k in &kept {
                println!("   verifica indipendente della soluzione stampata: {}", verify_solution(&g, k, &clues));
                print_grid(&g, k);
            }
            if args.get(5).map(|s| s == "fwd").unwrap_or(false) {
                let (c2, nd2, sols) = fwd_count(&g, &clues, u64::MAX);
                println!("   secondo risolutore (in avanti da 1): soluzioni = {} (nodi {})", c2, nd2);
                for s in sols.iter().take(1) {
                    let mut arr = [0u8; 64];
                    for i in 0..g.n {
                        arr[i] = s[i];
                    }
                    println!("   verifica indipendente: {}", verify_solution(&g, &arr, &clues));
                    print_grid(&g, &arr);
                }
            }
        }
        "pairs" => {
            let (_pt, st) = run_pairs(&g, nt, true);
            println!(
                "pairs {}x{}: coppie tentate={} N=0:{} N=1:{} N>=2:{} nodi={} tempo={:.1}s picco memoria={:.0} MB",
                h, w, st.attempts, st.n0, st.n1, st.n2, st.nodes, t0.elapsed().as_secs_f64(), peak_mb()
            );
        }
        "triples" => {
            let sym = args.get(4).map(|s| s == "sym").unwrap_or(true);
            let sample: usize = args.get(5).map(|s| s.parse().unwrap()).unwrap_or(1);
            let cache = format!("pt_{}x{}.bin", h, w);
            let pt: Vec<u8>;
            if let Ok(b) = std::fs::read(&cache) {
                println!("tabella delle coppie letta da {} ({} byte; calcolata da questo stesso programma)", cache, b.len());
                pt = b;
            } else {
                let (p, st) = run_pairs(&g, nt, true);
                println!(
                    "pairs {}x{}: coppie tentate={} N=0:{} N=1:{} N>=2:{} nodi={} tempo={:.1}s",
                    h, w, st.attempts, st.n0, st.n1, st.n2, st.nodes, t0.elapsed().as_secs_f64()
                );
                let _ = std::fs::write(&cache, &p);
                pt = p;
            }
            run_triples(&g, &pt, sym, nt, sample);
            println!("totale tempo={:.1}s picco memoria={:.0} MB", t0.elapsed().as_secs_f64(), peak_mb());
        }
        "verifylist" => {
            // legge righe "c1 v1 c2 v2 c3 v3 peso" e controlla con conteggio esatto (nessun limite) che ogni insieme abbia ESATTAMENTE 1 soluzione;
            // per le prime fwdn righe usa anche il secondo risolutore (in avanti da 1)
            let fname = &args[4];
            let fwdn: usize = args.get(5).map(|s| s.parse().unwrap()).unwrap_or(0);
            let txt = std::fs::read_to_string(fname).unwrap();
            let mut bad = 0u64;
            let mut tot = 0u64;
            let mut wsum = 0u64;
            let mut fwd_done = 0u64;
            let mut fwd_bad = 0u64;
            for (i, line) in txt.lines().enumerate() {
                let f: Vec<usize> = line.split_whitespace().map(|x| x.parse().unwrap()).collect();
                if f.len() < 7 {
                    continue;
                }
                let clues = [(f[0], f[1]), (f[2], f[3]), (f[4], f[5])];
                let (cnt, _, kept) = solve(&g, &clues, u64::MAX, 1);
                tot += 1;
                wsum += f[6] as u64;
                if cnt != 1 || kept.len() != 1 || !verify_solution(&g, &kept[0], &clues) {
                    bad += 1;
                    println!("   DISCORDANTE riga {}: {:?} -> {}", i, clues, cnt);
                }
                if i < fwdn {
                    let (c2, _, _) = fwd_count(&g, &clues, u64::MAX);
                    fwd_done += 1;
                    if c2 != 1 {
                        fwd_bad += 1;
                        println!("   DISCORDANTE (in avanti) riga {}: {:?} -> {}", i, clues, c2);
                    }
                }
            }
            println!("verifylist: righe={} somma_pesi={} discordanti(esatto)={} ; controllate col secondo risolutore={} discordanti={}", tot, wsum, bad, fwd_done, fwd_bad);
        }
        "streamcheck" => {
            // validazione: per K insiemi osservati (coppie e terne a caso + terne uniche dal file) confronta N esatto
            // calcolato con la ricerca ancorata a insiemi (solve) con N contato nel flusso di TUTTE le soluzioni (indizio (c,1) per ogni c)
            let seed: u64 = args[4].parse().unwrap();
            let k: usize = args[5].parse().unwrap();
            let uniq_file = args.get(6).cloned();
            let mut rng = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407) | 1;
            let mut nextr = move || {
                rng ^= rng << 13;
                rng ^= rng >> 7;
                rng ^= rng << 17;
                rng
            };
            let n = g.n;
            let mut sets: Vec<Vec<(usize, usize)>> = vec![];
            while sets.len() < k {
                let kk = if sets.len() % 2 == 0 { 2 } else { 3 };
                let mut vs: Vec<usize> = vec![];
                while vs.len() < kk {
                    let v = 1 + (nextr() % n as u64) as usize;
                    if !vs.contains(&v) {
                        vs.push(v);
                    }
                }
                vs.sort();
                let mut cs: Vec<usize> = vec![];
                let mut ok = true;
                for (i, &v) in vs.iter().enumerate() {
                    let mut tries = 0;
                    loop {
                        tries += 1;
                        if tries > 200 {
                            ok = false;
                            break;
                        }
                        let c = (nextr() % n as u64) as usize;
                        if cs.contains(&c) || (n % 2 == 1 && g.par(c, v) != 1) {
                            continue;
                        }
                        if i > 0 && g.dist[cs[i - 1] * n + c] as usize > v - vs[i - 1] {
                            continue;
                        }
                        cs.push(c);
                        break;
                    }
                    if !ok {
                        break;
                    }
                }
                if ok {
                    sets.push(vs.iter().zip(cs.iter()).map(|(&v, &c)| (c, v)).collect());
                }
            }
            if let Some(f) = uniq_file {
                let txt = std::fs::read_to_string(&f).unwrap();
                for (i, line) in txt.lines().enumerate() {
                    if i % 20 != 0 {
                        continue;
                    }
                    let q: Vec<usize> = line.split_whitespace().map(|x| x.parse().unwrap()).collect();
                    sets.push(vec![(q[0], q[1]), (q[2], q[3]), (q[4], q[5])]);
                }
            }
            let mut watch: Vec<([(u8, u8); 3], u8)> = vec![];
            for st in &sets {
                let mut arr = [(0u8, 0u8); 3];
                for (i, &(c, v)) in st.iter().enumerate() {
                    arr[i] = (c as u8, v as u8);
                }
                watch.push((arr, st.len() as u8));
            }
            let next = AtomicUsize::new(0);
            let total = Mutex::new((0u64, vec![0u64; watch.len()]));
            thread::scope(|sc| {
                for _ in 0..nt {
                    sc.spawn(|| loop {
                        let c = next.fetch_add(1, Ordering::SeqCst);
                        if c >= n {
                            break;
                        }
                        let (cnt, wc) = solve_watch(&g, &[(c, 1)], &watch);
                        let mut t = total.lock().unwrap();
                        t.0 += cnt;
                        for i in 0..wc.len() {
                            t.1[i] += wc[i];
                        }
                    });
                }
            });
            let (all, wcnt) = total.into_inner().unwrap();
            let mut bad = 0;
            let mut hist = [0u64; 4];
            let mut minn = u64::MAX;
            for (i, st) in sets.iter().enumerate() {
                let (e, _, _) = solve(&g, st, u64::MAX, 0);
                if e != wcnt[i] {
                    bad += 1;
                    println!("   DISCORDANTE {:?}: esatto {} flusso {}", st, e, wcnt[i]);
                }
                minn = minn.min(e);
                hist[if e == 0 { 0 } else if e == 1 { 1 } else if e < 100 { 2 } else { 3 }] += 1;
            }
            println!(
                "streamcheck {}x{}: soluzioni nel flusso={} ; insiemi osservati={} ; discordanze={} ; N=0:{} N=1:{} N in 2..99:{} N>=100:{} ; minimo N={}",
                h, w, all, sets.len(), bad, hist[0], hist[1], hist[2], hist[3], minn
            );
        }
        "fwdstream" => {
            // come streamcheck, ma il flusso delle soluzioni viene da un enumeratore indipendente (solo connessione delle caselle libere)
            // arg4 = seed, arg5 = K, arg6 = file terne uniche (facoltativo), arg7 = "all" per usare tutte le caselle iniziali
            let seed: u64 = args[4].parse().unwrap();
            let k: usize = args[5].parse().unwrap();
            let uniq_file = args.get(6).cloned().filter(|s| s != "-");
            let all_starts = args.get(7).map(|s| s == "all").unwrap_or(false);
            let mut rng = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407) | 1;
            let mut nextr = move || {
                rng ^= rng << 13;
                rng ^= rng >> 7;
                rng ^= rng << 17;
                rng
            };
            let n = g.n;
            let mut sets: Vec<Vec<(usize, usize)>> = vec![];
            while sets.len() < k {
                let kk = if sets.len() % 2 == 0 { 2 } else { 3 };
                let mut vs: Vec<usize> = vec![];
                while vs.len() < kk {
                    let v = 1 + (nextr() % n as u64) as usize;
                    if !vs.contains(&v) {
                        vs.push(v);
                    }
                }
                vs.sort();
                let mut cs: Vec<usize> = vec![];
                let mut ok = true;
                for (i, &v) in vs.iter().enumerate() {
                    let mut tries = 0;
                    loop {
                        tries += 1;
                        if tries > 200 {
                            ok = false;
                            break;
                        }
                        let c = (nextr() % n as u64) as usize;
                        if cs.contains(&c) {
                            continue;
                        }
                        if i > 0 && g.dist[cs[i - 1] * n + c] as usize > v - vs[i - 1] {
                            continue;
                        }
                        cs.push(c);
                        break;
                    }
                    if !ok {
                        break;
                    }
                }
                if ok {
                    sets.push(vs.iter().zip(cs.iter()).map(|(&v, &c)| (c, v)).collect());
                }
            }
            if let Some(f) = uniq_file {
                let txt = std::fs::read_to_string(&f).unwrap();
                for (i, line) in txt.lines().enumerate() {
                    if i % 20 != 0 {
                        continue;
                    }
                    let q: Vec<usize> = line.split_whitespace().map(|x| x.parse().unwrap()).collect();
                    sets.push(vec![(q[0], q[1]), (q[2], q[3]), (q[4], q[5])]);
                }
            }
            let mut watch: Vec<([(u8, u8); 3], u8)> = vec![];
            for st in &sets {
                let mut arr = [(0u8, 0u8); 3];
                for (i, &(c, v)) in st.iter().enumerate() {
                    arr[i] = (c as u8, v as u8);
                }
                watch.push((arr, st.len() as u8));
            }
            let starts: Vec<usize> = (0..n).filter(|&c| all_starts || n % 2 == 0 || g.color[c] == 0).collect();
            let next = AtomicUsize::new(0);
            let total = Mutex::new((0u64, vec![0u64; watch.len()]));
            thread::scope(|sc| {
                for _ in 0..nt {
                    sc.spawn(|| loop {
                        let i = next.fetch_add(1, Ordering::SeqCst);
                        if i >= starts.len() {
                            break;
                        }
                        let (cnt, wc) = fwd_stream(&g, starts[i], &watch);
                        let mut t = total.lock().unwrap();
                        t.0 += cnt;
                        for j in 0..wc.len() {
                            t.1[j] += wc[j];
                        }
                    });
                }
            });
            let (all, wcnt) = total.into_inner().unwrap();
            let mut bad = 0;
            let mut hist = [0u64; 4];
            for (i, st) in sets.iter().enumerate() {
                let (e, _, _) = solve(&g, st, u64::MAX, 0);
                if e != wcnt[i] {
                    bad += 1;
                    println!("   DISCORDANTE {:?}: esatto(ancorato) {} flusso(in avanti) {}", st, e, wcnt[i]);
                }
                hist[if e == 0 { 0 } else if e == 1 { 1 } else if e < 100 { 2 } else { 3 }] += 1;
            }
            println!(
                "fwdstream {}x{}: soluzioni nel flusso={} ; insiemi osservati={} ; discordanze={} ; N=0:{} N=1:{} N in 2..99:{} N>=100:{}",
                h, w, all, sets.len(), bad, hist[0], hist[1], hist[2], hist[3]
            );
        }
        "zerocheck" => {
            // flusso indipendente di TUTTE le soluzioni (solo connessione): ogni coppia (casella,numero) contenuta in una soluzione
            // deve avere esito >=1 nella tabella delle coppie pt_HxW.bin (nessuno zero falso); e nessuna deve avere esito 1.
            let cache = format!("pt_{}x{}.bin", h, w);
            let pt = std::fs::read(&cache).unwrap();
            let n = g.n;
            let starts: Vec<usize> = (0..n).filter(|&c| n % 2 == 0 || g.color[c] == 0).collect();
            let next = AtomicUsize::new(0);
            let total = Mutex::new((0u64, 0u64, 0u64, 0u64));
            thread::scope(|sc| {
                for _ in 0..nt {
                    sc.spawn(|| loop {
                        let i = next.fetch_add(1, Ordering::SeqCst);
                        if i >= starts.len() {
                            break;
                        }
                        let (c, b, o, l, _) = fwd_stream_pt(&g, starts[i], &pt, 0);
                        let mut t = total.lock().unwrap();
                        t.0 += c;
                        t.1 += b;
                        t.2 += o;
                        t.3 += l;
                    });
                }
            });
            let t = total.into_inner().unwrap();
            println!("zerocheck {}x{}: soluzioni={} ; ricerche in tabella={} ; coppie contenute in una soluzione ma segnate 0 = {} ; segnate 1 = {}", h, w, t.0, t.3, t.1, t.2);
        }
        "soundness" => {
            // campione di soluzioni dal flusso indipendente; per ciascuna, per OGNI terna di valori (e ogni coppia) controlla che
            // la ricerca ancorata (limite 2) trovi almeno 1 soluzione; se ne trova esattamente 1 la terna deve essere nel file delle terne uniche espanso.
            let period: u64 = args[4].parse().unwrap();
            let maxs: usize = args[5].parse().unwrap();
            let reps_file = &args[6];
            let n = g.n;
            let trs = transforms(&g);
            let mut uniq: HashSet<[u32; 3]> = HashSet::new();
            for line in std::fs::read_to_string(reps_file).unwrap().lines() {
                let q: Vec<usize> = line.split_whitespace().map(|x| x.parse().unwrap()).collect();
                if q.len() < 7 {
                    continue;
                }
                let t = [(q[0], q[1]), (q[2], q[3]), (q[4], q[5])];
                for &tr in trs.iter() {
                    uniq.insert(img_triple(&g, tr, &t));
                }
            }
            println!("soundness: terne uniche espanse dal file = {}", uniq.len());
            let starts: Vec<usize> = (0..n).filter(|&c| n % 2 == 0 || g.color[c] == 0).collect();
            let mut samples: Vec<[u8; 64]> = vec![];
            for &st in starts.iter() {
                let empty: Vec<([(u8, u8); 3], u8)> = Vec::new();
                let mut s = FS { g: &g, vals: [0; 64], count: 0, watch: &empty, wc: vec![], pt: None, bad: 0, ones: 0, lookups: 0, period, samples: vec![] };
                s.vals[st] = 1;
                fs_rec(&mut s, 1, st, 1u64 << st);
                samples.extend(s.samples);
            }
            let step = (samples.len() / maxs).max(1);
            let mut chosen: Vec<[u8; 64]> = samples.iter().step_by(step).take(maxs).cloned().collect();
            if args.get(7).map(|x| x == "fromreps").unwrap_or(false) {
                // soluzioni uniche delle terne rappresentanti (una ogni 'step_r' righe), cosi' il controllo include terne che DEVONO risultare uniche
                chosen.clear();
                let txt = std::fs::read_to_string(reps_file).unwrap();
                let lines: Vec<&str> = txt.lines().collect();
                let step_r = (lines.len() / maxs).max(1);
                for (i, line) in lines.iter().enumerate() {
                    if i % step_r != 0 || chosen.len() >= maxs {
                        continue;
                    }
                    let q: Vec<usize> = line.split_whitespace().map(|x| x.parse().unwrap()).collect();
                    let (cnt, _, kept) = solve(&g, &[(q[0], q[1]), (q[2], q[3]), (q[4], q[5])], u64::MAX, 1);
                    assert!(cnt == 1);
                    chosen.push(kept[0]);
                }
            }
            println!("soundness: soluzioni campionate = {} (su {} nel campione grezzo)", chosen.len(), samples.len());
            let next = AtomicUsize::new(0);
            let tot_tests = AtomicU64::new(0);
            let missing = AtomicU64::new(0);
            let ones_in = AtomicU64::new(0);
            let ones_out = AtomicU64::new(0);
            let notone_in = AtomicU64::new(0);
            let nodes = AtomicU64::new(0);
            thread::scope(|sc| {
                for _ in 0..nt {
                    sc.spawn(|| loop {
                        let si = next.fetch_add(1, Ordering::SeqCst);
                        if si >= chosen.len() {
                            break;
                        }
                        let sol = &chosen[si];
                        let mut pos = vec![0usize; n + 1];
                        for c in 0..n {
                            pos[sol[c] as usize] = c;
                        }
                        assert!(verify_solution(&g, sol, &[]));
                        for a in 1..=n {
                            for b in (a + 1)..=n {
                                let (cnt, nd, _) = solve(&g, &[(pos[a], a), (pos[b], b)], 2, 0);
                                nodes.fetch_add(nd, Ordering::Relaxed);
                                tot_tests.fetch_add(1, Ordering::Relaxed);
                                if cnt == 0 {
                                    missing.fetch_add(1, Ordering::Relaxed);
                                }
                                if cnt == 1 {
                                    ones_out.fetch_add(1, Ordering::Relaxed);
                                }
                            }
                        }
                        for a in 1..=n {
                            for b in (a + 1)..=n {
                                for c in (b + 1)..=n {
                                    let t = [(pos[a], a), (pos[b], b), (pos[c], c)];
                                    let (cnt, nd, _) = solve(&g, &t, 2, 0);
                                    nodes.fetch_add(nd, Ordering::Relaxed);
                                    tot_tests.fetch_add(1, Ordering::Relaxed);
                                    let key = img_triple(&g, (0, false), &t);
                                    let inlist = uniq.contains(&key);
                                    if cnt == 0 {
                                        missing.fetch_add(1, Ordering::Relaxed);
                                    }
                                    if cnt == 1 {
                                        if inlist {
                                            ones_in.fetch_add(1, Ordering::Relaxed);
                                        } else {
                                            ones_out.fetch_add(1, Ordering::Relaxed);
                                        }
                                    } else if inlist {
                                        notone_in.fetch_add(1, Ordering::Relaxed);
                                    }
                                }
                            }
                        }
                    });
                }
            });
            println!(
                "soundness {}x{}: ricerche={} (nodi {}) ; esito 0 su sottoinsiemi di soluzioni vere = {} ; terne con esito 1 gia' nel file = {} ; con esito 1 NON nel file (coppie incluse) = {} ; nel file ma esito>=2 = {}",
                h, w, tot_tests.load(Ordering::SeqCst), nodes.load(Ordering::SeqCst), missing.load(Ordering::SeqCst), ones_in.load(Ordering::SeqCst), ones_out.load(Ordering::SeqCst), notone_in.load(Ordering::SeqCst)
            );
        }
        "pairverify" => {
            // per ogni coppia con esito 2 nella tabella: rilancia la ricerca, tiene 2 soluzioni e verifica con codice separato che siano
            // soluzioni vere (numeri 1..n una volta, consecutivi adiacenti), contengano entrambi gli indizi e siano diverse tra loro
            let cache = format!("pt_{}x{}.bin", h, w);
            let pt = std::fs::read(&cache).unwrap();
            let n = g.n;
            let ssz = n * (n + 1);
            let next = AtomicUsize::new(0);
            let checked = AtomicU64::new(0);
            let bad = AtomicU64::new(0);
            thread::scope(|sc| {
                for _ in 0..nt {
                    sc.spawn(|| loop {
                        let o1 = next.fetch_add(1, Ordering::SeqCst);
                        if o1 >= ssz {
                            break;
                        }
                        for o2 in (o1 + 1)..ssz {
                            if pt[o1 * ssz + o2] != 2 {
                                continue;
                            }
                            let (c1, v1) = (o1 / (n + 1), o1 % (n + 1));
                            let (c2, v2) = (o2 / (n + 1), o2 % (n + 1));
                            let clues = [(c1, v1), (c2, v2)];
                            let (cnt, _, kept) = solve(&g, &clues, 2, 2);
                            checked.fetch_add(1, Ordering::Relaxed);
                            let ok = cnt == 2
                                && kept.len() == 2
                                && verify_solution(&g, &kept[0], &clues)
                                && verify_solution(&g, &kept[1], &clues)
                                && kept[0][..n] != kept[1][..n];
                            if !ok {
                                bad.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                    });
                }
            });
            println!("pairverify {}x{}: coppie con esito 2 ricontrollate = {} ; con due soluzioni verificate e diverse = {} ; anomalie = {}", h, w, checked.load(Ordering::SeqCst), checked.load(Ordering::SeqCst) - bad.load(Ordering::SeqCst), bad.load(Ordering::SeqCst));
        }
        "sumpair" => {
            // validazione: per (c1,v1,v2): somma su c2 di N((c1,v1),(c2,v2)) = N((c1,v1))
            let c1: usize = args[4].parse().unwrap();
            let v1: usize = args[5].parse().unwrap();
            let v2: usize = args[6].parse().unwrap();
            let (n1, _, _) = solve(&g, &[(c1, v1)], u64::MAX, 0);
            let mut s = 0u64;
            for c2 in 0..g.n {
                if c2 == c1 {
                    continue;
                }
                let (cn, _, _) = solve(&g, &[(c1, v1), (c2, v2)], u64::MAX, 0);
                s += cn;
            }
            println!("sumpair {}x{} c1={} v1={} v2={}: N(c1,v1)={} ; somma_c2 N(c1,v1,c2,v2)={} ; {}", h, w, c1, v1, v2, n1, s, if n1 == s { "OK" } else { "DISCORDANTE" });
        }
        _ => eprintln!("comando sconosciuto"),
    }
    eprintln!("[{} {}x{}] tempo totale {:.1}s, picco memoria {:.0} MB", cmd, h, w, t0.elapsed().as_secs_f64(), peak_mb());
}
