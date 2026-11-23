#!/usr/bin/env python3
"""Lays out the Sanatorium's windows, sunken stairwells, lamps and path-node graph (#518), from the plan
in #503. Writes src/maps/map/sanatorium/data.rs; `sanatorium.rs` builds the walls to match it.

    python3 tools/sanatorium_layout.py

Every grid of nodes is laid out inside a Zone's box, inset from its walls, and each edge between two
of them is kept only if the straight walk between them clears the rectangles in `BLOCKS` (Props,
posts, the stairwell holes). The Doors, windows, stairs and drops are listed by hand below."""
import math
import os

ROOT = os.environ.get("ROOT", ".")
OUT = f"{ROOT}/src/maps/map/sanatorium/data.rs"

STOREY = 4.0
NODES = []        # (name, x, y, z)
EDGES = []        # (a, b, gate source)
BY_NAME = {}
BLOCKS = []       # (x0, z0, x1, z1, y-level or None): rectangles an edge at that level mustn't cross
ZONE_NODES = {}   # zone -> [names] of its grid


def node(name, x, y, z):
    assert name not in BY_NAME, name
    BY_NAME[name] = (x, y, z)
    NODES.append((name, x, y, z))
    return name


def clear(a, b, margin=0.45):
    (ax, ay, az), (bx, by, bz) = BY_NAME[a], BY_NAME[b]
    for x0, z0, x1, z1, level, tol in BLOCKS:
        if abs(level - ay) > tol and abs(level - by) > tol:
            continue
        x0, z0, x1, z1 = x0 - margin, z0 - margin, x1 + margin, z1 + margin
        # Segment against the rectangle, sampled finely.
        steps = max(2, int(math.hypot(bx - ax, bz - az) / 0.1))
        for i in range(steps + 1):
            t = i / steps
            x, z = ax + (bx - ax) * t, az + (bz - az) * t
            if x0 < x < x1 and z0 < z < z1:
                return False
    return True


def edge(a, b, gate="OPEN", check=True):
    if check and gate == "OPEN":
        assert clear(a, b), f"{a} -> {b} crosses a block"
    EDGES.append((a, b, gate))


def space(lo, hi, step):
    n = max(1, math.ceil((hi - lo) / step))
    return [lo + (hi - lo) * i / n for i in range(n + 1)] if hi > lo else [lo]


def grid(zone, code, x0, x1, z0, z1, y, step=7.0, xs=None, zs=None):
    xs = xs or space(x0, x1, step)
    zs = zs or space(z0, z1, step)
    names = {}
    for i, x in enumerate(xs):
        for j, z in enumerate(zs):
            if any(bx0 - 0.3 < x < bx1 + 0.3 and bz0 - 0.3 < z < bz1 + 0.3 and abs(lv - y) <= tol for bx0, bz0, bx1, bz1, lv, tol in BLOCKS):
                continue
            names[(i, j)] = node(f"{code}_{i}_{j}", round(x, 2), y, round(z, 2))
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
        if math.hypot(BY_NAME[c][0] - x, BY_NAME[c][2] - z) > within:
            break
        if abs(BY_NAME[c][1] - y) < 0.3 and clear(name, c):
            EDGES.append((name, c, "OPEN"))
            done += 1
            if done == count:
                return
    assert done, f"{name} reaches nothing in {zone}"


# ---------------------------------------------------------------- blocks (Props, posts, holes, rails)
# The Founder's plinth, the bandstand's posts and the Operating Theatre's table (Pack-a-Punch).
BLOCKS += [
    (-2.5, -17.5, 2.5, -12.5, 0.0, 0.3),
    (-3.5, 24.5, 3.5, 31.5, 0.0, 0.3),        # the bandstand, seen from the ground
    (-3.25, 24.75, -2.75, 25.25, 0.6, 0.3), (2.75, 24.75, 3.25, 25.25, 0.6, 0.3),
    (-3.25, 30.75, -2.75, 31.25, 0.6, 0.3), (2.75, 30.75, 3.25, 31.25, 0.6, 0.3),
    (21.0, 45.5, 23.0, 46.5, 4.0, 0.3),       # Pack-a-Punch
    (-4.2, 56.0, 4.2, 64.0, 4.0, 0.3),        # the Main Stair's well, as the Landing's floor sees it
    (-29.2, 14.0, -25.8, 27.4, 4.0, 0.3),     # the Fire Escape's well, as the Sun Roof's floor sees it
]

# ---------------------------------------------------------------- zones' grids
HALL = grid("EntranceHall", "hall", -7.5, 7.5, 2.5, 11.5, 0.0, step=7.5)
FORE = grid("Forecourt", "fc", -46.0, 46.0, -27.0, -3.0, 0.0, step=8.0)
WCOR = grid("WestCorridor", "wc", -48.0, -12.0, 11.0, 11.0, 0.0, step=7.5)
ECOR = grid("EastCorridor", "ec", 12.0, 48.0, 11.0, 11.0, 0.0, step=7.5)
COURT = grid("GardenCourt", "gc", -23.5, 23.5, 16.5, 39.5, 0.0, step=7.5)
REFEC = grid("Refectory", "rf", -23.5, 23.5, 44.5, 53.5, 0.0, step=7.8)
BOIL = grid("BoilerHouse", "bh", -69.5, -52.5, -11.5, 11.5, 0.0, step=7.5)
MORT = grid("Mortuary", "mo", 52.5, 69.5, -11.5, 11.5, 0.0, step=7.5)
LONG = grid("LongWard", "lw", -47.5, -31.5, 16.5, 39.5, 0.0, step=7.5)
DAY = grid("DayRoom", "dr", -47.5, -28.5, 44.5, 53.5, 0.0, step=7.5)
BATH = grid("BathHall", "ba", 28.5, 47.5, 16.5, 39.5, 0.0, step=7.5)
TREAT = grid("TreatmentRoom", "tr", 28.5, 47.5, 44.5, 53.5, 0.0, step=7.5)
LAND = grid("UpperLanding", "ul", -8.0, 8.0, 44.5, 54.5, 4.0, step=6.0)
THEA = grid("OperatingTheatre", "ot", 13.0, 32.0, 44.0, 49.5, 4.0, step=6.4)
SUN = grid("SunRoof", "sr", -47.5, -31.0, 17.0, 40.0, 4.0, step=7.5)
ISO = grid("IsolationWard", "iw", -23.5, -12.5, 44.5, 53.5, 4.0, step=7.0)
CHILD = grid("ChildrensWard", "cw", -47.5, -28.5, 44.5, 53.5, 4.0, step=7.5)

# ---------------------------------------------------------------- Raised Floors and their steps
# The bandstand (7 x 7, 0.6 up, steps on its west and east sides), in the Garden Court.
BS = [node("bs_w", -3.0, 0.6, 28.0), node("bs_c", 0.0, 0.6, 28.0), node("bs_e", 3.0, 0.6, 28.0)]
for a, b in zip(BS, BS[1:]):
    edge(a, b, check=False)
node("bs_w_foot", -5.9, 0.0, 28.0)
node("bs_e_foot", 5.9, 0.0, 28.0)
edge("bs_w_foot", "bs_w", check=False)
edge("bs_e_foot", "bs_e", check=False)
ZONE_NODES["GardenCourt"] += ["bs_w_foot", "bs_e_foot", "bs_w", "bs_c", "bs_e"]
attach("bs_w_foot", "GardenCourt", 3)
attach("bs_e_foot", "GardenCourt", 3)
# The observation gallery (1.2 up along the north wall, four steps up at each end).
GAL = [node("gal_w", 16.0, 5.2, 54.0), node("gal_c", 22.0, 5.2, 54.0), node("gal_e", 28.0, 5.2, 54.0)]
for a, b in zip(GAL, GAL[1:]):
    edge(a, b, check=False)
node("gal_sw", 11.0, 4.0, 54.0)
node("gal_se", 33.0, 4.0, 54.0)
edge("gal_sw", "gal_w", check=False)
edge("gal_se", "gal_e", check=False)
ZONE_NODES["OperatingTheatre"] += ["gal_sw", "gal_se"]
attach("gal_sw", "OperatingTheatre", 2)
attach("gal_se", "OperatingTheatre", 2)

# ---------------------------------------------------------------- Doors
# name, wall axis ('x': wall along x at z=line; 'z': wall along z at x=line), line, position along it,
# the y of its floor, how far the nodes stand off the wall, the Zone on the low side and on the high.
DOORS = [
    ("d1", "x", 0.0, 0.0, 0.0, 1.8, "Forecourt", "EntranceHall"),
    ("d2", "z", -10.0, 11.0, 0.0, 1.8, "WestCorridor", "EntranceHall"),
    ("d3", "z", 10.0, 11.0, 0.0, 1.8, "EntranceHall", "EastCorridor"),
    ("d4", "z", -50.0, 11.0, 0.0, 1.8, "BoilerHouse", "WestCorridor"),
    ("d5", "z", -50.0, -7.0, 0.0, 1.8, "BoilerHouse", "Forecourt"),
    ("d6", "z", 50.0, 11.0, 0.0, 1.8, "EastCorridor", "Mortuary"),
    ("d7", "z", 50.0, -7.0, 0.0, 1.8, "Forecourt", "Mortuary"),
    ("d8", "x", 14.0, -18.0, 0.0, 1.8, "WestCorridor", "GardenCourt"),
    ("d9", "x", 14.0, 18.0, 0.0, 1.8, "EastCorridor", "GardenCourt"),
    ("d10", "x", 42.0, 0.0, 0.0, 1.8, "GardenCourt", "Refectory"),
    ("d12", "z", 10.0, 49.0, 4.0, 1.8, "UpperLanding", "OperatingTheatre"),
    ("q1", "x", 14.0, -40.0, 0.0, 2.5, "WestCorridor", "LongWard"),
    ("q2", "z", -26.0, 49.0, 0.0, 2.5, "DayRoom", "Refectory"),
    ("q3", "x", 14.0, 40.0, 0.0, 2.5, "EastCorridor", "BathHall"),
    ("q4", "z", 26.0, 49.0, 0.0, 2.5, "Refectory", "TreatmentRoom"),
    ("q5", "z", -10.0, 49.0, 4.0, 2.5, "IsolationWard", "UpperLanding"),
    ("q6", "x", 42.0, -38.0, 4.0, 2.5, "SunRoof", "ChildrensWard"),
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

# D11 (Refectory -> the Main Stair's tower) and D13 (the Garden Court -> the Fire Escape's bay).
node("d11_a", 2.0, 0.0, 54.2)
attach("d11_a", "Refectory")
node("ms_d_out", 2.0, 0.0, 57.8)
edge("d11_a", "ms_d_out", "Gate::Door(ZoneId::Refectory, ZoneId::UpperLanding)", check=False)
node("d13_b", -24.2, 0.0, 17.0)
attach("d13_b", "GardenCourt")
node("fe_d_out", -27.8, 0.0, 17.0)
edge("fe_d_out", "d13_b", "Gate::Door(ZoneId::GardenCourt, ZoneId::SunRoof)", check=False)

# ---------------------------------------------------------------- Wings' archways (fixed joins)
for name, a, b, za, zb in [
    ("arch_w", (-40.0, 0.0, 40.0), (-40.0, 0.0, 44.0), "LongWard", "DayRoom"),
    ("arch_e", (38.0, 0.0, 40.0), (38.0, 0.0, 44.0), "BathHall", "TreatmentRoom"),
    ("arch_i", (-24.0, 4.0, 49.0), (-28.0, 4.0, 49.0), "IsolationWard", "ChildrensWard"),
]:
    na = node(f"{name}_a", *a)
    nb = node(f"{name}_b", *b)
    edge(na, nb)
    attach(na, za)
    attach(nb, zb)

# ---------------------------------------------------------------- Stairs
# Main Stair: up the tower's west half from its north end, south onto the Upper Landing.
node("ms_a_out", 2.0, 0.0, 60.5)
node("ms_b_out", 2.0, 0.0, 63.3)
node("ms_c_out", -2.0, 0.0, 63.4)
node("ms_mid_out", -2.0, 2.0, 59.6)
node("ms_top_out", -2.0, 3.53, 57.0)
for a, b in [("ms_d_out", "ms_a_out"), ("ms_a_out", "ms_b_out"), ("ms_b_out", "ms_c_out"), ("ms_c_out", "ms_mid_out"), ("ms_mid_out", "ms_top_out")]:
    edge(a, b, check=False)
node("ms_land", -2.0, 4.0, 54.5)  # a Landing grid node, if the grid didn't make one there
# Fire Escape: up the bay along the Long Ward's east wall, onto the Sun Roof.
node("fe_mid_out", -27.5, 2.0, 22.75)
node("fe_top", -27.5, 3.81, 26.6)
node("sr_top", -27.5, 4.0, 29.5)
for a, b in [("fe_d_out", "fe_mid_out"), ("fe_mid_out", "fe_top"), ("fe_top", "sr_top")]:
    edge(a, b, check=False)
attach("sr_top", "SunRoof", 3)
attach("ms_land", "UpperLanding", 3)
edge("ms_top_out", "ms_land", check=False)

# The Sun Roof's one drop: through the gap in the court-side parapet (z 31 to 34), gated on D13.
node("sr_drop", -27.4, 4.0, 32.5)
node("gc_drop", -23.4, 0.0, 32.5)
attach("sr_drop", "SunRoof", 2)
attach("gc_drop", "GardenCourt", 3)
edge("sr_drop", "gc_drop", "Gate::DoorDrop(ZoneId::GardenCourt, ZoneId::SunRoof)", check=False)

# ---------------------------------------------------------------- Landing alcoves (beside the stair well)
node("ul_w", -7.0, 4.0, 60.0)
node("ul_e", 7.0, 4.0, 60.0)
ZONE_NODES["UpperLanding"] += ["ul_w", "ul_e"]
attach("ul_w", "UpperLanding", 2, within=8.0)
attach("ul_e", "UpperLanding", 2, within=8.0)

# ---------------------------------------------------------------- Windows
# (zone, x, y, z, out): each Barrier is one, in this order.
WINDOWS = [
    ("EntranceHall", -10.0, 0.0, 4.0, (-1, 0)), ("EntranceHall", 10.0, 0.0, 4.0, (1, 0)),
    ("WestCorridor", -30.0, 0.0, 8.0, (0, -1)), ("WestCorridor", -20.0, 0.0, 8.0, (0, -1)),
    ("EastCorridor", 20.0, 0.0, 8.0, (0, -1)), ("EastCorridor", 30.0, 0.0, 8.0, (0, -1)),
    ("Refectory", -16.0, 0.0, 56.0, (0, 1)), ("Refectory", 12.0, 0.0, 56.0, (0, 1)),
    ("BoilerHouse", -58.0, 0.0, -14.0, (0, -1)), ("BoilerHouse", -60.0, 0.0, 14.0, (0, 1)),
    ("Mortuary", 56.0, 0.0, 14.0, (0, 1)), ("Mortuary", 66.0, 0.0, 14.0, (0, 1)),
    ("LongWard", -50.0, 0.0, 18.0, (-1, 0)), ("LongWard", -50.0, 0.0, 24.0, (-1, 0)),
    ("DayRoom", -40.0, 0.0, 56.0, (0, 1)),
    ("BathHall", 50.0, 0.0, 22.0, (1, 0)), ("BathHall", 50.0, 0.0, 32.0, (1, 0)),
    ("TreatmentRoom", 40.0, 0.0, 56.0, (0, 1)),
    ("UpperLanding", -10.0, 4.0, 61.0, (-1, 0)),
    ("OperatingTheatre", 18.0, 5.2, 56.0, (0, 1)), ("OperatingTheatre", 26.0, 5.2, 56.0, (0, 1)),
    ("SunRoof", -50.0, 4.0, 30.0, (-1, 0)), ("SunRoof", -50.0, 4.0, 38.0, (-1, 0)),
    ("IsolationWard", -22.0, 4.0, 56.0, (0, 1)),
    ("ChildrensWard", -50.0, 4.0, 46.0, (-1, 0)), ("ChildrensWard", -50.0, 4.0, 52.0, (-1, 0)),
]
assert len(WINDOWS) == 26
for i, (zone, x, y, z, out) in enumerate(WINDOWS):
    outside = node(f"w{i}_out", x + out[0] * 2.0, y, z + out[1] * 2.0)
    inside = node(f"w{i}_in", x - out[0] * 1.8, y, z - out[1] * 1.8)
    edge(outside, inside, f"barrier({i})", check=False)
    if zone == "OperatingTheatre":
        edge(inside, GAL[1 if x == 18.0 else 2], check=False)
    else:
        attach(inside, zone)

# ---------------------------------------------------------------- Vents (a Nova Crawler's drop, then its floor spot)
VENTS = [("WestCorridor", -26.0, 0.0, 11.0), ("EastCorridor", 26.0, 0.0, 11.0), ("Refectory", 0.0, 0.0, 49.0), ("UpperLanding", 6.0, 4.0, 50.0)]
for i, (zone, x, y, z) in enumerate(VENTS):
    floor = node(f"vf{i}", x, y, z)
    attach(floor, zone, 3)
    vent = node(f"vent_{i}", x, y + 3.5, z)
    edge(vent, floor, "Gate::OneWay", check=False)

# ---------------------------------------------------------------- Lamps
LAMPS = []   # (zone, x, y, z)


def lamps(zone, y, pts):
    for x, z in pts:
        LAMPS.append((zone, x, y, z))


def lattice(xs, zs):
    return [(x, z) for x in xs for z in zs]


lamps("EntranceHall", 7.85, lattice([-5, 5], [3.5, 10.5]))
lamps("WestCorridor", 3.85, lattice([-45, -35, -25, -15], [11]))
lamps("EastCorridor", 3.85, lattice([15, 25, 35, 45], [11]))
lamps("Refectory", 3.85, lattice([-20.8, -10.4, 0, 10.4, 20.8], [45.5, 52.5]))
lamps("BoilerHouse", 7.85, lattice([-67, -61, -55], [-9, 0, 9]))
lamps("Mortuary", 3.85, lattice([55, 61, 67], [-9, 0, 9]))
lamps("LongWard", 3.85, lattice([-44, -35], [17.5, 24.5, 31.5, 38.5]))
lamps("DayRoom", 3.85, lattice([-44, -38, -32], [45.5, 52.5]))
lamps("BathHall", 3.85, lattice([31, 38, 45], [19, 28, 37]))
lamps("TreatmentRoom", 3.85, lattice([32, 38, 44], [45.5, 52.5]))
lamps("UpperLanding", 7.85, [(-5, 45), (5, 45), (-6.5, 53), (6.5, 53), (0, 50), (-7, 60.5), (7, 60.5)])
lamps("OperatingTheatre", 7.85, lattice([16, 22, 28], [45.5, 52.5]))
lamps("IsolationWard", 7.85, lattice([-21, -15], [45.5, 52.5]))
lamps("ChildrensWard", 7.85, lattice([-44, -38, -32], [45.5, 52.5]))
# The four outdoor posts: two on the Forecourt's gate piers, two at the bandstand.
lamps("Forecourt", 3.0, [(-4.5, -1.0), (4.5, -1.0)])
lamps("GardenCourt", 3.0, [(-4.0, 22.5), (4.0, 22.5)])

# ---------------------------------------------------------------- write
ZONES = {
    "EntranceHall": (-10, 10, 0, 14, 0, 8), "WestCorridor": (-50, -10, 8, 14, 0, 4), "EastCorridor": (10, 50, 8, 14, 0, 4),
    "Refectory": (-26, 26, 42, 56, 0, 4), "BoilerHouse": (-72, -50, -14, 14, 0, 8), "Mortuary": (50, 72, -14, 14, 0, 4),
    "LongWard": (-50, -29, 14, 42, 0, 4), "DayRoom": (-50, -26, 42, 56, 0, 4), "BathHall": (26, 50, 14, 42, 0, 4),
    "TreatmentRoom": (26, 50, 42, 56, 0, 4), "UpperLanding": (-10, 10, 42, 64, 4, 8), "OperatingTheatre": (10, 34, 42, 56, 4, 8),
    "IsolationWard": (-26, -10, 42, 56, 4, 8), "ChildrensWard": (-50, -26, 42, 56, 4, 8),
}


def check_lamps():
    bad = []
    for zone, (x0, x1, z0, z1, fl, top) in ZONES.items():
        mine = [(x, z) for z_, x, y, z in LAMPS if z_ == zone]
        x = x0 + 1.0
        while x < x1 - 1.0:
            z = z0 + 1.0
            while z < z1 - 1.0:
                in_well = (zone == "UpperLanding" and -4 <= x <= 4 and 56.2 <= z <= 63.8)
                if not in_well and min(math.hypot(x - lx, z - lz) for lx, lz in mine) > 6.5:
                    bad.append((zone, x, z))
                z += 0.5
            x += 0.5
    assert not bad, bad[:10]


check_lamps()
assert len(LAMPS) == 96, len(LAMPS)


def rs(n):
    s = f"{n:.2f}".rstrip("0").rstrip(".")
    return s if "." in s else s + ".0"


out = []
out.append("//! The Sanatorium's windows, stairwells, lamps and path-node graph, laid out from `sanatorium.rs`'s\n//! walls and Props by `tools/sanatorium_layout.py` (rewritten by it: edit the script, not this file).\n")
out.append("use bevy::prelude::*;\n\nuse crate::enemies::nav::Gate;\nuse crate::maps::map::{BarrierId, Sunken};\nuse crate::maps::zone::ZoneId;\n\nuse super::W0;\n")
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
""")
out.append("/// The stairwells: the Main Stair's in the Upper Landing's floor and the Fire Escape's in the Sun Roof's,\n/// each a storey deep, in a bay that stands in no ground Zone's box.\n")
out.append("pub(super) const SUNKEN: [Sunken; 2] = [\n    Sunken { zone: ZoneId::UpperLanding, min: v(-3.8, 56.2), max: v(3.8, 63.8), depth: 4.0, rim: false },\n    Sunken { zone: ZoneId::SunRoof, min: v(-28.8, 14.2), max: v(-26.2, 27.0), depth: 4.0, rim: false },\n];\n")
out.append("/// The 26 windows, each the home of one Barrier, in the order the path nodes' `w<n>_out` count them.\npub(in crate::maps::map) const WINDOWS: [(ZoneId, Vec3, Vec2); 26] = [\n")
for zone, x, y, z, o in WINDOWS:
    out.append(f"    window(ZoneId::{zone}, {rs(x)}, {rs(y)}, {rs(z)}, v({rs(o[0])}, {rs(o[1])})),\n")
out.append("];\n\n")
out.append(f"/// The lamps ({len(LAMPS)}: {len(LAMPS) - 4} indoors on a grid whose cells reach `LAMP_REACH`, and 4 outdoor posts).\npub(super) const LAMPS: [(ZoneId, Vec3); {len(LAMPS)}] = [\n")
for zone, x, y, z in LAMPS:
    out.append(f"    (ZoneId::{zone}, Vec3::new({rs(x)}, {rs(y)}, {rs(z)})),\n")
out.append("];\n\n")
out.append(f"pub(super) const NODES: &[(&str, f32, f32, f32)] = &[\n")
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
