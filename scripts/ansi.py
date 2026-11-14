#!/usr/bin/env python3
"""Reconstructor de pantalla desde la salida cruda de un pty (ANSI/VT100).

Uso:
    python3 ansi.py fichero.raw 120 40          # pantalla como texto
    python3 ansi.py fichero.raw 120 40 --json   # rejilla de celdas (para PNG)

Soporta: CUP/HVP, ED/EL, SGR (24-bit, 256, 16 colores, negrita), CR/LF/BS,
secuencias OSC y privadas (se ignoran). Celdas = (carácter, fg, bg, bold).
"""
import json
import sys
import unicodedata

# Paleta xterm-256 → RGB
def _x256(n):
    base = [
        (0, 0, 0), (128, 0, 0), (0, 128, 0), (128, 128, 0),
        (0, 0, 128), (128, 0, 128), (0, 128, 128), (192, 192, 192),
        (128, 128, 128), (255, 0, 0), (0, 255, 0), (255, 255, 0),
        (0, 0, 255), (255, 0, 255), (0, 255, 255), (255, 255, 255),
    ]
    if n < 16:
        return base[n]
    if n < 232:
        n -= 16
        r, g, b = n // 36, (n // 6) % 6, n % 6
        f = lambda v: 0 if v == 0 else 55 + v * 40
        return (f(r), f(g), f(b))
    v = 8 + (n - 232) * 10
    return (v, v, v)

FG16 = [
    (0, 0, 0), (205, 49, 49), (13, 188, 121), (229, 229, 16),
    (36, 114, 200), (188, 63, 188), (17, 168, 205), (229, 229, 229),
    (102, 102, 102), (241, 76, 76), (35, 209, 139), (245, 245, 67),
    (59, 142, 234), (214, 112, 214), (41, 184, 219), (255, 255, 255),
]


class Screen:
    def __init__(self, cols, rows, bg=(0, 0, 0)):
        self.cols, self.rows = cols, rows
        self.bg = bg
        self.reset()

    def reset(self):
        self.x = self.y = 0
        self.fg = None
        self.bg = self.bg0 = self.bg
        self.bold = False
        self.grid = [
            [(" ", None, self.bg, False) for _ in range(self.cols)]
            for _ in range(self.rows)
        ]

    def _scroll(self):
        self.grid.pop(0)
        self.grid.append([(" ", None, self.bg, False) for _ in range(self.cols)])

    def put(self, ch):
        if ch == "\n":
            self.y += 1
            self.x = 0
        elif ch == "\r":
            self.x = 0
        elif ch == "\b":
            self.x = max(0, self.x - 1)
        elif ch in ("\x07", "\x00"):
            pass
        else:
            if self.y >= self.rows:
                self._scroll()
                self.y = self.rows - 1
            if 0 <= self.x < self.cols and 0 <= self.y < self.rows:
                self.grid[self.y][self.x] = (ch, self.fg, self.bg, self.bold)
                # Carácter ancho: deja la siguiente celda como continuación.
                if unicodedata.east_asian_width(ch) in ("W", "F") and self.x + 1 < self.cols:
                    self.grid[self.y][self.x + 1] = ("", self.fg, self.bg, self.bold)
            self.x += 1

    def _sgr(self, params):
        i = 0
        nums = [p for p in params if p != ""]
        while i < len(nums):
            try:
                n = int(nums[i])
            except ValueError:
                i += 1
                continue
            if n == 0:
                self.fg, self.bg, self.bold = None, self.bg0, False
            elif n == 1:
                self.bold = True
            elif n in (21, 22):
                self.bold = False
            elif n == 39:
                self.fg = None
            elif n == 49:
                self.bg = self.bg0
            elif 30 <= n <= 37:
                self.fg = FG16[n - 30]
            elif 90 <= n <= 97:
                self.fg = FG16[n - 90 + 8]
            elif 40 <= n <= 47:
                self.bg = FG16[n - 40]
            elif 100 <= n <= 107:
                self.bg = FG16[n - 100 + 8]
            elif n in (38, 48):
                if i + 1 < len(nums) and nums[i + 1] == "2":
                    c = tuple(int(x) for x in nums[i + 2:i + 5])
                    if len(c) == 3:
                        if n == 38:
                            self.fg = c
                        else:
                            self.bg = c
                    i += 4
                elif i + 1 < len(nums) and nums[i + 1] == "5":
                    c = _x256(int(nums[i + 2]))
                    if n == 38:
                        self.fg = c
                    else:
                        self.bg = c
                    i += 2
            i += 1

    def feed(self, text):
        i = 0
        n = len(text)
        while i < n:
            ch = text[i]
            if ch != "\x1b":
                self.put(ch)
                i += 1
                continue
            if i + 1 >= n:
                break
            nxt = text[i + 1]
            if nxt == "[":
                j = i + 2
                while j < n and not ("@" <= text[j] <= "~"):
                    j += 1
                if j >= n:
                    break
                body, final = text[i + 2:j], text[j]
                params = body.lstrip("?").replace(":", ";").split(";")
                if final == "H" or final == "f":
                    r = int(params[0]) if params and params[0].isdigit() else 1
                    c = int(params[1]) if len(params) > 1 and params[1].isdigit() else 1
                    self.y, self.x = max(0, r - 1), max(0, c - 1)
                elif final == "A":
                    self.y = max(0, self.y - (int(params[0]) if params[0].isdigit() else 1))
                elif final == "B":
                    self.y += int(params[0]) if params[0].isdigit() else 1
                elif final == "C":
                    self.x += int(params[0]) if params[0].isdigit() else 1
                elif final == "D":
                    self.x = max(0, self.x - (int(params[0]) if params[0].isdigit() else 1))
                elif final == "G":
                    self.x = max(0, (int(params[0]) if params[0].isdigit() else 1) - 1)
                elif final == "d":
                    self.y = max(0, (int(params[0]) if params[0].isdigit() else 1) - 1)
                elif final == "J":
                    mode = int(params[0]) if params[0].isdigit() else 0
                    blank = [(" ", None, self.bg, False)] * self.cols
                    if mode == 2 or mode == 3:
                        self.grid = [list(blank) for _ in range(self.rows)]
                    elif mode == 0:
                        for x in range(self.x, self.cols):
                            self.grid[self.y][x] = (" ", None, self.bg, False)
                        for y in range(self.y + 1, self.rows):
                            self.grid[y] = list(blank)
                elif final == "K":
                    mode = int(params[0]) if params[0].isdigit() else 0
                    if mode == 0:
                        rng = range(self.x, self.cols)
                    elif mode == 1:
                        rng = range(0, self.x + 1)
                    else:
                        rng = range(0, self.cols)
                    for x in rng:
                        self.grid[self.y][x] = (" ", None, self.bg, False)
                elif final == "m":
                    self._sgr(params if body else ["0"])
                i = j + 1
            elif nxt == "]":
                j = i + 2
                while j < n and text[j] not in ("\x07",):
                    if text[j] == "\x1b" and j + 1 < n and text[j + 1] == "\\":
                        j += 1
                        break
                    j += 1
                i = j + 1
            elif nxt in "()#":
                i += 3
            elif nxt == "M":  # RI
                self.y = max(0, self.y - 1)
                i += 2
            else:
                i += 2

    def text(self, trim=True):
        lines = []
        for row in self.grid:
            s = "".join(c[0] for c in row)
            lines.append(s.rstrip() if trim else s)
        return "\n".join(lines)

    def cells(self):
        return [
            [[c[0], c[1], c[2], c[3]] for c in row] for row in self.grid
        ]


def load(path, cols, rows):
    raw = open(path, "rb").read().decode("utf-8", "replace")
    sc = Screen(cols, rows)
    sc.feed(raw)
    return sc


if __name__ == "__main__":
    if len(sys.argv) < 4:
        print(__doc__)
        sys.exit(1)
    path, cols, rows = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
    sc = load(path, cols, rows)
    if "--json" in sys.argv:
        print(json.dumps({"cols": cols, "rows": rows, "cells": sc.cells()}))
    else:
        print(sc.text())
