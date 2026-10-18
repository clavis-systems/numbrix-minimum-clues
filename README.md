# Minimal Numbrix puzzles: 3 clues are enough on 7×7, 2 on 5×6


In a **Numbrix** puzzle you fill an m×n grid (m rows, n columns) with the numbers 1, 2, …, m·n so that consecutive
numbers are in cells next to each other (horizontally or vertically). Some numbers are given as **clues**, and a set
of clues **defines** a puzzle if exactly one solution contains them.

Hanson and Nash ("Minimal and maximal Numbrix puzzles", [arXiv:1706.09389](https://arxiv.org/abs/1706.09389)) proved
that ⌈m/2⌉ clues are always enough for 3 ≤ m ≤ n. They conjectured that this is the minimum for 5×n, 6×n and 7×n. I
checked the small cases by computer.

## What I found

- **7×7: the minimum is 3, not 4.** No set of 2 clues defines a puzzle, but 54,608 sets of 3 clues do. This goes
  against conjecture (iii) ("the minimum number of clues for 7×n is 4") for n = 7.
- **7×8: the minimum is also 3** (100,104 sets of 3 clues, none of 2).
- **5×6 (5 rows, 6 columns): the minimum is 2, not 3.** There are 40 defining pairs. This goes against conjecture
  (i) for n = 6.
- **The other cases agree with the conjectures:** 5×5, 5×7, 5×8, 6×6, 6×7 and 6×8 have minimum 3.
- **8×8:** not computed yet.

**Minimum number of clues for an n×n board, n = 1…7:** 0, 2, 2, 2, 3, 3, 3.

The counts of defining sets count a puzzle, its rotations and reflections, and its reversal (k → m·n+1−k)
separately.

Cells are numbered row by row, starting at 0. A clue is written `cell:number`: for example `24:1` is the number 1 in
cell 24, the centre of the 7×7 board.

### A 3-clue 7×7 puzzle

```
 .  6  .  .  .  .  .            7  6  5  4 49 48 47
 .  .  .  .  .  .  .            8  9 10  3 18 19 46
31  .  .  .  .  .  .           31 30 11  2 17 20 45
 .  .  .  1  .  .  .    ->     32 29 12  1 16 21 44
 .  .  .  .  .  .  .           33 28 13 14 15 22 43
 .  .  .  .  .  .  .           34 27 26 25 24 23 42
 .  .  .  .  .  .  .           35 36 37 38 39 40 41
```

This example has 1 in the middle. Hanson and Nash wrote that they had not yet analysed all the 7×7 cases, and that
in the subsets they looked at (for example, solutions with 1 in the middle) they had not found 3 clues, with 1 as a
clue, that define a puzzle. There are 80 defining sets of 3 clues that contain the clue 1 in the centre cell.

### A 2-clue 5×6 puzzle

```
 .  .  2  7  .  .          30  1  2  7  8  9
 .  .  .  .  .  .          29 28  3  6 11 10
 .  .  .  .  .  .    ->    26 27  4  5 12 13
 .  .  .  .  .  .          25 22 21 18 17 14
 .  .  .  .  .  .          24 23 20 19 16 15
```

All 40 defining pairs of 5×6 use the two middle cells of the top row or of the bottom row. The shape matters: if you
keep the two clues in the same places (top row, third and fourth cells) on a board with 6 rows and 5 columns, there
are 8 solutions.

## All the numbers

"Defining sets of 3" counts every 3-clue set with a unique solution, also those that already contain a defining
pair. This only matters where defining pairs exist.

| Board | Solutions | Defining sets of 1 | of 2 | of 3 | Minimum |
|---|---|---|---|---|---|
| 1×1 | 1 | 1 | 0 | 0 | 0 (the only solution needs no clue) |
| 2×2 | 8 | 0 | 32 | 32 | 2 |
| 2×3 | 16 | 0 | 116 | 256 | 2 |
| 3×3 | 40 | 0 | 64 | 1,616 | 2 |
| 3×4 | 124 | 0 | 744 | 10,448 | 2 |
| 4×4 | 552 | 0 | 1,080 | 53,104 | 2 |
| 4×5 | 2,012 | 0 | 1,016 | 169,128 | 2 |
| 5×5 | 8,648 | 0 | 0 | 155,824 | 3 |
| 5×6 | 53,992 | 0 | **40** | 671,208 | **2** |
| 5×7 | 219,444 | 0 | 0 | 267,832 | 3 |
| 5×8 | 1,205,608 | 0 | 0 | 804,536 | 3 |
| 6×6 | 458,696 | 0 | 0 | 638,000 | 3 |
| 6×7 | 3,240,068 | 0 | 0 | 569,792 | 3 |
| 6×8 | 24,142,924 | 0 | 0 | 478,432 | 3 |
| 7×7 | 27,070,560 | 0 | 0 | **54,608** | **3** |
| 7×8 | 351,810,620 | 0 | 0 | 100,104 | 3 |

The numbers of solutions match OEIS [A096969](https://oeis.org/A096969) for the square boards. For the rectangles
they match [A366399](https://oeis.org/A366399) after doubling: A366399 is a triangle T(n,k) with k ≤ n that counts
each path once, without direction (for example T(2,2) = 4, while a 2×2 board has 8 solutions). For 5×6,
T(6,5) = 26,996 and 2 × 26,996 = 53,992.

## How it was checked

I did not want to trust one program, so the same numbers were computed in different ways:

| Program | How it works | Where |
|---|---|---|
| A (Rust) | For each clue (cell, number), it lists the solutions containing it and counts all pairs of other clues | `a/` |
| B (Rust) | Groups all solutions by pairs of positions. Written by a separate AI agent that only saw a short specification, not program A | `b/` |
| D (Rust) | One pass over all solutions, with a counter for every pair of cells and numbers; its own path generator. It checks sets of 1 and 2 clues only | `d/` |
| C (Python) | A small solver that counts the solutions of a given puzzle | `c/` |
| Brute force (Python) | Lists every solution and every set of 1, 2 and 3 clues, without shortcuts; for small boards | `python/` |
| numpy | Reads the full list of the 27,070,560 solutions of 7×7 from disk and counts directly | `python/` |
| Reviewer (Rust + Python) | Written by another separate AI agent, with no access to the other programs or results | `revisione/` |

What agrees with what:
- **7×7:**
  - A (with and without symmetry), B and the reviewer all give 54,608 defining sets of 3;
  - A, B, D and the reviewer all give 0 defining sets of 2;
  - the examples have exactly one solution, checked with C and with numpy.
- **5×6:** A, B, D and the reviewer all give 40 pairs; the example was also checked with C.
- **7×8:**
  - the "no 2-clue set" result is checked by A, D and the reviewer;
  - the count 100,104 comes from A only. Some of its examples (in the `a_7x8_gruppo_*` files) were checked with C and
    numpy.
- **Small boards:** A, B and D agree with the brute force in `python/` (for example on 4×4 and 4×5).

Not every check left a file: the outputs of A, B and D are in `risultati/`, and so is the reviewer's list of the
7×7 defining sets of 3. The other reviewer runs, the brute force runs and the numpy checks are only in my notes.

### The files in `risultati/`

- One JSON file per board and program: `a_MxN.json`, `b_MxN.json` and `d_MxN.json`. `a_MxN_simmetrie.json` is
  program A run with the symmetries of the board.
- `a_7x7_gruppo_C_V.json` and `a_7x8_gruppo_C_V.json`: program A run only on the sets that contain the clue C:V,
  with examples. For instance, `a_7x7_gruppo_24_1.json` gives the 80 sets with 1 in the centre.
- The keys are in Italian:
  - "soluzioni" = solutions;
  - "definenti_1/2/3" = defining sets of 1/2/3 clues;
  - "esempi" = examples;
  - "indizi" = clues, as [cell, number];
  - "soluzione" = the number in each cell, row by row;
  - "secondi" = seconds.
- `terne_definenti_7x7_rappresentanti.txt` ("representative defining triples") is the reviewer's list of the
  54,608 defining sets of 3 clues of 7×7, grouped under the 16 symmetries of the board: the 8 rotations and
  reflections, each with or without the reversal k → 50−k.
  - Each of its 3,413 lines is "cell number cell number cell number weight". The weight is the size of the group,
    always 16 here.
  - 3,413 × 16 = 54,608.

## How to run it

You need Rust (cargo) and Python 3 with numpy. On a 16-thread laptop (AMD Ryzen 7):

```
cd a
cargo run --release -- conta 7 7                 # 27070560 solutions, about 5 s
cargo run --release -- definenti 7 7 simmetrie   # 0, 0, 54608 defining sets of 1, 2, 3 clues, about 1 min
cargo run --release -- definenti 5 6             # 40 defining pairs, under 1 s
cargo run --release -- gruppo 7 7 24 1           # 80 defining sets of 3 with the clue 24:1
cd ..
python c/risolutore_c.py 7 7 24:1 1:6 14:31      # exactly 1 solution
cd b
cargo run --release -- griglia 5 6 --senza-file  # program B: 40 pairs
```

The program names and outputs are in Italian ("conta" = count, "definenti" = defining sets, "simmetrie" = use the
symmetries of the board, "gruppo" = group, "griglia" = grid).

Notes on the separate agents' programs:
- **Program B** writes its files to a folder outside the repository unless you say otherwise: use `--uscita FOLDER`
  (output folder) or `--senza-file` (no files).
- **Program B's notes** (`b/LEGGIMI.md`, in Italian) were written before the 7×7 run; its 7×7 output is
  `risultati/b_7x7.json`.
- **The reviewer** (`revisione/`) uses a Windows function to report memory use, so it builds only on Windows, and it
  writes its files in the current folder.
- **Comments in the code** sometimes cite planning documents in Italian (`PIANO_NUMBRIX_48H.md`, `SPEC_B.md`) that
  are not part of this repository.

## Limits

- These are computer results. For 7×7 they come from several independent programs.
- The 3-clue counts for 5×7, 5×8, 6×7, 6×8 and 7×8 come from program A only.
  - For 5×7, 5×8, 6×7 and 6×8 the minimum 3 does not depend on them: Hanson and Nash proved that 3 clues are enough
    there, and A and D find no defining pair.
  - For 7×8 the minimum 3 rests on A's examples, which were checked separately.
- I checked boards up to 7×8. I do not know yet what happens on larger boards.

## How this was made

I am an independent researcher. I did this work with AI assistants (Anthropic's Claude): they helped me find the
question, wrote and ran the programs under my direction, and helped me write this text. Two of the checks (program B
and the reviewer) were written by separate AI agents that did not see the other programs. I am responsible for what
is written here; if you find a mistake, please open an issue.

## License

Apache-2.0.
