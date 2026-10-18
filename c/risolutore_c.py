"""Risolutore di controllo C (PIANO_NUMBRIX_48H.md): dato un puzzle Numbrix (griglia m×n e indizi casella:numero),
conta le soluzioni fermandosi a 2. Scritto da zero, senza riusare il programma A.

Ricerca in profondità sui numeri 1, 2, 3, …; tagli:
- una casella con un indizio può ricevere solo il suo numero;
- il prossimo indizio (numero w nella casella c) deve essere raggiungibile: distanza di Manhattan ≤ w − k con la
  stessa parità;
- le caselle ancora libere, più la testa, devono restare collegate, e al più una di loro può avere un solo vicino
  libero (sarebbe la fine del percorso).
Uso: python -I risolutore_c.py M N c1:v1 c2:v2 ...   (caselle numerate riga*N + colonna, da 0)
"""
import json
import sys


def risolvi(m, n, indizi, limite=2):
    nn = m * n
    vicini = [[] for _ in range(nn)]
    for r in range(m):
        for c in range(n):
            i = r * n + c
            for (dr, dc) in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                r2, c2 = r + dr, c + dc
                if 0 <= r2 < m and 0 <= c2 < n:
                    vicini[i].append(r2 * n + c2)
    maschera_vicini = [sum(1 << j for j in vicini[i]) for i in range(nn)]
    numero_in = dict(indizi)                      # casella -> numero
    casella_di = {v: c for c, v in indizi}        # numero -> casella
    valori_indizi = sorted(casella_di)
    tutte = (1 << nn) - 1

    def dist(a, b):
        return abs(a // n - b // n) + abs(a % n - b % n)

    def prossimo_indizio(k):
        for w in valori_indizi:
            if w > k:
                return w
        return None

    def ok_struttura(testa, liberi):
        if liberi == 0:
            return True
        h = liberi | (1 << testa)
        uno = 0
        x = liberi
        while x:
            b = x & -x
            u = b.bit_length() - 1
            x ^= b
            d = bin(maschera_vicini[u] & h).count("1")
            if d == 0:
                return False
            if d == 1:
                uno += 1
                if uno > 1:
                    return False
        # collegamento: visita in ampiezza dentro h partendo dalla testa
        visto = 1 << testa
        frontiera = [testa]
        while frontiera:
            nuova = []
            for u in frontiera:
                y = maschera_vicini[u] & h & ~visto
                while y:
                    b = y & -y
                    visto |= b
                    nuova.append(b.bit_length() - 1)
                    y ^= b
            frontiera = nuova
        return visto == h

    trovate = []
    sol = [0] * nn

    def ammissibile(x, k, liberi):
        # x riceve il numero k; liberi = caselle libere dopo la mossa
        if x in numero_in and numero_in[x] != k:
            return False
        if k in casella_di and casella_di[k] != x:
            return False
        w = prossimo_indizio(k)
        if w is not None:
            c = casella_di[w]
            if not (liberi >> c) & 1:
                return False
            d = dist(x, c)
            if d > w - k or (w - k - d) % 2:
                return False
        return ok_struttura(x, liberi)

    def dfs(testa, k, liberi):
        if len(trovate) >= limite:
            return
        if k == nn:
            trovate.append(list(sol))
            return
        for x in vicini[testa]:
            if (liberi >> x) & 1:
                l2 = liberi & ~(1 << x)
                if ammissibile(x, k + 1, l2):
                    sol[x] = k + 1
                    dfs(x, k + 1, l2)
                    sol[x] = 0

    for s in range(nn):
        liberi = tutte & ~(1 << s)
        if ammissibile(s, 1, liberi):
            sol[s] = 1
            dfs(s, 1, liberi)
            sol[s] = 0
        if len(trovate) >= limite:
            break
    return trovate


def controlla_soluzione(m, n, s, indizi):
    """Controllo diretto: s è una permutazione di 1..N, numeri consecutivi vicini, indizi rispettati."""
    nn = m * n
    if sorted(s) != list(range(1, nn + 1)):
        return False
    pos = {v: i for i, v in enumerate(s)}
    for k in range(1, nn):
        a, b = pos[k], pos[k + 1]
        if abs(a // n - b // n) + abs(a % n - b % n) != 1:
            return False
    return all(s[c] == v for c, v in indizi)


if __name__ == "__main__":
    m, n = int(sys.argv[1]), int(sys.argv[2])
    indizi = [tuple(int(x) for x in a.split(":")) for a in sys.argv[3:]]
    tr = risolvi(m, n, indizi)
    print(json.dumps({"m": m, "n": n, "indizi": indizi, "soluzioni_fino_a_2": len(tr),
                      "soluzione_valida": [controlla_soluzione(m, n, s, indizi) for s in tr],
                      "soluzione": tr[0] if len(tr) == 1 else None}))
