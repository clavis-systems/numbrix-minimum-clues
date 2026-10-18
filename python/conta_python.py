"""Cancello 1, punto 2: conteggio indipendente a forza bruta delle soluzioni di Numbrix m×n (percorsi hamiltoniani
orientati), senza potature. Serve solo per griglie piccole.
Uso: python -I conta_python.py M N
"""
import sys


def conta(m, n):
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
    visti = [False] * nn
    totale = 0

    def dfs(x, k):
        nonlocal totale
        if k == nn:
            totale += 1
            return
        for y in vicini[x]:
            if not visti[y]:
                visti[y] = True
                dfs(y, k + 1)
                visti[y] = False

    for s in range(nn):
        visti[s] = True
        dfs(s, 1)
        visti[s] = False
    return totale


if __name__ == "__main__":
    m, n = int(sys.argv[1]), int(sys.argv[2])
    print(f'{{"m": {m}, "n": {n}, "soluzioni": {conta(m, n)}}}')
