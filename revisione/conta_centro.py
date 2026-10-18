"""Dalle 54.608 terne uniche (espansione delle orbite) conta quelle che contengono l'indizio (casella, 1) per le caselle richieste,
e quelle che stanno in una soluzione con l'1 nella casella richiesta (anche se l'1 non e' uno degli indizi).
Uso: python -I conta_centro.py FILE_REPS"""
import sys
from collections import Counter

H = W = 7
N = 49
fname = sys.argv[1]

maps = []
funcs = [lambda r, c: (r, c), lambda r, c: (H - 1 - r, c), lambda r, c: (r, W - 1 - c), lambda r, c: (H - 1 - r, W - 1 - c),
         lambda r, c: (c, r), lambda r, c: (c, W - 1 - r), lambda r, c: (H - 1 - c, r), lambda r, c: (H - 1 - c, W - 1 - r)]
for f in funcs:
    m = [0] * N
    for r in range(H):
        for c in range(W):
            r2, c2 = f(r, c)
            m[r * W + c] = r2 * W + c2
    maps.append(m)

allt = set()
for line in open(fname):
    f = list(map(int, line.split()))
    if len(f) < 7:
        continue
    t = ((f[0], f[1]), (f[2], f[3]), (f[4], f[5]))
    for m in maps:
        for rev in (False, True):
            allt.add(tuple(sorted(((N + 1 - v) if rev else v, m[c]) for c, v in t)))
print("terne uniche totali:", len(allt))
cnt1 = Counter()
for t in allt:
    for v, c in t:
        if v == 1:
            cnt1[c] += 1
print("terne che contengono l'indizio 1 in ciascuna casella (casella: numero):", dict(sorted(cnt1.items())))
print("  centro (24,1):", cnt1[24], " (2,2)=casella 16:", cnt1[16], " (2,4)=18:", cnt1[18], " (4,2)=30:", cnt1[30], " (4,4)=32:", cnt1[32])
print("  terne con 1 come indizio: totale", sum(cnt1.values()))
print("  terne SENZA l'indizio 1:", len(allt) - sum(cnt1.values()))
