"""Controllo con numpy sull'elenco completo delle soluzioni (file binario, N = M*N byte per soluzione: il numero di
ogni casella). Nessuna ricerca: solo filtri e conteggi. Il file si legge a pezzi (memmap), quindi può essere più
grande della RAM.

1. "valide": ogni riga è una permutazione di 1..N con numeri consecutivi in caselle vicine; righe tutte diverse
   (solo per file che stanno in memoria);
2. "puzzle": per ogni puzzle dato, quante righe rispettano tutti gli indizi;
3. "gruppo C V": per un elemento (casella, numero), quante terne definenti lo contengono, contando per ogni coppia di
   altre caselle le combinazioni di numeri che compaiono una volta sola.
Uso: python -I controlla_numpy.py FILE [--griglia M N] [valide] [gruppo C V] [puzzle c:v,c:v,c:v ...]
"""
import itertools
import json
import sys
import time

import numpy as np

M = N = 7
BLOCCO = 4_000_000


def carica(p):
    a = np.memmap(p, dtype=np.uint8, mode="r")
    assert a.size % (M * N) == 0
    return a.reshape(-1, M * N)


def valide(a):
    nn = M * N
    righe_ok = 0
    for i in range(0, len(a), BLOCCO):
        b = np.asarray(a[i:i + BLOCCO]).astype(np.int16)
        ordinate = np.sort(b, axis=1)
        perm = np.all(ordinate == np.arange(1, nn + 1, dtype=np.int16), axis=1)
        pos = np.argsort(b, axis=1)
        r, c = pos // N, pos % N
        passi = np.abs(np.diff(r, axis=1)) + np.abs(np.diff(c, axis=1))
        righe_ok += int(np.sum(perm & np.all(passi == 1, axis=1)))
    v = np.ascontiguousarray(a).view(np.dtype((np.void, nn))).ravel()
    return righe_ok, len(np.unique(v))


def conta_puzzle(a, indizi):
    tot = 0
    for i in range(0, len(a), BLOCCO):
        b = a[i:i + BLOCCO]
        mask = np.ones(len(b), dtype=bool)
        for c, v in indizi:
            mask &= b[:, c] == v
        tot += int(mask.sum())
    return tot


def terne_del_gruppo(a, c0, v0):
    parti = []
    for i in range(0, len(a), BLOCCO):
        b = a[i:i + BLOCCO]
        parti.append(np.asarray(b[b[:, c0] == v0]))
    s = np.concatenate(parti)
    altre = [x for x in range(M * N) if x != c0]
    tot = 0
    for c1, c2 in itertools.combinations(altre, 2):
        chiave = s[:, c1].astype(np.int32) * 128 + s[:, c2].astype(np.int32)
        conti = np.bincount(chiave, minlength=128 * 128)
        tot += int(np.sum(conti == 1))
    return len(s), tot


def main():
    global M, N
    args = sys.argv[2:]
    if "--griglia" in args:
        i = args.index("--griglia")
        M, N = int(args[i + 1]), int(args[i + 2])
        del args[i:i + 3]
    a = carica(sys.argv[1])
    out = {"griglia": [M, N], "righe": len(a)}
    t = time.time()
    if "valide" in args:
        out["righe_valide"], out["righe_distinte"] = valide(a)
    if "gruppo" in args:
        i = args.index("gruppo")
        c0, v0 = int(args[i + 1]), int(args[i + 2])
        out["gruppo"] = [c0, v0]
        out["soluzioni_con_elemento"], out["terne_definenti_con_elemento"] = terne_del_gruppo(a, c0, v0)
    if "puzzle" in args:
        i = args.index("puzzle")
        out["puzzle"] = []
        for p in args[i + 1:]:
            if p in ("valide", "gruppo"):
                break
            ind = [tuple(int(x) for x in q.split(":")) for q in p.split(",")]
            out["puzzle"].append({"indizi": ind, "righe_che_lo_rispettano": conta_puzzle(a, ind)})
    out["secondi"] = round(time.time() - t, 1)
    print(json.dumps(out))


if __name__ == "__main__":
    main()
