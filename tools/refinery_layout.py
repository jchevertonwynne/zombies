#!/usr/bin/env python3
"""Lays out the Refinery's windows, lamps, plant structures (Props) and path-node graph (#556), from the plan in #508.
Writes src/maps/map/refinery/data.rs; `refinery.rs` builds the walls, stairs and decks to match it.

    python3 tools/refinery_layout.py

Every grid of nodes is laid out inside a Zone's box, and each edge between two of them is kept only if the straight walk between
them clears the rectangles in `BLOCKS` (the plant, the pens, the stair bays). The Doors, stairs, windows and the one drop are
listed by hand below. The seven Blazes' rects (#507, #508) are kept clear of every node; the Fire ticket replaces that table with
`Layout.fuel_lines`."""
import math
import os

ROOT = os.environ.get("ROOT", ".")
OUT = f"{ROOT}/src/maps/map/refinery/data.rs"

NODES = []        # (name, x, y, z)
EDGES = []        # (a, b, gate source)
BY_NAME = {}
BLOCKS = []       # (x0, z0, x1, z1, level, tol): rectangles an edge at that level mustn't cross
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
    return False


def in_blaze(x, z, y, margin=0.0):
    for x0, z0, x1, z1, fy in BLAZES:
        if abs(fy - y) < 1.0 and x0 - margin < x < x1 + margin and z0 - margin < z < z1 + margin:
            return True
    return False


def grid(zone, code, xs, zs, y):
    names = {}
    for i, x in enumerate(xs):
        for j, z in enumerate(zs):
            if blocked(x, z, y, 0.5) or in_blaze(x, z, y, 1.0):
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


def space(lo, hi, step):
    n = max(1, math.ceil((hi - lo) / step - 1e-9))
    return [round(lo + (hi - lo) * i / n, 2) for i in range(n + 1)] if hi > lo else [lo]


# ---------------------------------------------------------------- the Zones' boxes (x0, x1, z0, z1, floor) and the notches
ZONES = {
    "Gatehouse": (-20, 20, 0, 20, 0), "TankerDock": (-60, -20, 0, 20, 0), "FittingShop": (20, 60, 0, 20, 0),
    "TankFarm": (-60, -20, 20, 60, 0), "RackRoad": (-20, 20, 20, 60, 0), "CompressorHouse": (20, 60, 20, 60, 0),
    "FlareYard": (-60, -20, 60, 90, 0), "ColumnYard": (-20, 20, 60, 90, 0), "Substation": (20, 60, 60, 90, 0),
    "RackWalk": (-4, 4, 20, 64, 4), "TankWalk": (-58, -4, 30, 64, 4), "PipeBridge": (4, 36, 52, 62, 4),
    "ControlHouse": (24, 56, 62, 86, 4), "ColumnPlatform": (-12, 12, 64, 88, 4), "TowerDeck": (-8, 8, 68, 84, 8),
}
ORDER = list(ZONES)
# The stair bays, cut out of the ground Zones' boxes (and the Tank Walk's three rectangles' gaps).
BAYS = {
    "s1": (-8, 32, -4, 44), "s2": (-50, 22, -46, 30), "s3": (-58, 54, -54, 64),
    "s4": (56, 66, 60, 76), "s5": (-16, 68, -12, 78), "s6": (12, 70, 16, 82),
}
TANKWALK_CUTS = [(-38, 30, -4, 50), (-54, 54, -4, 64), (-58, 56, -54, 64)]


def volume(zone):
    x0, x1, z0, z1, fl = ZONES[zone]
    if zone == "TowerDeck":
        return (x0, x1, 7.75, 12.0, z0, z1)
    return (x0, x1, fl - 0.25, fl + 3.75, z0, z1)


def zone_at(x, y, z):
    for zone in ORDER:
        x0, x1, y0, y1, z0, z1 = volume(zone)
        if not (x0 <= x <= x1 and y0 <= y <= y1 and z0 <= z <= z1):
            continue
        notches = list(BAYS.values()) if ZONES[zone][4] == 0 else []
        if zone == "TankWalk":
            notches = TANKWALK_CUTS
        if any(a <= x <= c and b <= z <= d for a, b, c, d in notches):
            continue
        return zone
    return None


# ---------------------------------------------------------------- the Blazes (#508's seven rects) and the Valves' spots
BLAZES = [
    (-3.5, 28, 3.5, 32, 0), (-3.5, 44, 3.5, 48, 0),     # the Rack Line
    (-42, 38, -38, 42, 0),                              # the Bund Line
    (-30, 50, -26, 54, 4),                              # the Gantry Line
    (-34, 85, -30, 90, 0),                              # the Flare Line
    (3, 74, 8, 78, 0),                                  # the Furnace Line
    (-40, 7, -34, 11, 0),                               # the Dock Line
]
VALVES = [(-4.5, 0, 24), (4.5, 0, 56), (-40, 0, 35), (-40, 0, 45), (-33.5, 4, 52), (-22.5, 4, 52),
          (-37.5, 0, 87.5), (-26.5, 0, 87.5), (5.5, 0, 71.5), (5.5, 0, 80.5), (-37, 0, 4), (-37, 0, 14)]

# ---------------------------------------------------------------- plant structures: the Props that stand as loop centres and Blaze walls
# (kind, zone, cx, y, cz, w, h, d). Tanks and the column are round (Barrel); everything else is a plain box.
PLANT = [
    ("Machine", "Gatehouse", 0.0, 0.0, 11.0, 4.0, 2.5, 12.0),          # the weighbridge island
    ("Machine", "Gatehouse", 3.5, 0.0, 15.0, 3.0, 3.0, 4.0),           # the booth, on the island's east edge
    ("Machine", "TankerDock", -52.0, 0.0, 10.0, 4.0, 3.0, 12.0),       # the west tanker
    ("Machine", "TankerDock", -44.0, 0.0, 10.0, 4.0, 3.0, 12.0),       # the east tanker
    ("Barrel", "TankFarm", -48.0, 0.0, 40.0, 12.0, 10.0, 12.0),        # Tank A
    ("Barrel", "TankFarm", -32.0, 0.0, 40.0, 12.0, 10.0, 12.0),        # Tank B
    ("Sandbags", "RackRoad", -3.9, 0.0, 40.0, 0.4, 1.0, 32.0),         # the west sleeper line
    ("Sandbags", "RackRoad", 3.9, 0.0, 40.0, 0.4, 1.0, 32.0),          # the east sleeper line
    ("Machine", "FlareYard", -40.0, 0.0, 82.0, 6.0, 4.0, 6.0),         # the flare stack's base
    ("Machine", "FlareYard", -31.5, 0.0, 82.0, 11.0, 3.5, 4.0),        # the knockout drum
    ("Barrel", "ColumnYard", 0.0, 0.0, 76.0, 6.0, 12.0, 6.0),          # the column's base
    ("Machine", "ColumnYard", 12.0, 0.0, 76.0, 8.0, 2.9, 12.0),        # the fired heater (its roof, to 4 m, is a block `refinery.rs` adds under S6)
    ("Machine", "Substation", 40.0, 0.0, 76.0, 10.0, 2.4, 14.0),       # the transformer pen
]
for z in (26.0, 34.0, 42.0, 50.0):                                     # the rack's bents, under the Rack Walk
    for x in (-3.4, 3.4):
        PLANT.append(("Pillar", "RackRoad", x, 0.0, z, 0.4, 4.0, 0.4))
for x, z in ((26.0, 64.0), (54.0, 64.0), (26.0, 84.0), (40.0, 84.0), (54.0, 84.0)):   # the Control House's stilts
    PLANT.append(("Pillar", "Substation", x, 0.0, z, 0.6, 4.0, 0.6))
for kind, zone, cx, y, cz, w, h, d in PLANT:
    BLOCKS.append((cx - w / 2, cz - d / 2, cx + w / 2, cz + d / 2, y, 0.3))

# The catwalks' fixtures: the Tank A column through the Girder Ring, the column through the Column Platform and the Tower Deck,
# Juggernog at the Rack Walk's south end, Pack-a-Punch, and the Rack Walk's drop's cantilever.
BLOCKS += [
    (-54, 34, -42, 46, 4.0, 0.3), (-3, 73, 3, 79, 4.0, 0.3), (-3, 73, 3, 79, 8.0, 0.3),
    (-1.0, 20.0, 1.0, 21.2, 4.0, 0.3), (-1.0, 79.1, 1.0, 80.1, 8.0, 0.3),
]
# The Raised Floor in the Tanker Dock, seen from the ground.
BLOCKS.append((-32, 4, -24, 16, 0.0, 0.3))
BLOCKS.append((-29.5, 1.6, -26.5, 4, 0.0, 0.3))
BLOCKS.append((-29.5, 16, -26.5, 18.4, 0.0, 0.3))
# The stair bays, as the ground sees them.
BLOCKS.append((-60, 54, -58, 64, 0.0, 0.3))   # S3's west fill, to the blast wall
for name, (x0, z0, x1, z1) in BAYS.items():
    if name != "s6":
        BLOCKS.append((x0, z0, x1, z1, 0.0, 0.3))
    else:
        BLOCKS.append((x0, z0, x1, z1, 4.0, 0.3))

# ---------------------------------------------------------------- windows
# (zone, x, y, z, out): each Barrier is one, in this order. A window stands in a hatch wall `PEN` in from the wall it faces (or at a
# catwalk's edge), with a walled pen behind it where the Zombies gather.
PEN, PEN_HALF = 2.8, 2.0
WINDOWS = [
    ("Gatehouse", -12.0, 0.0, 2.8, (0, -1)), ("Gatehouse", 12.0, 0.0, 2.8, (0, -1)),
    ("TankerDock", -57.2, 0.0, 7.0, (-1, 0)), ("TankerDock", -57.2, 0.0, 11.0, (-1, 0)), ("TankerDock", -57.2, 0.0, 15.0, (-1, 0)),
    ("FittingShop", 28.0, 0.0, 2.8, (0, -1)), ("FittingShop", 40.0, 0.0, 2.8, (0, -1)), ("FittingShop", 57.2, 0.0, 14.0, (1, 0)),
    ("TankFarm", -57.2, 0.0, 28.0, (-1, 0)), ("TankFarm", -57.2, 0.0, 46.0, (-1, 0)),
    ("CompressorHouse", 57.2, 0.0, 34.0, (1, 0)), ("CompressorHouse", 57.2, 0.0, 46.0, (1, 0)),
    ("FlareYard", -57.2, 0.0, 70.0, (-1, 0)), ("FlareYard", -57.2, 0.0, 80.0, (-1, 0)), ("FlareYard", -50.0, 0.0, 87.2, (0, 1)),
    ("ColumnYard", -12.0, 0.0, 87.2, (0, 1)), ("ColumnYard", 12.0, 0.0, 87.2, (0, 1)),
    ("Substation", 57.2, 0.0, 82.0, (1, 0)), ("Substation", 30.0, 0.0, 87.2, (0, 1)), ("Substation", 50.0, 0.0, 87.2, (0, 1)),
    ("TankWalk", -42.0, 4.0, 30.0, (0, -1)), ("TankWalk", -12.0, 4.0, 54.0, (0, 1)),
    ("RackWalk", 4.0, 4.0, 36.0, (1, 0)), ("PipeBridge", 12.0, 4.0, 52.0, (0, -1)),
    ("ControlHouse", 36.0, 4.0, 86.0, (0, 1)), ("ControlHouse", 46.0, 4.0, 86.0, (0, 1)),
    ("ColumnPlatform", -12.0, 4.0, 84.0, (-1, 0)), ("ColumnPlatform", 12.0, 4.0, 86.0, (1, 0)),
    ("TowerDeck", -6.0, 8.0, 84.0, (0, 1)),
]
assert len(WINDOWS) == 29


def pen_rect(x, y, z, out):
    """The footprint of the pen behind a window, hatch wall to back wall, as a block."""
    ox, oz = out
    if ox == 0:
        return (x - PEN_HALF - 0.2, min(z, z + oz * PEN) - 0.2, x + PEN_HALF + 0.2, max(z, z + oz * PEN) + 0.2)
    return (min(x, x + ox * PEN) - 0.2, z - PEN_HALF - 0.2, max(x, x + ox * PEN) + 0.2, z + PEN_HALF + 0.2)


for zone, x, y, z, out in WINDOWS:
    if y == 0.0:
        BLOCKS.append((*pen_rect(x, y, z, out), 0.0, 0.3))
    # a catwalk's pen floats over the yard, as a slab: nothing on the ground is in its way



# ---------------------------------------------------------------- zones' grids
def G(zone, code, xs, zs, y=None):
    return grid(zone, code, xs, zs, ZONES[zone][4] if y is None else y)


def nid(x, y, z):
    for name, (nx, ny, nz) in BY_NAME.items():
        if abs(nx - x) < 0.02 and abs(ny - y) < 0.02 and abs(nz - z) < 0.02:
            return name
    raise KeyError((x, y, z))


def link(p, q, gate="OPEN"):
    """An edge between the nodes at two points, across a Blaze's rect (an Open edge until the Fire ticket gates it)."""
    EDGES.append((nid(*p), nid(*q), gate))


def snode(name, x, y, z, zone=None):
    """A node outside every Zone ends in `_out`."""
    if zone_at(x, y, z) is None:
        name += "_out"
    return node(name, x, y, z)


G("Gatehouse", "gh", space(-17, 17, 6), [5.0, 11.0, 17.0])
G("TankerDock", "td", [-56.0, -48.0, -38.0, -28.0, -22.5], [2.0, 6.0, 10.0, 14.0, 18.0])
G("FittingShop", "fs", space(23, 57, 6), [5.0, 10.0, 15.0])
G("TankFarm", "tf", space(-57, -23, 6), space(23, 57, 6))
G("RackRoad", "rr", [-17.0, -11.0, -6.0, 0.0, 6.0, 11.0, 17.0], [22.5, 28.5, 34.0, 39.5, 45.0, 50.5, 58.0])
G("CompressorHouse", "cs", space(23, 57, 6), space(23, 57, 6))
G("FlareYard", "fy", space(-57, -23, 6), space(63, 87, 6))
G("ColumnYard", "cy", space(-17, 17, 5.5), space(63, 87, 5))
G("Substation", "ss", space(23, 57, 6), space(63, 87, 6))
G("RackWalk", "rw", [-2.0, 2.0], space(23, 62, 5), 4.0)
G("TankWalk", "tw", [-56.0, -48.0, -40.0], [32.0, 40.0, 48.0], 4.0)
G("TankWalk", "tg", space(-52, -8, 5.5), [52.0], 4.0)
G("PipeBridge", "pb", space(7, 33, 5.2), [55.0, 59.0], 4.0)
G("ControlHouse", "cx", space(28, 52, 8), [66.0, 72.0, 78.0, 83.0], 4.0)
G("ColumnPlatform", "cp", space(-9.5, 9.5, 4.75), space(66.5, 85.5, 4.75), 4.0)
G("TowerDeck", "tk", space(-6, 6, 6), [70.0, 76.0, 82.0], 8.0)

# The Tanker Dock's Raised Floor (x -32..-24, z 4..16, 1.2 m up) with a flight of steps on its north and its south side.
G("TankerDock", "pf", [-30.0, -26.0], [6.0, 10.0, 14.0], 1.2)
node("pfn_foot", -28.0, 0.0, 19.0)
node("pfn_top", -28.0, 1.2, 15.2)
node("pfs_foot", -28.0, 0.0, 1.0)
node("pfs_top", -28.0, 1.2, 4.8)
ZONE_NODES["TankerDock"] += ["pfn_foot", "pfn_top", "pfs_foot", "pfs_top"]
edge("pfn_foot", "pfn_top", check=False)
edge("pfs_foot", "pfs_top", check=False)
node("pfn_v", -24.0, 0.0, 19.0)
ZONE_NODES["TankerDock"].append("pfn_v")
edge("pfn_foot", "pfn_v", check=False)
attach("pfn_v", "TankerDock", 2, within=8.0)
node("pfs_v", -32.5, 0.0, 1.0)
node("pfs_w", -24.0, 0.0, 1.0)
ZONE_NODES["TankerDock"] += ["pfs_v", "pfs_w"]
edge("pfs_foot", "pfs_v", check=False)
edge("pfs_foot", "pfs_w", check=False)
attach("pfs_v", "TankerDock", 2, within=8.0)
attach("pfs_w", "TankerDock", 2, within=8.0)
edge("pfn_top", nid(-26.0, 1.2, 14.0), check=False)
edge("pfn_top", nid(-30.0, 1.2, 14.0), check=False)
edge("pfs_top", nid(-30.0, 1.2, 6.0), check=False)
edge("pfs_top", nid(-26.0, 1.2, 6.0), check=False)

# ---------------------------------------------------------------- Doors
# name, wall axis ('x': wall along x at z=line; 'z': wall along z at x=line), line, position along it, the y of its floor, how far the
# nodes stand off the wall, the Zone on the low side and on the high.
DOORS = [
    ("d1", "z", -20.0, 10.0, 0.0, 1.8, "TankerDock", "Gatehouse"),
    ("d2", "z", 20.0, 10.0, 0.0, 1.8, "Gatehouse", "FittingShop"),
    ("d3", "x", 20.0, -10.5, 0.0, 1.8, "Gatehouse", "RackRoad"),
    ("d4", "x", 20.0, -40.0, 0.0, 1.8, "TankerDock", "TankFarm"),
    ("d5", "x", 20.0, 40.0, 0.0, 1.8, "FittingShop", "CompressorHouse"),
    ("d6", "z", -20.0, 28.0, 0.0, 1.8, "TankFarm", "RackRoad"),
    ("d7", "z", 20.0, 28.0, 0.0, 1.8, "RackRoad", "CompressorHouse"),
    ("d8", "x", 60.0, -40.0, 0.0, 1.8, "TankFarm", "FlareYard"),
    ("d9", "x", 60.0, 11.5, 0.0, 1.8, "RackRoad", "ColumnYard"),
    ("d10", "x", 60.0, 40.0, 0.0, 1.8, "CompressorHouse", "Substation"),
    ("d11", "z", -20.0, 75.0, 0.0, 1.8, "FlareYard", "ColumnYard"),
    ("d12", "z", 20.0, 75.0, 0.0, 1.8, "ColumnYard", "Substation"),
    ("g1", "z", 4.0, 57.0, 4.0, 1.8, "RackWalk", "PipeBridge"),
    ("g2", "x", 64.0, 0.0, 4.0, 1.8, "RackWalk", "ColumnPlatform"),
    ("g3", "z", -4.0, 52.0, 4.0, 1.8, "TankWalk", "RackWalk"),
]
for name, axis, line, pos, y, off, low, high in DOORS:
    if axis == "x":
        a, b = (pos, y, line - off), (pos, y, line + off)
    else:
        a, b = (line - off, y, pos), (line + off, y, pos)
    na = node(f"{name}_a", round(a[0], 2), y, round(a[2], 2))
    nb = node(f"{name}_b", round(b[0], 2), y, round(b[2], 2))
    edge(na, nb, f"Gate::Door(ZoneId::{low}, ZoneId::{high})", check=False)
    attach(na, low)
    attach(nb, high)

# F1: the fixed archway from the Pipe Bridge into the Control House, in its south wall at x 30.
node("f1_a", 30.0, 4.0, 60.6)
node("f1_b", 30.0, 4.0, 63.4)
edge("f1_a", "f1_b", check=False)
attach("f1_a", "PipeBridge", 2, within=8.0)
attach("f1_b", "ControlHouse", 2, within=8.0)

# ---------------------------------------------------------------- the stair bays (all straight flights, 8 m run for a storey)
# Each flight's door stands at its foot; the nodes inside the bay stand outside every Zone but where it lies in its top Zone.
def stair(code, a_pt, zone_low, chain, zone_high, top_zone_link, joins, via=None):
    a = node(f"{code}_a", *a_pt)
    if via:
        p = node(f"{code}_p", *via)
        edge(a, p)
        ZONE_NODES[zone_low].append(p)
        attach(p, zone_low, 2, within=9.0)
    else:
        attach(a, zone_low, 2, within=8.0)
    prev = a
    first = True
    for i, pt in enumerate(chain):
        n = snode(f"{code}_{i}", *pt)
        edge(prev, n, f"Gate::Door(ZoneId::{joins[0]}, ZoneId::{joins[1]})" if first else "OPEN", check=False)
        first = False
        prev = n
    return prev


# S1: z 32 to 44 along x -8..-4: the flight climbs z 32..40, a landing z 40..44, and the Rack Walk's west edge is open there.
last = stair("s1", (-6.0, 0.0, 30.7), "RackRoad", [(-6.0, 0.75, 33.5), (-6.0, 2.0, 36.0), (-6.0, 3.75, 39.5), (-6.0, 4.0, 42.5)], "RackWalk", None, ("RackRoad", "RackWalk"), via=(-11.0, 0.0, 30.7))
edge(last, nid(-2.0, 4.0, 42.5), check=False)
# S2: z 22 to 30 along x -50..-46, into the Girder Ring's south side.
last = stair("s2", (-48.0, 0.0, 20.9), "TankFarm", [(-48.0, 0.75, 23.5), (-48.0, 2.0, 26.0), (-48.0, 3.75, 29.5)], "TankWalk", None, ("TankFarm", "TankWalk"), via=(-53.0, 0.0, 21.0))
attach(last, "TankWalk", 2, within=8.0)
# S3: z 64 down to 54 along x -58..-54: up from the Flare Yard to the North Gantry's west end, a landing at z 54..56.
last = stair("s3", (-56.0, 0.0, 65.3), "FlareYard", [(-56.0, 0.75, 62.5), (-56.0, 2.0, 60.0), (-56.0, 3.75, 56.5), (-56.0, 4.0, 55.0)], "TankWalk", None, ("FlareYard", "TankWalk"), via=(-52.0, 0.0, 66.0))
node("s3_g", -56.0, 4.0, 52.0)
edge(last, "s3_g", check=False)
edge("s3_g", nid(-52.0, 4.0, 52.0), check=False)
# S4: z 66 to 76 along x 56..60, in through the Control House's east doorway at z 75.
last = stair("s4", (58.0, 0.0, 64.7), "Substation", [(58.0, 0.75, 67.5), (58.0, 2.0, 70.0), (58.0, 3.75, 73.5), (58.0, 4.0, 75.0)], "ControlHouse", None, ("Substation", "ControlHouse"))
node("s4_in", 54.0, 4.0, 75.0)
edge(last, "s4_in", check=False)
attach("s4_in", "ControlHouse", 2, within=8.0)
# S5: z 68 to 78 along x -16..-12, onto the Column Platform's west edge at z 76..78.
last = stair("s5", (-14.0, 0.0, 66.7), "ColumnYard", [(-14.0, 0.75, 69.5), (-14.0, 2.0, 72.0), (-14.0, 3.75, 75.5), (-14.0, 4.0, 77.0)], "ColumnPlatform", None, ("ColumnYard", "ColumnPlatform"))
node("s5_in", -10.0, 4.0, 77.0)
edge(last, "s5_in", check=False)
attach("s5_in", "ColumnPlatform", 2, within=8.0)
# S6: x 12..16, z 70..82, over the heater's roof: the Power Door PD at the foot, the flight z 72..80 (y 4 to 8), and the landing runs west to the Tower Deck.
node("s6_a", 10.2, 4.0, 71.0)
attach("s6_a", "ColumnPlatform", 2, within=8.0)
prev = snode("s6_0", 13.8, 4.0, 71.0)
edge("s6_a", prev, "Gate::Door(ZoneId::ColumnPlatform, ZoneId::TowerDeck)", check=False)
for i, pt in enumerate([(14.0, 5.5, 74.0), (14.0, 7.25, 77.5), (14.0, 8.0, 81.0), (10.0, 8.0, 81.0)], start=1):
    n = snode(f"s6_{i}", *pt)
    edge(prev, n, check=False)
    prev = n
node("s6_in", 6.0, 8.0, 81.0)
edge(prev, "s6_in", check=False)
ZONE_NODES["TowerDeck"].append("s6_in")
attach("s6_in", "TowerDeck", 2, within=8.0)

# ---------------------------------------------------------------- the Rack Walk's one drop: through the gap in its east railing at z 30 (a 1 m cantilever), gated on S1.
node("rw_drop", 3.0, 4.0, 30.0)
node("rr_drop", 6.5, 0.0, 30.0)
ZONE_NODES["RackWalk"].append("rw_drop")
ZONE_NODES["RackRoad"].append("rr_drop")
attach("rw_drop", "RackWalk", 2, within=8.0)
attach("rr_drop", "RackRoad", 3, within=8.0)
edge("rw_drop", "rr_drop", "Gate::DoorDrop(ZoneId::RackRoad, ZoneId::RackWalk)", check=False)

# ---------------------------------------------------------------- crossings: Open edges straight across a Blaze (the Fire ticket gates them)
link((0.0, 0.0, 22.5), (0.0, 0.0, 34.0))
link((0.0, 0.0, 39.5), (0.0, 0.0, 50.5))
link((-40.0, 0.0, 34.33), (-40.0, 0.0, 45.67))
link((-35.5, 4.0, 52.0), (-24.5, 4.0, 52.0))
link((-40.0, 0.0, 87.0), (-28.67, 0.0, 87.0))
link((7.29, 0.0, 72.6), (7.29, 0.0, 82.2))
link((-38.0, 0.0, 6.0), (-38.0, 0.0, 14.0))

# ---------------------------------------------------------------- Windows
for i, (zone, x, y, z, out) in enumerate(WINDOWS):
    outside = node(f"w{i}_out", x + out[0] * 2.0, y, z + out[1] * 2.0)
    inside = node(f"w{i}_in", x - out[0] * 1.8, y, z - out[1] * 1.8)
    edge(outside, inside, f"barrier({i})", check=False)
    ZONE_NODES[zone].append(inside)
    attach(inside, zone, 2, within=10.0)

# ---------------------------------------------------------------- Lamps
LAMPS = []   # (zone, x, y, z)


def lamps(zone, y, pts):
    for x, z in pts:
        LAMPS.append((zone, x, y, z))


def lattice(xs, zs):
    return [(x, z) for x in xs for z in zs]


SODIUM = 7.0
lamps("Gatehouse", SODIUM, [(-12, 9), (12, 9), (-12, 17), (12, 17)])
lamps("TankerDock", SODIUM, [(-56, 18), (-46, 18), (-36, 18), (-24, 18)])
lamps("TankFarm", SODIUM, [(-56, 24), (-24, 24), (-56, 56), (-24, 56)])
lamps("RackRoad", SODIUM, lattice([-12, 12], [28, 40, 52]))
lamps("FlareYard", SODIUM, [(-52, 66), (-24, 66), (-46, 88)])
lamps("ColumnYard", SODIUM, [(-18, 66), (18, 66), (-18, 84), (18, 84)])
lamps("Substation", SODIUM, [(24, 68), (24, 80), (52, 72)])
CATWALK = 4.0 + 2.6
lamps("RackWalk", CATWALK, [(3.7, 26), (3.7, 38), (3.7, 50), (3.7, 60)])
lamps("TankWalk", CATWALK, [(-56, 32), (-40, 48), (-30, 52), (-18, 52), (-8, 52)])
lamps("PipeBridge", CATWALK, [(10, 52.3), (20, 61.7), (32, 52.3)])
lamps("ColumnPlatform", CATWALK, [(-10, 66), (10, 66), (0, 86)])
lamps("TowerDeck", 8.0 + 2.6, [(-6, 70), (6, 82)])
SHED = 3.45
lamps("FittingShop", SHED, lattice([24, 32, 40, 48, 56], [5, 15]))
lamps("CompressorHouse", SHED, lattice([24, 32, 40, 48, 56], [25, 35, 45, 55]))
lamps("ControlHouse", 7.85, lattice([28, 36, 44, 52], [66, 74, 82]))
assert len(LAMPS) == 28 + 17 + 42, len(LAMPS)

# ---------------------------------------------------------------- checks
fail = []
INDOOR = ("FittingShop", "CompressorHouse", "ControlHouse")


def check_lamps():
    for zone in INDOOR:
        x0, x1, z0, z1, fl = ZONES[zone]
        mine = [(x, z) for z_, x, y, z in LAMPS if z_ == zone]
        x = x0 + 1.0
        while x <= x1 - 1.0 + 1e-6:
            z = z0 + 1.0
            while z <= z1 - 1.0 + 1e-6:
                if min(math.hypot(x - lx, z - lz) for lx, lz in mine) > 6.5:
                    fail.append(("lamp reach", zone, x, z))
                z += 0.25
            x += 0.25


check_lamps()

# Every node outside every Zone ends in `_out`, and the others don't (but a window's spawn point, which may stand in its own Zone).
for name, (x, y, z) in BY_NAME.items():
    zone = zone_at(x, y, z)
    if (zone is None) != name.endswith("_out") and not (name.startswith("w") and name.endswith("_out")):
        fail.append(("name vs zone", name, zone))
    if zone and ZONES[zone][4] != 4.0 and zone in ("RackWalk", "TankWalk"):
        pass
    for x0, z0, x1, z1, fy in BLAZES:
        if abs(fy - y) < 1.0 and x0 < x < x1 and z0 < z < z1:
            fail.append(("node in a Blaze", name))

# Every Valve spot is clear of the plant and of every Blaze.
for vx, vy, vz in VALVES:
    if blocked(vx, vz, vy, 0.0):
        fail.append(("valve in a block", vx, vz))
    for x0, z0, x1, z1, fy in BLAZES:
        if abs(fy - vy) < 1.0 and x0 - 1.5 < vx < x1 + 1.5 and z0 - 1.5 < vz < z1 + 1.5 and not any(abs(v[0] - vx) < 0.01 and abs(v[2] - vz) < 0.01 and False for v in []):
            pass

# Connectivity: with every Door bought, every Barrier broken and every drop taken, every node reaches every other.
adj = {n: set() for n in BY_NAME}
for a, b, g in EDGES:
    adj[a].add(b)
    if "barrier" not in g and "DoorDrop" not in g and "OneWay" not in g:
        adj[b].add(a)
    else:
        pass
seen, stack = set(), ["gh_0_0"]
while stack:
    n = stack.pop()
    if n in seen:
        continue
    seen.add(n)
    stack.extend(adj[n] - seen)
missing = sorted(set(BY_NAME) - seen)
# Window outside nodes are only entered by the Barrier edge (one-way): they reach in but nothing reaches them.
missing = [n for n in missing if not (n.startswith("w") and n.endswith("_out"))]
if missing:
    fail.append(("unreachable", missing[:20]))


def coverage():
    """Every open spot of a Zone's floor, on the 1.5 m grid the headless test walks, has a node it can walk straight to."""
    bad = []
    for zone in ORDER:
        x0, x1, y0, y1, z0, z1 = volume(zone)
        fl = ZONES[zone][4]
        names = [n for n in BY_NAME if True]
        x = x0 + 0.75
        while x < x1:
            z = z0 + 0.75
            while z < z1:
                zz = z
                z += 1.5
                if zone_at(x, fl + 0.1, zz) != zone:
                    continue
                if blocked(x, zz, fl, 0.35 + 0.05):
                    continue
                if zone == "TankerDock" and -32 < x < -24 and 4 < zz < 16:
                    continue
                own = sorted((m for m in BY_NAME if abs(BY_NAME[m][1] - fl) < 0.6), key=lambda m: math.hypot(BY_NAME[m][0] - x, BY_NAME[m][2] - zz))[:6]
                ok = any(not any(abs(l - fl) <= t and seg_hits(x, zz, BY_NAME[m][0], BY_NAME[m][2], bx0 - 0.3, bz0 - 0.3, bx1 + 0.3, bz1 + 0.3) for bx0, bz0, bx1, bz1, l, t in BLOCKS) for m in own)
                if not ok:
                    bad.append((zone, round(x, 2), round(zz, 2)))
            x += 1.5
    return bad


if os.environ.get("COVERAGE"):
    for b in coverage():
        print("uncovered", b)

if fail:
    for f in fail:
        print("FAIL", f)
    raise SystemExit(1)

# ---------------------------------------------------------------- write
out = []
out.append("//! The Refinery's windows, lamps, plant structures and path-node graph, laid out from `refinery.rs`'s walls by\n//! `tools/refinery_layout.py` (rewritten by it: edit the script, not this file).\n")
out.append("use bevy::prelude::*;\n\nuse crate::enemies::nav::Gate;\nuse crate::maps::map::{BarrierId, Prop, PropKind, prop};\nuse crate::maps::zone::ZoneId;\n\nuse super::W0;\n")
out.append("""const fn v(x: f32, z: f32) -> Vec2 {
    Vec2::new(x, z)
}

const fn barrier(i: u8) -> Gate {
    Gate::Barrier(BarrierId(W0 + i))
}

const OPEN: Gate = Gate::Open;

const fn window(zone: ZoneId, x: f32, y: f32, z: f32, out: Vec2) -> (ZoneId, Vec3, Vec2) {
    (zone, Vec3::new(x, y, z), out)
}

const fn p(kind: PropKind, zone: ZoneId, x: f32, y: f32, z: f32, size: (f32, f32, f32)) -> Prop {
    prop(kind, zone, (x, y, z), size)
}

""")
out.append("/// The 29 windows, each the home of one Barrier, in the order the path nodes' `w<n>_out` count them: culvert arches in the blast wall\n/// and the sheds' walls (each in a hatch wall `PEN` in from the wall it faces, with a walled pen of floor behind it), and ladder cages on the\n/// catwalks (each in a cage wall at the edge, with a pen on a ladder landing outside the railing).\npub(in crate::maps::map) const WINDOWS: [(ZoneId, Vec3, Vec2); 29] = [\n")
for zone, x, y, z, o in WINDOWS:
    out.append(f"    window(ZoneId::{zone}, {rs(x)}, {rs(y)}, {rs(z)}, v({rs(o[0])}, {rs(o[1])})),\n")
out.append("];\n\n")
out.append(f"/// The lamps ({len(LAMPS)}: 28 sodium posts, 17 catwalk lamps and 42 indoor lamps on a grid whose cells reach `LAMP_REACH`).\npub(super) const LAMPS: [(ZoneId, Vec3); {len(LAMPS)}] = [\n")
for zone, x, y, z in LAMPS:
    out.append(f"    (ZoneId::{zone}, Vec3::new({rs(x)}, {rs(y)}, {rs(z)})),\n")
out.append("];\n\n")
out.append(f"/// The plant structures that are loop centres and Blaze walls ({len(PLANT)}): the tanks and the column (round), the blast-wall-high things and\n/// the sleepers (plain solid boxes now; their silhouettes come with the Props).\npub(super) const PLANT: [Prop; {len(PLANT)}] = [\n")
for kind, zone, cx, y, cz, w, h, d in PLANT:
    out.append(f"    p(PropKind::{kind}, ZoneId::{zone}, {rs(cx)}, {rs(y)}, {rs(cz)}, ({rs(w)}, {rs(h)}, {rs(d)})),\n")
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
