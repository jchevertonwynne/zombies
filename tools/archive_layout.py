#!/usr/bin/env python3
"""Lays out the Archive's windows, stairwells, lamps, fixed stacks, loop centres, Infills and path-node graph
(#531, #536), from the plan in #513. Writes src/maps/map/archive/data.rs; `archive.rs` builds the walls to
match it (its `RANGES` table and the pens round each window follow the numbers here).

    python3 tools/archive_layout.py

Every grid of nodes is laid out inside a Zone's box, and each edge between two of them is kept only
if the straight walk between them clears the rectangles in `BLOCKS` (the Props, the Bookcases, the
pens, the stairwells). The Doors, Ranges, stairs and windows are listed by hand below."""
import math
import os

ROOT = os.environ.get("ROOT", ".")
OUT = f"{ROOT}/src/maps/map/archive/data.rs"

NODES = []        # (name, x, y, z)
EDGES = []        # (a, b, gate source)
BY_NAME = {}
BLOCKS = []       # (x0, z0, x1, z1, level, tol): rectangles an edge at that level mustn't cross
NO_NODE = []      # (x0, z0, x1, z1, level, tol): where a grid puts no node (a Range's footprint and mouth strips)
ZONE_NODES = {}   # zone -> [names] of its grid


def node(name, x, y, z):
    assert name not in BY_NAME, name
    BY_NAME[name] = (x, y, z)
    NODES.append((name, round(x, 2), round(y, 2), round(z, 2)))
    return name


def seg_hits(ax, az, bx, bz, x0, z0, x1, z1):
    steps = max(2, int(math.hypot(bx - ax, bz - az) / 0.1))
    for i in range(steps + 1):
        t = i / steps
        x, z = ax + (bx - ax) * t, az + (bz - az) * t
        if x0 < x < x1 and z0 < z < z1:
            return True
    return False


def clear(a, b, margin=0.45):
    (ax, ay, az), (bx, by, bz) = BY_NAME[a], BY_NAME[b]
    for x0, z0, x1, z1, level, tol in BLOCKS:
        if abs(level - ay) > tol and abs(level - by) > tol:
            continue
        if seg_hits(ax, az, bx, bz, x0 - margin, z0 - margin, x1 + margin, z1 + margin):
            return False
    return True


def edge(a, b, gate="OPEN", check=True):
    if check and gate == "OPEN":
        assert clear(a, b), f"{a} -> {b} crosses a block"
    EDGES.append((a, b, gate))


def blocked(x, z, y, margin):
    for x0, z0, x1, z1, level, tol in BLOCKS:
        if abs(level - y) <= tol and x0 - margin < x < x1 + margin and z0 - margin < z < z1 + margin:
            return True
    for x0, z0, x1, z1, level, tol in NO_NODE:
        if abs(level - y) <= tol and x0 < x < x1 and z0 < z < z1:
            return True
    return False


def grid(zone, code, xs, zs, y, extra=()):
    names = {}
    for i, x in enumerate(xs):
        for j, z in enumerate(zs):
            if blocked(x, z, y, 0.3):
                continue
            names[(i, j)] = node(f"{code}_{i}_{j}", x, y, z)
    for (i, j), a in names.items():
        for di, dj in ((1, 0), (0, 1), (1, 1), (1, -1)):
            b = names.get((i + di, j + dj))
            if b and clear(a, b):
                EDGES.append((a, b, "OPEN"))
    ZONE_NODES.setdefault(zone, []).extend(names.values())
    return list(names.values())


def attach(name, zone, count=2, within=14.0):
    """Joins `name` to the nearest nodes of `zone`'s grid it can walk straight to."""
    x, y, z = BY_NAME[name]
    cands = sorted(ZONE_NODES[zone], key=lambda n: math.hypot(BY_NAME[n][0] - x, BY_NAME[n][2] - z) + 3 * abs(BY_NAME[n][1] - y))
    done = 0
    for c in cands:
        if c == name:
            continue
        if math.hypot(BY_NAME[c][0] - x, BY_NAME[c][2] - z) > within:
            break
        if abs(BY_NAME[c][1] - y) < 0.3 and clear(name, c):
            EDGES.append((name, c, "OPEN"))
            done += 1
            if done == count:
                return
    assert done, f"{name} reaches nothing in {zone}"


def rs(n):
    s = f"{n:.2f}".rstrip("0").rstrip(".")
    return s if "." in s else s + ".0"


# ---------------------------------------------------------------- the Zones' boxes (x0, x1, z0, z1, floor, top)
ZONES = {
    "CatalogueHall": (-20, 20, 0, 12, 8, 12),
    "ReadingRoom": (-20, 20, 12, 36, 8, 14),
    "MapRoom": (-36, -20, 12, 36, 8, 12),
    "LectureTheatre": (20, 36, 0, 24, 4, 12),
    "OldStacks": (-20, 0, 4, 20, 4, 8),
    "FolioStacks": (-20, 0, 20, 36, 4, 8),
    "PeriodicalStacks": (0, 20, 20, 36, 4, 8),
    "NewStacks": (0, 20, 4, 20, 4, 8),
    "Bindery": (20, 36, 24, 36, 4, 8),
    "WellStair": (-36, -20, 0, 12, 4, 8),
    "FloodedCellar": (-20, 20, 0, 20, 0, 4),
    "Undercroft": (-20, 0, 20, 36, 0, 4),
    "StrongRoom": (0, 20, 20, 36, 0, 4),
    "PumpRoom": (20, 36, 0, 24, 0, 4),
}

# ---------------------------------------------------------------- the Ranges (name, axis, line, rail, bookcases, start, gate face, gate zones)
# `axis` is the way the rail runs; the Bookcases are 1.2 along it and 5.0 across it, on `line`.
BOOK, AISLE, LONG, MOUTH = 1.2, 3.0, 5.0, 0.6
GATE_OFF = 1.4   # from a footprint's face to the middle of its gate's wall
RANGES = [
    # name, axis, line, rail, n, start, y, gate side (-1/+1 across), (zone low, zone high)
    ("r0", "x", 12.0, -3.9, 4, 2, 8.0, +1, ("CatalogueHall", "ReadingRoom")),
    ("ra", "x", 20.0, -14.3, 3, 0, 4.0, +1, ("OldStacks", "FolioStacks")),
    ("rb", "z", 0.0, 26.8, 2, 0, 4.0, +1, ("FolioStacks", "PeriodicalStacks")),
    ("rc", "x", 20.0, 7.7, 3, 3, 4.0, +1, ("NewStacks", "PeriodicalStacks")),
    ("rd", "z", 0.0, 7.8, 2, 2, 4.0, +1, ("OldStacks", "NewStacks")),
]


def rect(axis, line, along0, along1, across0, across1):
    """A rectangle (x0, z0, x1, z1) from along-the-rail and across-it extents."""
    if axis == "x":
        return (along0, line + across0, along1, line + across1)
    return (line + across0, along0, line + across1, along1)


def at(axis, line, along, across):
    return (along, line + across) if axis == "x" else (line + across, along)


# ---------------------------------------------------------------- Props: fixed stacks and loop centres
STACK_W, STACK_H, STACK_L = 1.2, 3.0, 5.0
FIXED = []   # (zone, cx, cz) a fixed stack, 1.2 across x and 5.0 along z
STACKS_X = {
    "OldStacks": [-11.3, -7.1], "FolioStacks": [-11.3, -7.1],
    "PeriodicalStacks": [12.2, 8.0], "NewStacks": [12.2, 8.0],
}
STACKS_Z = {"OldStacks": 11.4, "NewStacks": 11.4, "FolioStacks": 29.6, "PeriodicalStacks": 29.6}
for zone, xs in STACKS_X.items():
    for x in xs:
        FIXED.append((zone, x, STACKS_Z[zone]))

LOOP = []   # (kind, zone, cx, y, cz, w, h, d)
for x in (-12.5, 12.5):
    LOOP.append(("Counter", "CatalogueHall", x, 8.0, 6.5, 8.0, 1.5, 1.2))      # the two card-catalogue banks
LOOP.append(("Table", "ReadingRoom", 0.0, 8.0, 24.0, 6.0, 1.6, 6.0))           # the issue desk
LOOP.append(("Bench", "LectureTheatre", 28.0, 4.0, 11.0, 10.0, 5.4, 14.0))     # the theatre tiers
for x in (-12.0, -4.0, 4.0, 12.0):
    for z in (4.5, 15.5):
        LOOP.append(("Pillar", "FloodedCellar", x, 0.0, z, 0.8, 3.9, 0.8))     # the 8 brick piers
for x in (-15.0, -10.0, -5.0):
    for z in (23.5, 28.5, 33.5):
        LOOP.append(("Pillar", "Undercroft", x, 0.0, z, 0.6, 3.9, 0.6))        # the 3 x 3 columns

for zone, cx, cz in FIXED:
    BLOCKS.append((cx - STACK_W / 2, cz - STACK_L / 2, cx + STACK_W / 2, cz + STACK_L / 2, ZONES[zone][4], 0.3))
for kind, zone, cx, y, cz, w, h, d in LOOP:
    level, tol = (y, 0.3) if zone != "LectureTheatre" else (6.0, 2.5)
    BLOCKS.append((cx - w / 2, cz - d / 2, cx + w / 2, cz + d / 2, level, tol))

# The Infills (#536): 34 solid blocks of the building, from #515 section 7's placement intent moved off every Door, window landing,
# Buyable, Spawn Point, lamp, Range sweep, mouth strip and gate, the Training loops' 3 m lanes and the stairwells. (zone, name, x0, z0, x1, z1).
# The book-lift core is the four quarters on the Stacks level (x -3..3, z 17..23) with a pit under each Zone it crosses below; the
# Undercroft's and the Strong Room's are cut back from the Box Location and Pack-a-Punch.
INFILLS = [
    ("CatalogueHall", "quad-stair core", -20, 0, -16.5, 2.9), ("CatalogueHall", "flue breast", 19, 9.8, 20, 11.9),
    ("ReadingRoom", "wall-case nib", -20, 25.9, -19.2, 29.8), ("ReadingRoom", "stair-tower corner", 16.4, 32, 20, 36), ("ReadingRoom", "chimney breast", 19.2, 18, 20, 21),
    ("MapRoom", "plan-store cupboard", -24, 30.6, -20, 32.6), ("MapRoom", "pier", -35.8, 35.4, -34.6, 36),
    ("LectureTheatre", "projection box", 28, 0, 31, 0.5), ("LectureTheatre", "organ-loft nib", 34.7, 21.2, 35.8, 24),
    ("OldStacks", "core quarter", -3, 17, 0, 20), ("OldStacks", "ring pier", -14.6, 4, -13.1, 5.5), ("OldStacks", "vent riser", -10.5, 4, -9.5, 5),
    ("FolioStacks", "core quarter", -3, 20, 0, 23), ("FolioStacks", "ring pier", -16.6, 34.5, -15.1, 36), ("FolioStacks", "hoist shaft", -7.2, 34.2, -5.2, 36),
    ("PeriodicalStacks", "core quarter", 0, 20, 3, 23), ("PeriodicalStacks", "ring pier", 18.5, 34.7, 20, 36), ("PeriodicalStacks", "press chute", 7.1, 35.2, 8.5, 36),
    ("NewStacks", "core quarter", 0, 17, 3, 20), ("NewStacks", "ring pier", 18.5, 8.4, 20, 9.9), ("NewStacks", "service duct", 19.2, 10.6, 20, 12.6),
    ("Bindery", "chimney breast", 30.8, 35.2, 33.2, 36), ("Bindery", "stair-bay nib", 31.4, 24, 32.1, 26.5),
    ("WellStair", "newel wall", -33.5, 11.3, -25.5, 12),
    ("FloodedCellar", "lift pit", -3, 17, 3, 20), ("FloodedCellar", "culvert head", -20, 3.5, -17.6, 5.3), ("FloodedCellar", "pump sump", 17.5, 11.85, 20, 14.3),
    ("Undercroft", "lift pit", -3, 20, 0, 20.85), ("Undercroft", "wall respond", -20, 22.6, -19.4, 23.6),
    ("StrongRoom", "lift pit", 0, 20, 2.55, 23), ("StrongRoom", "vault-door reveal", 6.6, 20, 8.1, 21.2), ("StrongRoom", "book cage", 17, 33.7, 20, 36),
    ("PumpRoom", "boiler flue", 34.8, 6.8, 36, 10.8), ("PumpRoom", "coal store", 32, 0, 36, 3),
]
assert len(INFILLS) == 34
for zone, _, x0, z0, x1, z1 in INFILLS:
    level, tol = (ZONES[zone][4], 0.3) if zone != "LectureTheatre" else (6.0, 2.5)
    BLOCKS.append((x0, z0, x1, z1, level, tol))

# The Causeway (a Raised Floor over the bed) stops anyone walking across it at bed level.
CAUSEWAY = (-16.0, 8.5, 16.0, 11.5)
BLOCKS.append((*CAUSEWAY, 0.0, 0.3))

# ---------------------------------------------------------------- Range blocks: Bookcases, gate walls
RANGE_INFO = {}
for name, axis, line, rail, n, start, y, side, (zlo, zhi) in RANGES:
    aisle = rail + BOOK * start
    mid = aisle + AISLE / 2
    RANGE_INFO[name] = dict(axis=axis, line=line, rail=rail, n=n, start=start, y=y, mid=mid, side=side)
    # The Bookcases, shut against the open Aisle at its start Rest.
    for i in range(n):
        lo = rail + BOOK * i + (AISLE if i >= start else 0.0)
        BLOCKS.append((*rect(axis, line, lo, lo + BOOK, -LONG / 2, LONG / 2), y, 0.3))
    # Nothing but the chain stands in the sweep or on either mouth strip.
    NO_NODE.append((*rect(axis, line, rail - 0.1, rail + BOOK * n + AISLE + 0.1, -LONG / 2 - MOUTH - 0.1, LONG / 2 + MOUTH + 0.1), y, 0.3))
    # The gate's grille: a plane along the whole face, closed in by a cap at each end, with the Door (2.4 m) across the
    # Aisle's start Rest (`archive.rs`'s `range`). The Wheels' corridor behind it is the Door's alone.
    face = side * (LONG / 2)
    plane = side * (LONG / 2 + GATE_OFF)
    end = rail + BOOK * n + AISLE
    for lo, hi in ((rail - 0.2, mid - 1.2), (mid + 1.2, end + 0.2)):
        BLOCKS.append((*rect(axis, line, lo, hi, plane - 0.2, plane + 0.2), y, 0.3))
    for along in (rail, end):
        BLOCKS.append((*rect(axis, line, along - 0.2, along + 0.2, min(face, plane), max(face, plane)), y, 0.3))
    NO_NODE.append((*rect(axis, line, rail - 0.6, end + 0.6, -LONG / 2 - MOUTH - 0.1, max(face, plane) + 1.0), y, 0.3))

# ---------------------------------------------------------------- Windows: (zone, wall axis, wall line, along, y, out) one pen each
# `out` is the way from the zone towards its wall. The window stands PEN in from the wall, in a hatch wall 4 m wide,
# with a pen behind it (`archive.rs` builds the walls from `WINDOWS`).
PEN = 2.8
# How far in the inside node stands: a narrow lane between the pen and the stacks leaves less than the usual 1.8 m.
INSIDE = {}
WINDOW_DEFS = [
    ("CatalogueHall", "x", 0.0, -12.0, 8.0, (0, -1)), ("CatalogueHall", "x", 0.0, 12.0, 8.0, (0, -1)),
    ("ReadingRoom", "x", 36.0, -12.0, 8.0, (0, 1)), ("ReadingRoom", "x", 36.0, 0.0, 8.0, (0, 1)), ("ReadingRoom", "x", 36.0, 12.0, 8.0, (0, 1)),
    ("MapRoom", "z", -36.0, 18.0, 8.0, (-1, 0)), ("MapRoom", "z", -36.0, 26.0, 8.0, (-1, 0)),
    ("LectureTheatre", "x", 24.0, 22.4, 4.0, (0, 1)), ("LectureTheatre", "x", 24.0, 32.0, 4.0, (0, 1)),
    ("OldStacks", "z", -20.0, 6.0, 4.0, (-1, 0)), ("OldStacks", "z", -20.0, 15.0, 4.0, (-1, 0)),
    ("FolioStacks", "z", -20.0, 22.5, 4.0, (-1, 0)), ("FolioStacks", "z", -20.0, 28.5, 4.0, (-1, 0)),
    ("PeriodicalStacks", "z", 20.0, 22.5, 4.0, (1, 0)), ("PeriodicalStacks", "z", 20.0, 26.7, 4.0, (1, 0)),
    ("NewStacks", "z", 20.0, 6.0, 4.0, (1, 0)), ("NewStacks", "z", 20.0, 15.0, 4.0, (1, 0)),
    ("Bindery", "x", 36.0, 23.0, 4.0, (0, 1)), ("Bindery", "x", 36.0, 28.5, 4.0, (0, 1)),
    ("WellStair", "z", -36.0, 6.0, 4.0, (-1, 0)),
    ("FloodedCellar", "z", 20.0, 3.0, 0.0, (1, 0)), ("FloodedCellar", "z", 20.0, 17.0, 0.0, (1, 0)),
    ("Undercroft", "z", -20.0, 26.0, 0.0, (-1, 0)), ("Undercroft", "z", -20.0, 31.0, 0.0, (-1, 0)),
    ("StrongRoom", "z", 20.0, 25.0, 0.0, (1, 0)), ("StrongRoom", "z", 20.0, 31.0, 0.0, (1, 0)),
    ("PumpRoom", "x", 24.0, 25.0, 0.0, (0, 1)), ("PumpRoom", "x", 24.0, 30.0, 0.0, (0, 1)),
]
assert len(WINDOW_DEFS) == 28
WINDOWS = []
for zone, axis, wall, along, y, out in WINDOW_DEFS:
    # The window's centre is on the hatch wall, PEN in from the wall.
    if axis == "x":
        c = (along, wall - out[1] * PEN)
    else:
        c = (wall - out[0] * PEN, along)
    WINDOWS.append((zone, c[0], y, c[1], out))
    # The pen, hatch wall and all, is solid to the grid (the window's own nodes go in it).
    x, z = c
    if axis == "x":
        r = (x - 2.2, min(z, wall) - 0.0, x + 2.2, max(z, wall))
    else:
        r = (min(x, wall), z - 2.2, max(x, wall), z + 2.2)
    BLOCKS.append((*r, y, 0.3))

# ---------------------------------------------------------------- Stairs' wells and rails (blocks, as the floor sees them)
# The Map Stair: the Map Room's floor open over the corridor to D3, with a rail along its south edge.
BLOCKS.append((-34.0, 32.8, -20.0, 36.0, 8.0, 0.3))
BLOCKS.append((-34.0, 32.9, -20.0, 33.1, 8.0, 0.3))
# The Bindery Stair: open from z 24 to 32.2 along the east wall, with a rail along its west edge.
BLOCKS.append((32.4, 24.0, 36.0, 32.2, 4.0, 0.3))
BLOCKS.append((32.2, 24.0, 32.6, 32.2, 4.0, 0.3))
# The Well Stair: open along the east wall from z 0 to 9, with a rail along its west edge.
BLOCKS.append((-24.0, 0.0, -20.0, 9.0, 4.0, 0.3))
BLOCKS.append((-24.2, 0.0, -23.8, 9.0, 4.0, 0.3))
# The Well Stair's bed, the Map Stair's corridor and the Bindery Stair's foot, seen from below.
# The stairs' own nodes are laid by hand (they climb), so the beds are blocked only for their ramps' levels.

# ---------------------------------------------------------------- the Zones' grids
HALL_XS_OLD = [-18.4, -15.0, -9.2, -4.8]
HALL_XS_NEW = [18.4, 15.0, 10.1, 5.75]

CH = grid("CatalogueHall", "ch", [-18, -14, -10, -6, -2, 2, 6, 10, 14, 18], [4.4, 7.8, 9.6], 8.0)
RR = grid("ReadingRoom", "rr", [-17, -11, -5.5, 0, 5.5, 11, 17], [17.8, 21.5, 25.0, 28.8, 31.5], 8.0)
MR = grid("MapRoom", "mr", [-31, -26, -22], [14.5, 19.0, 23.5, 28.0, 31.5], 8.0)
OLD = grid("OldStacks", "ol", HALL_XS_OLD, [6.5, 11.4, 15.4], 4.0)
FOL = grid("FolioStacks", "fo", HALL_XS_OLD, [25.6, 29.6, 33.6], 4.0)
PER = grid("PeriodicalStacks", "pe", HALL_XS_NEW, [25.6, 29.6, 33.6], 4.0)
NEW = grid("NewStacks", "ne", HALL_XS_NEW, [6.5, 11.4, 15.4], 4.0)
BIN = grid("Bindery", "bi", [22.5, 27, 30.5], [26.0, 29.5, 32.0], 4.0)
WELL = grid("WellStair", "we", [-31, -27], [2.5, 6.5, 10.5], 4.0)
CELL = grid("FloodedCellar", "fc", [-18, -13.5, -9, -4.5, 0, 4.5, 9, 13.5, 18], [2.2, 6.8, 13.0, 17.8], 0.0)
UND = grid("Undercroft", "un", [-17.5, -12.5, -7.5, -2.5], [21.8, 26.0, 31.0, 34.5], 0.0)
STR = grid("StrongRoom", "st", [3, 8, 13], [24.5, 29.0, 33.5], 0.0)
PUMP = grid("PumpRoom", "pu", [23, 28, 33], [3.0, 8.0, 13.0, 18.0], 0.0)

# Corner nodes the grids' lanes leave out.
for zone, name, x, y, z in [
    ("OldStacks", "ol_c0", -16.0, 4.0, 18.4), ("OldStacks", "ol_c1", -4.8, 4.0, 18.2),
    ("FolioStacks", "fo_c0", -16.0, 4.0, 21.6), ("FolioStacks", "fo_c1", -4.8, 4.0, 21.4),
    ("PeriodicalStacks", "pe_c0", 16.0, 4.0, 21.6), ("PeriodicalStacks", "pe_c1", 5.75, 4.0, 21.4),
    ("NewStacks", "ne_c0", 16.0, 4.0, 18.4), ("NewStacks", "ne_c1", 5.75, 4.0, 18.2),
    ("OldStacks", "ol_s", -3.4, 4.0, 5.6), ("NewStacks", "ne_s", 3.4, 4.0, 5.6),
    ("FolioStacks", "fo_n", -3.4, 4.0, 34.4), ("PeriodicalStacks", "pe_n", 3.4, 4.0, 34.4),
    ("PumpRoom", "pu_c", 21.4, 0.0, 22.4),
    ("StrongRoom", "st_d", 14.5, 0.0, 21.8), ("StrongRoom", "st_c", 18.4, 0.0, 21.4),
]:
    node(name, x, y, z)
    ZONE_NODES[zone].append(name)
    attach(name, zone, 2, within=6.0)

# The pockets the Infills leave between themselves and a pen: a node in each, walked out of to the grid (the open-floor test).
node("ch_p0", -15.35, 8.0, 1.5)
node("ch_p1", -15.35, 8.0, 3.9)
edge("ch_p0", "ch_p1")
ZONE_NODES["CatalogueHall"] += ["ch_p0", "ch_p1"]
attach("ch_p1", "CatalogueHall", 2, within=6.0)
node("rr_p0", 15.3, 8.0, 34.6)
node("rr_p1", 15.3, 8.0, 31.4)
edge("rr_p0", "rr_p1")
ZONE_NODES["ReadingRoom"] += ["rr_p0", "rr_p1"]
attach("rr_p1", "ReadingRoom", 2, within=6.0)

# ---------------------------------------------------------------- the Lecture Theatre: crossover, aisles (ramps) and dais
LT_C = [node("lt_c0", 23.2, 8.0, 2.0), node("lt_c1", 28.5, 8.0, 2.0), node("lt_c2", 34.0, 8.0, 2.0)]
for a, b in zip(LT_C, LT_C[1:]):
    edge(a, b)
RAMP_Z = [5.2, 9.0, 12.8, 16.4]


def ramp_y(z):
    return 4.0 + (18.0 - z) * 4.0 / 14.0


for side, x, first in (("w", 21.5, "lt_c0"), ("e", 34.5, "lt_c2")):
    prev = first
    for k, z in enumerate(RAMP_Z):
        n = node(f"lt_{side}{k}", x, ramp_y(z), z)
        edge(prev, n, check=False)
        prev = n
    n = node(f"lt_{side}d", x, 4.0, 19.5)
    edge(prev, n, check=False)
DAIS = [f"lt_wd", node("lt_d1", 26.5, 4.0, 19.5), node("lt_d2", 30.5, 4.0, 19.5), "lt_ed"]
for a, b in zip(DAIS, DAIS[1:]):
    edge(a, b)
ZONE_NODES["LectureTheatre"] = LT_C + [n for n in BY_NAME if n.startswith("lt_")]

# ---------------------------------------------------------------- Doors
# name, wall axis ('x': wall along x at z=line; 'z': wall along z at x=line), line, position along it, the y of its floor,
# how far the nodes stand off the wall, the Zone on the low side and on the high.
DOORS = [
    ("d1", "z", 20.0, 2.0, 8.0, 1.8, "CatalogueHall", "LectureTheatre"),
    ("d2", "z", -20.0, 24.0, 8.0, 1.8, "MapRoom", "ReadingRoom"),
    ("d3", "z", -20.0, 34.5, 4.0, 1.8, "MapRoom", "FolioStacks"),
    ("d4", "x", 24.0, 26.0, 4.0, 1.8, "LectureTheatre", "Bindery"),
    ("d5", "z", 20.0, 30.0, 4.0, 1.8, "PeriodicalStacks", "Bindery"),
    ("d6", "z", -20.0, 10.4, 4.0, 1.8, "WellStair", "OldStacks"),
    ("d8", "x", 24.0, 34.2, 0.0, 1.4, "PumpRoom", "Bindery"),
    ("d9", "z", 20.0, 10.0, 0.0, 1.7, "FloodedCellar", "PumpRoom"),
    ("d10", "x", 20.0, -10.0, 0.0, 1.8, "FloodedCellar", "Undercroft"),
    ("pd", "x", 20.0, 10.0, 0.0, 1.8, "FloodedCellar", "StrongRoom"),
]
SPECIAL_DOOR_NODES = {"d8", "d9", "d3", "d6", "d1"}
for name, axis, line, pos, y, off, low, high in DOORS:
    if axis == "x":
        a, b = (pos, y, line - off), (pos, y, line + off)
    else:
        a, b = (line - off, y, pos), (line + off, y, pos)
    na = node(f"{name}_a", round(a[0], 2), y, round(a[2], 2))
    nb = node(f"{name}_b", round(b[0], 2), y, round(b[2], 2))
    edge(na, nb, f"Gate::Door(ZoneId::{low}, ZoneId::{high})", check=False)
    for n, zone in ((na, low), (nb, high)):
        if name == "d1" and n == nb:
            edge(nb, "lt_c0", check=False)
            edge(nb, "lt_w0", check=False)
            continue
        if name == "d8" and n == nb:
            continue   # the foot of the Bindery Stair, wired below
        if name == "d3" and n == na:
            continue   # the Map Stair's foot, wired below
        if name == "d6" and n == na:
            continue   # the Well Stair's landing, wired below
        if name == "d4" and n == na:
            edge(na, "lt_d1", check=False)
            continue
        if name == "d9" and n == na:
            continue   # the Causeway's east foot, wired below
        attach(n, zone, 2)

# The fixed archway from the Well Stair's foot into the Flooded Cellar.
node("arch_a", -21.5, 0.0, 1.3)
node("arch_b", -18.5, 0.0, 1.3)
edge("arch_a", "arch_b")
attach("arch_b", "FloodedCellar", 2, within=6.0)

# ---------------------------------------------------------------- the stairs (all straight flights)
# Well Stair: down south along x -22, from the landing at D6 (z 10.4, y 4) to the foot at z 2.6 (y 0).
edge_chain = [("d6_a", node("we_top", -22.0, 3.75, 8.6)), ("we_top", node("we_mid", -22.0, 2.0, 5.8)), ("we_mid", node("we_foot", -22.0, 0.0, 2.6)), ("we_foot", "arch_a")]
for a, b in edge_chain:
    edge(a, b, check=False)
attach("d6_a", "WellStair", 2, within=8.0)
ZONE_NODES["WellStair"] += ["d6_a"]
# Map Stair: down east along z 34.5 from the Map Room (x -35, y 8) to its foot at x -27.2 (y 4), then D3.
node("ms_land", -35.0, 8.0, 34.4)
node("ms_top", -33.4, 7.65, 34.5)
node("ms_mid", -30.6, 6.0, 34.5)
node("ms_foot", -27.2, 4.0, 34.5)
for a, b in [("ms_land", "ms_top"), ("ms_top", "ms_mid"), ("ms_mid", "ms_foot"), ("ms_foot", "d3_a")]:
    edge(a, b, check=False)
node("ms_w", -35.0, 8.0, 31.2)
edge("ms_w", "ms_land", check=False)
attach("ms_w", "MapRoom", 2, within=8.0)
ZONE_NODES["MapRoom"] += ["ms_w", "ms_land"]
# Bindery Stair: up north along x 34.2 from D8's foot at z 25.9 (y 0) to the Bindery's floor at z 33.4 (y 4).
node("bs_mid", 34.2, 2.0, 28.8)
node("bs_top", 34.2, 3.76, 31.8)
node("bs_land", 34.2, 4.0, 33.4)
for a, b in [("d8_b", "bs_mid"), ("bs_mid", "bs_top"), ("bs_top", "bs_land")]:
    edge(a, b, check=False)
node("bi_n", 31.6, 4.0, 34.4)
ZONE_NODES["Bindery"].append("bi_n")
edge("bs_land", "bi_n")
attach("bi_n", "Bindery", 2, within=8.0)
# The Causeway (3 m wide, 0.6 m up) with steps at both ends: its nodes, and its feet at D9 and the west end.
CZ = [node(f"cw_{i}", x, 0.6, 10.0) for i, x in enumerate([-15.2, -11.0, -6.0, -2.0, 2.0, 6.0, 11.0, 15.2])]
for a, b in zip(CZ, CZ[1:]):
    edge(a, b)
node("cw_w_foot", -18.3, 0.0, 10.0)
edge("cw_w_foot", CZ[0], check=False)
edge("d9_a", CZ[-1], check=False)
for n in ("cw_w_foot", "d9_a"):
    attach(n, "FloodedCellar", 2, within=6.0)

# ---------------------------------------------------------------- the Ranges' chains: mouth - Aisle - mouth
for k, (name, axis, line, rail, n, start, y, side, (zlo, zhi)) in enumerate(RANGES):
    info = RANGE_INFO[name]
    pos = lambda along, across: (lambda p: (round(p[0], 2), y, round(p[1], 2)))(at(axis, line, along, across))
    first_m1 = None
    # One chain for each Rest: the mouth, the Aisle and the mouth, each edge open only while the Range stands there.
    for r in range(n + 1):
        mid_r = rail + BOOK * r + AISLE / 2
        gate = f"rest({k}, {r})"
        m_lo = node(f"{name}_m0_{r}", *pos(mid_r, -(LONG / 2 + MOUTH)))
        ai = node(f"{name}_ai_{r}", *pos(mid_r, 0.0))
        m_hi = node(f"{name}_m1_{r}", *pos(mid_r, LONG / 2 + MOUTH))
        edge(m_lo, ai, gate, check=False)
        edge(ai, m_hi, gate, check=False)
        attach(m_lo, zlo, 2, within=8.0)
        # The high mouths stand in the gate's corridor, side by side: along it to the Door.
        if r > 0:
            edge(f"{name}_m1_{r - 1}", m_hi, check=False)
    # The gate is on the high side: a node beyond it, and the Door between that and the mouth across the start Rest.
    mid = info["mid"]
    g = node(f"{name}_g", *pos(mid, LONG / 2 + GATE_OFF + 0.2 + 0.9))
    edge(g, f"{name}_m1_{start}", f"Gate::Door(ZoneId::{zlo}, ZoneId::{zhi})", check=False)
    attach(g, zhi, 2, within=8.0)

# ---------------------------------------------------------------- Windows
for i, (zone, x, y, z, out) in enumerate(WINDOWS):
    outside = node(f"w{i}_out", x + out[0] * 2.0, y, z + out[1] * 2.0)
    back = INSIDE.get(zone, 1.8)
    inside = node(f"w{i}_in", x - out[0] * back, y, z - out[1] * back)
    edge(outside, inside, f"barrier({i})", check=False)
    attach(inside, zone, 2, within=10.0)

# ---------------------------------------------------------------- Vents (a Nova Crawler's drop, then its floor spot)
VENTS = [("ReadingRoom", -11.0, 8.0, 28.8), ("LectureTheatre", 26.5, 4.0, 19.5), ("FolioStacks", -13.4, 4.0, 25.6), ("NewStacks", 14.3, 4.0, 15.4)]
for i, (zone, x, y, z) in enumerate(VENTS):
    floor = node(f"vf{i}", x, y, z)
    attach(floor, zone, 3, within=6.0)
    vent = node(f"vent_{i}", x, y + 3.5, z)
    edge(vent, floor, "Gate::OneWay", check=False)

# ---------------------------------------------------------------- Lamps
LAMPS = []   # (zone, x, y, z)


def lamps(zone, y, xs, zs):
    for x in xs:
        for z in zs:
            LAMPS.append((zone, x, y, z))


lamps("CatalogueHall", 11.85, [-15, -5, 5, 15], [3.2, 9.6])
lamps("ReadingRoom", 11.5, [-15, -5, 5, 15], [17.0, 24.0, 31.0])
lamps("MapRoom", 11.85, [-31, -25], [16, 24, 32])
lamps("LectureTheatre", 11.85, [25, 31], [4.5, 12, 20])
lamps("OldStacks", 7.85, [-16.0, -9.2, -4.8], [6.5, 15.4])
lamps("FolioStacks", 7.85, [-16.0, -9.2, -4.8], [25.6, 33.6])
lamps("PeriodicalStacks", 7.85, [16.0, 10.1, 5.2], [25.6, 33.6])
lamps("NewStacks", 7.85, [16.0, 10.1, 5.75], [6.5, 15.4])
lamps("Bindery", 7.85, [24.0, 31], [27, 33])
lamps("WellStair", 7.85, [-30, -24], [3, 9])
lamps("FloodedCellar", 3.85, [-16, -8, 0, 8, 16], [5, 15])
lamps("Undercroft", 3.85, [-14, -5], [25, 32])
lamps("StrongRoom", 3.85, [6, 14], [25, 32])
lamps("PumpRoom", 3.85, [25, 31], [4, 12, 20])
assert len(LAMPS) == 88, len(LAMPS)


def check_lamps():
    bad = []
    for zone, (x0, x1, z0, z1, fl, top) in ZONES.items():
        mine = [(x, z) for z_, x, y, z in LAMPS if z_ == zone]
        x = x0 + 1.0
        while x <= x1 - 1.0 + 1e-9:
            z = z0 + 1.0
            while z <= z1 - 1.0 + 1e-9:
                if min(math.hypot(x - lx, z - lz) for lx, lz in mine) > 6.5:
                    bad.append((zone, round(x, 2), round(z, 2)))
                z += 0.25
            x += 0.25
    assert not bad, bad[:10]
    # No lamp over a fixed stack or in a Range's sweep, mouth strips or gate.
    for zone, x, y, z in LAMPS:
        for bx0, bz0, bx1, bz1, *_ in [(cx - STACK_W / 2, cz - STACK_L / 2, cx + STACK_W / 2, cz + STACK_L / 2, 0, 0) for zn, cx, cz in FIXED if zn == zone]:
            assert not (bx0 - 0.3 < x < bx1 + 0.3 and bz0 - 0.3 < z < bz1 + 0.3), (zone, x, z)
        for nx0, nz0, nx1, nz1, level, tol in NO_NODE:
            assert not (abs(level - ZONES[zone][4]) <= tol and nx0 < x < nx1 and nz0 < z < nz1), ("lamp in a Range", zone, x, z)


check_lamps()

# ---------------------------------------------------------------- a check of the open floor, as the headless test walks it
PROPS_XZ = [(cx - STACK_W / 2, cz - STACK_L / 2, cx + STACK_W / 2, cz + STACK_L / 2, ZONES[z][4]) for z, cx, cz in FIXED]


def coverage():
    bad = []
    for zone, (x0, x1, z0, z1, fl, top) in ZONES.items():
        if zone in ("LectureTheatre",):
            continue
        names = ZONE_NODES[zone]
        x = x0 + 0.75
        while x < x1:
            z = z0 + 0.75
            while z < z1:
                z += 1.5
                zz = z - 1.5
                if blocked(x, zz, fl, 0.35 + 0.05):
                    continue
                if zone == "FloodedCellar" and CAUSEWAY[0] < x < CAUSEWAY[2] and CAUSEWAY[1] < zz < CAUSEWAY[3]:
                    continue
                # spots over the stairwells have no floor at the zone's height
                if (zone == "MapRoom" and -34.0 < x < -20.0 and zz > 32.8) or (zone == "Bindery" and x > 32.4 and zz < 32.2) or (zone == "WellStair" and -24.0 < x and zz < 9.0):
                    continue
                own = sorted(names, key=lambda m: math.hypot(BY_NAME[m][0] - x, BY_NAME[m][2] - zz) + 3 * abs(BY_NAME[m][1] - fl))[:6]
                ok = False
                for m in own:
                    mx, my, mz = BY_NAME[m]
                    if abs(my - fl) > 0.5:
                        continue
                    if not any(abs(l - fl) <= t and seg_hits(x, zz, mx, mz, bx0 - 0.3, bz0 - 0.3, bx1 + 0.3, bz1 + 0.3) for bx0, bz0, bx1, bz1, l, t in BLOCKS):
                        ok = True
                        break
                if not ok:
                    bad.append((zone, round(x, 2), round(zz, 2)))
            x += 1.5
    return bad


if os.environ.get("COVERAGE"):
    for b in coverage():
        print("uncovered", b)

# ---------------------------------------------------------------- write
out = []
out.append("//! The Archive's windows, stairwells, lamps, fixed stacks, loop centres and path-node graph, laid out from\n//! `archive.rs`'s walls and Ranges by `tools/archive_layout.py` (rewritten by it: edit the script, not this file).\n")
out.append("use bevy::prelude::*;\n\nuse crate::enemies::nav::Gate;\nuse crate::maps::map::{BarrierId, Infill, Prop, PropKind, Sunken, infill, prop};\nuse crate::maps::shifting::PieceId;\nuse crate::maps::zone::ZoneId;\n\nuse super::W0;\n")
out.append("""const fn v(x: f32, z: f32) -> Vec2 {
    Vec2::new(x, z)
}

const fn barrier(i: u8) -> Gate {
    Gate::Barrier(BarrierId(W0 + i))
}

const OPEN: Gate = Gate::Open;

const fn rest(piece: u8, rest: u8) -> Gate {
    Gate::Rest(PieceId(piece), rest)
}

const fn window(zone: ZoneId, x: f32, y: f32, z: f32, out: Vec2) -> (ZoneId, Vec3, Vec2) {
    (zone, Vec3::new(x, y, z), out)
}

const fn p(kind: PropKind, zone: ZoneId, x: f32, y: f32, z: f32, size: (f32, f32, f32)) -> Prop {
    prop(kind, zone, (x, y, z), size)
}

""")
out.append("""/// The stairwells and the Flooded Cellar's pool bed: the Map Stair's corridor in the Map Room's floor, the Bindery
/// Stair's and the Well Stair's, each a storey deep, and the pool bed `POOL_DEPTH` under the Cellar's nominal floor.
pub(super) const SUNKEN: [Sunken; 4] = [
    Sunken { zone: ZoneId::MapRoom, min: v(-34.0, 33.0), max: v(-20.0, 36.0), depth: 4.0, rim: false },
    Sunken { zone: ZoneId::Bindery, min: v(32.4, 24.0), max: v(36.0, 32.2), depth: 4.0, rim: false },
    Sunken { zone: ZoneId::WellStair, min: v(-24.0, 0.0), max: v(-20.0, 9.0), depth: 4.0, rim: false },
    Sunken { zone: ZoneId::FloodedCellar, min: v(-20.0, 0.0), max: v(20.0, 20.0), depth: 0.5, rim: false },
];
""")
out.append("/// The 28 windows (boarded service hatches), each the home of one Barrier, in the order the path nodes' `w<n>_out` count\n/// them. Each stands in a hatch wall `PEN` in from the wall it faces, with a pen of floor behind it.\npub(in crate::maps::map) const WINDOWS: [(ZoneId, Vec3, Vec2); 28] = [\n")
for zone, x, y, z, o in WINDOWS:
    out.append(f"    window(ZoneId::{zone}, {rs(x)}, {rs(y)}, {rs(z)}, v({rs(o[0])}, {rs(o[1])})),\n")
out.append("];\n\n")
out.append(f"/// The lamps ({len(LAMPS)}: a grid per Zone whose cells reach `LAMP_REACH`, hung over the lanes and never over a fixed stack or in a Range).\npub(super) const LAMPS: [(ZoneId, Vec3); {len(LAMPS)}] = [\n")
for zone, x, y, z in LAMPS:
    out.append(f"    (ZoneId::{zone}, Vec3::new({rs(x)}, {rs(y)}, {rs(z)})),\n")
out.append("];\n\n")
out.append(f"/// The fixed stacks: tall shelves, 1.2 x 5 m, bolted down in the ring halls on no rail ({len(FIXED)} of them), `Stack` Props (`stacks_look`).\npub(super) const FIXED_STACKS: [Prop; {len(FIXED)}] = [\n")
for zone, cx, cz in FIXED:
    out.append(f"    p(PropKind::Stack, ZoneId::{zone}, {rs(cx)}, {rs(ZONES[zone][4])}, {rs(cz)}, ({rs(STACK_W)}, {rs(STACK_H)}, {rs(STACK_L)})),\n")
out.append("];\n\n")
out.append(f"/// The Training loops' centres, as plain boxes: the two catalogue banks, the issue desk, the theatre tiers, the 8 brick piers and the\n/// Undercroft's 3 x 3 columns.\npub(super) const LOOP_CENTRES: [Prop; {len(LOOP)}] = [\n")
for kind, zone, cx, y, cz, w, h, d in LOOP:
    out.append(f"    p(PropKind::{kind}, ZoneId::{zone}, {rs(cx)}, {rs(y)}, {rs(cz)}, ({rs(w)}, {rs(h)}, {rs(d)})),\n")
out.append("];\n\n")
out.append(f"/// The Infills ({len(INFILLS)}: one to three in every Zone, #515 section 7's 34 moved off what the Zone keeps clear; the lift pits stand on the pool's bed).\npub(super) const INFILLS: [Infill; {len(INFILLS)}] = [\n")
for zone, name, x0, z0, x1, z1 in INFILLS:
    out.append(f"    // {name}\n    infill(ZoneId::{zone}, ({rs(x0)}, {rs(z0)}), ({rs(x1)}, {rs(z1)})),\n")
out.append("];\n\n")
out.append("pub(super) const NODES: &[(&str, f32, f32, f32)] = &[\n")
for name, x, y, z in NODES:
    out.append(f'    ("{name}", {rs(x)}, {rs(y)}, {rs(z)}),\n')
out.append("];\n\n")
out.append("pub(super) const EDGES: &[(&str, &str, Gate)] = &[\n")
for a, b, gate in EDGES:
    out.append(f'    ("{a}", "{b}", {gate}),\n')
out.append("];\n")
os.makedirs(os.path.dirname(OUT), exist_ok=True)
open(OUT, "w").write("".join(out))
print(f"{len(NODES)} nodes, {len(EDGES)} edges, {len(LAMPS)} lamps -> {OUT}")
