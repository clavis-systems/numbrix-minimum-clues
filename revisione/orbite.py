"""Controllo indipendente (in Python) dei pesi delle orbite e della somma finale.
Uso: python -I orbite.py FILE_REPS H W
Ogni riga del file: c1 v1 c2 v2 c3 v3 peso. Le simmetrie sono ricostruite qui da zero.
"""
import sys

fname, H, W = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
N = H * W


def geo_maps():
    maps = []
    base = []
    for r in range(H):
        for c in range(W):
            base.append((r, c))
    funcs = [lambda r, c: (r, c), lambda r, c: (H - 1 - r, c), lambda r, c: (r, W - 1 - c), lambda r, c: (H - 1 - r, W - 1 - c)]
    if H == W:
        funcs += [lambda r, c: (c, r), lambda r, c: (c, W - 1 - r), lambda r, c: (H - 1 - c, r), lambda r, c: (H - 1 - c, W - 1 - r)]
    for f in funcs:
        m = [0] * N
        for r, c in base:
            r2, c2 = f(r, c)
            m[r * W + c] = r2 * W + c2
        maps.append(m)
    # controllo: sono permutazioni distinte
    assert len({tuple(m) for m in maps}) == len(maps)
    return maps


MAPS = geo_maps()


def images(t):
    """t = ((c,v),(c,v),(c,v)); restituisce l'insieme delle immagini come tuple ordinate di (v,c)"""
    out = set()
    for m in MAPS:
        for rev in (False, True):
            im = []
            for c, v in t:
                im.append(((N + 1 - v) if rev else v, m[c]))
            out.add(tuple(sorted(im)))
    return out


tot = 0
canon_forms = set()
all_triples = set()
bad = 0
n = 0
for line in open(fname):
    f = list(map(int, line.split()))
    if len(f) < 7:
        continue
    n += 1
    t = ((f[0], f[1]), (f[2], f[3]), (f[4], f[5]))
    w = f[6]
    ims = images(t)
    if len(ims) != w:
        bad += 1
        print("peso diverso", t, w, len(ims))
    me = tuple(sorted((v, c) for c, v in t))
    if min(ims) != me:
        bad += 1
        print("non canonico", t)
    if me in canon_forms:
        bad += 1
        print("rappresentante ripetuto", t)
    canon_forms.add(me)
    all_triples |= ims
    tot += w
print(f"orbite.py: rappresentanti={n} somma_pesi={tot} terne distinte nell'unione delle orbite={len(all_triples)} anomalie={bad}")
