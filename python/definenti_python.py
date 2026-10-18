"""Controllo a forza bruta degli insiemi definenti (solo griglie piccole, fino a 5×5).

Elenca tutte le soluzioni; per ogni soluzione prende tutti i sottoinsiemi di 1, 2 e 3 suoi elementi (casella, numero)
e li conta in un dizionario. Gli insiemi definenti sono quelli contati una volta sola. Metodo volutamente diverso dal
programma A (nessun raggruppamento, nessuna saturazione).
Uso: python -I definenti_python.py M N
"""
import itertools
import json
import sys
from collections import Counter


def soluzioni(m, n):
    nn = m * n
    vicini = []
    for r in range(m):
        for c in range(n):
            v = []
            if r > 0: v.append((r - 1) * n + c)
            if r + 1 < m: v.append((r + 1) * n + c)
            if c > 0: v.append(r * n + c - 1)
            if c + 1 < n: v.append(r * n + c + 1)
            vicini.append(v)
    val = [0] * nn
    out = []

    def dfs(x, k):
        if k == nn:
            out.append(tuple(val))
            return
        for y in vicini[x]:
            if val[y] == 0:
                val[y] = k + 1
                dfs(y, k + 1)
                val[y] = 0

    for s in range(nn):
        val[s] = 1
        dfs(s, 1)
        val[s] = 0
    return out


def main():
    m, n = int(sys.argv[1]), int(sys.argv[2])
    sol = soluzioni(m, n)
    ris = {"m": m, "n": n, "soluzioni": len(sol)}
    for k in (1, 2, 3):
        cont = Counter()
        for s in sol:
            elementi = [(c, v) for c, v in enumerate(s)]
            cont.update(itertools.combinations(elementi, k))
        ris[f"definenti_{k}"] = sum(1 for x in cont.values() if x == 1)
    print(json.dumps(ris))


if __name__ == "__main__":
    main()
