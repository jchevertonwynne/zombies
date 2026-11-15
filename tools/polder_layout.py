#!/usr/bin/env python3
"""Lays out the Polder's pools, sunken floors, lamps and path-node graph from the walls and Props in polder.rs.
Writes src/maps/map/polder/data.rs."""
import math, re, sys, os, collections

ROOT = os.environ.get("ROOT", ".")
SRC = open(f"{ROOT}/src/maps/map/polder.rs").read()
ENV = dict(GROUND=0.0, CREST=4.0, H=4.0, RAIL=1.1, FENCE=1.8, PARAPET=1.6, STOREY=4.0, BERM=-1.6, BED=-4.5, CHALK=1.5, LANDING=3.0, RUN=6.0, FLIGHT=4.0, TRAP_FLIGHT=5.6)

def ev(s):
    return eval(s, {}, ENV)

# ---------------- zones ----------------
ZONES = {  # name: (x0, x1, z0, z1, floor, code)
    "TownSquare": (-26, 18, -40, 0, 0, "ts"), "WeighHouse": (-56, -26, -40, 0, 0, "wh"), "Brewery": (-80, -56, -56, 0, 0, "br"),
    "CheeseMarket": (18, 54, -40, 0, 0, "cm"), "AlmshouseCourt": (-26, 40, -56, -40, 0, "ac"), "Windmill": (-80, -56, 0, 48, 0, "wm"),
    "Warehouse": (-56, -34, 0, 24, 0, "wa"), "LockYard": (-34, -13, 0, 24, 0, "ly"), "SluiceHouse": (-13, 13, 0, 24, 0, "sh"),
    "TollQuay": (13, 34, 0, 24, 0, "tq"), "Shipyard": (34, 58, 0, 24, 0, "sy"), "CoalYard": (58, 80, 0, 48, 0, "cy"),
    "HerringQuay": (-56, 14, 24, 48, 0, "hq"), "CustomsHouse": (14, 58, 24, 48, 0, "cu"), "DykeRoad": (-80, 44, 48, 60, 4, "dr"),
    "PumpingStation": (44, 80, 48, 60, 4, "ps"),
}
INDOOR = {"WeighHouse", "Brewery", "Warehouse", "SluiceHouse", "CustomsHouse", "PumpingStation"}

def zone_of(x, z, y=0.1):
    for n, (x0, x1, z0, z1, fl, c) in ZONES.items():
        lo = fl - 0.25 + (0 if fl == 0 else 0)
        if x0 <= x <= x1 and z0 <= z <= z1 and y >= (fl - 0.25 - 9) and y <= (fl + 3.75 + (4.25 if n == "Windmill" else 0)):
            return n
    return None

# ---------------- water ----------------
SUNKEN = []   # (zone, x0, z0, x1, z1, depth)
POOLS = []    # (zone, x0, z0, x1, z1, bed)
WATER_RECTS = []  # (x0, z0, x1, z1) for obstacle purposes (any sunken)

def sunk(zone, x0, z0, x1, z1, depth):
    SUNKEN.append((zone, x0, z0, x1, z1, depth))

def pool(zone, x0, z0, x1, z1, bed):
    if x1 - x0 > 1e-6 and z1 - z0 > 1e-6:
        POOLS.append((zone, x0, z0, x1, z1, bed))

def deep(zone, x0, x1, zlo, zhi, gap=False):
    sunk(zone, x0, zlo, x1, zhi, 4.5)
    if not gap:
        pool(zone, x0, zlo, x1, zhi, -4.5)

FORDS = []  # (zone, sx, x0, x1, zlo, zhi, north_own, far_door)
def ford(zone, x0, x1, zlo, zhi, sx, own_north):
    b0, b1 = zlo + 3.5, zhi - 3.5
    sunk(zone, x0, zlo, x1, b0, 1.6)
    sunk(zone, x0, b0, x1, b1, 2.2)
    sunk(zone, x0, b1, x1, zhi, 1.6)
    for (za, zb) in [(zlo, b0), (b1, zhi)]:
        pool(zone, x0, za, sx - 1.0, zb, -1.6)
        pool(zone, sx + 1.0, za, x1, zb, -1.6)
    pool(zone, x0, b0, sx - 1.0, b1, -2.2)
    pool(zone, sx + 1.0, b0, x1, b1, -2.2)
    pool(zone, sx - 1.0, b0 + 1.5, sx + 1.0, b1 - 1.5, -2.2)
    FORDS.append((zone, sx, x0, x1, zlo, zhi, own_north))

# Town Canal
TC = (-12.0, 0.0)
deep("Brewery", -80, -70, *TC); deep("Brewery", -70, -66, *TC, gap=True); deep("Brewery", -66, -56, *TC)
deep("WeighHouse", -56, -50, *TC); ford("WeighHouse", -50, -36, *TC, -43, False); deep("WeighHouse", -36, -26, *TC)
deep("TownSquare", -26, -23.75, *TC); deep("TownSquare", -23.75, -20.25, *TC); deep("TownSquare", -20.25, 18, *TC)
deep("CheeseMarket", 18, 20, *TC); ford("CheeseMarket", 20, 32, *TC, 26, False); deep("CheeseMarket", 32, 42.25, *TC)
deep("CheeseMarket", 42.25, 45.75, *TC); deep("CheeseMarket", 45.75, 80, *TC)
# Harbour
HB = (24.0, 36.0)
deep("HerringQuay", -56, -54, *HB); ford("HerringQuay", -54, -42, *HB, -48, True); deep("HerringQuay", -42, -25.75, *HB)
deep("HerringQuay", -25.75, -22.25, *HB); deep("HerringQuay", -22.25, 14, *HB)
deep("CustomsHouse", 14, 22.25, *HB); deep("CustomsHouse", 22.25, 25.75, *HB); deep("CustomsHouse", 25.75, 40, *HB)
ford("CustomsHouse", 40, 54, *HB, 47, True); deep("CustomsHouse", 54, 58, *HB)
# Lock chambers
sunk("SluiceHouse", -13, 0, -6, 24, 4.5)
# each chamber's pool is cut at its gate's 1.4 m mouth (Inland z 1.8..3.2, Sea z 20.8..22.2), so the
# Deep Water walls can let the gate's walkway through
pool("SluiceHouse", -13, 0, -6, 1.8, -4.5); pool("SluiceHouse", -13, 1.8, -6, 3.2, -4.5); pool("SluiceHouse", -13, 3.2, -6, 24, -4.5)
sunk("SluiceHouse", 6, 0, 13, 24, 4.5)
pool("SluiceHouse", 6, 0, 13, 20.8, -4.5); pool("SluiceHouse", 6, 20.8, 13, 22.2, -4.5); pool("SluiceHouse", 6, 22.2, 13, 24, -4.5)
# Sea
DR_WIN = [-30.0, 10.0]; PS_WIN = [55.0, 70.0]
sunk("DykeRoad", -80, 60, 44, 66, 8.5); sunk("PumpingStation", 44, 60, 80, 66, 8.5)
pool("DykeRoad", -80, 64, 44, 66, -4.5); pool("PumpingStation", 44, 64, 80, 66, -4.5)
def fills(zone, lo, hi, wins):
    cur = lo
    for w in wins:
        pool(zone, cur, 60, w - 2, 64, -4.5); cur = w + 2
    pool(zone, cur, 60, hi, 64, -4.5)
fills("DykeRoad", -80, 44, DR_WIN); fills("PumpingStation", 44, 80, PS_WIN)
# Wells (stairwells up the dyke)
WELLS = [("DykeRoad", -72, 48, -68, 54.2, 4.0), ("DykeRoad", -22.8, 48, -17.2, 57.2, 4.0), ("PumpingStation", 65.2, 48, 70.8, 57.2, 4.0)]
for w in WELLS:
    SUNKEN.append(w)

CANAL_SUNK = [s for s in SUNKEN if s[5] >= 1.5 and s not in WELLS]

# ---------------- parse the Rust ----------------
PROPS = []  # (kind, zone, x, y, z, w, h, d)
for m in re.finditer(r"^\s+p\((\w+), (\w+), (-?[\d.]+), ([^,]+), (-?[\d.]+), \(([\d., ]+)\)\),", SRC, re.M):
    kind, zone, x, y, z, size = m.groups()
    w, h, d = [float(t) for t in size.split(",")]
    PROPS.append((kind, zone, float(x), ev(y), float(z), w, h, d))

WALLS = []  # (axis, line, a, b, y0, h, gaps)
GAPRE = re.compile(r"\((-?[\d.]+), (door|pdoor|pgate|gate|Gap::Window|Gap::Open)(?:\((\w+), (\w+)(?:, (\w+))?\))?")
DOORS = []  # (axis, line, pos, zoneA, zoneB, y0, name)
for m in re.finditer(r"^\s+(xwall|zwall)\(b, (.*?), vec!\[(.*)\]\);", SRC, re.M | re.S):
    pass
for line in SRC.splitlines():
    m = re.match(r"\s+(xwall|zwall)\(b, (.*)\);\s*$", line)
    if not m:
        continue
    kind, rest = m.groups()
    mm = re.match(r"(.*), vec!\[(.*)\]$", rest)
    head, gaps = mm.groups()
    try:
        if kind == "xwall":
            r = re.match(r"\((.*?), (.*?)\), (.*?), (.*?), (.*)$", head)
            a, b, line_, y0, h = [ev(t) for t in r.groups()]
            axis = "x"
        else:
            r = re.match(r"(.*?), \((.*?), (.*?)\), (.*?), (.*)$", head)
            line_, a, b, y0, h = [ev(t) for t in r.groups()]
            axis = "z"
    except NameError:
        continue
    gl = []
    for g in GAPRE.finditer(gaps):
        pos, what, za, zb, price = g.groups()
        gl.append((float(pos), what, za, zb))
        if what in ("door", "pdoor", "pgate", "gate"):
            DOORS.append((axis, line_, float(pos), za, zb, y0, price))
    WALLS.append((axis, line_, a, b, y0, h, gl))

WINDOWS = []
for m in re.finditer(r"^\s+window\((\w+), (-?[\d.]+), ([^,]+), (-?[\d.]+), Vec2::(\w+)\),", SRC, re.M):
    zone, x, y, z, out = m.groups()
    o = dict(X=(1, 0), NEG_X=(-1, 0), Y=(0, 1), NEG_Y=(0, -1))[out]
    WINDOWS.append((zone, float(x), ev(y), float(z), o))
assert len(WINDOWS) == 30, len(WINDOWS)
RAISED = []
for m in re.finditer(r"^const (\w+): RaisedFloor = RaisedFloor \{ zone: ZoneId::(\w+), min: v\((-?[\d.]+), (-?[\d.]+)\), max: v\((-?[\d.]+), (-?[\d.]+)\), height: ([^,]+),", SRC, re.M):
    n, zone, x0, z0, x1, z1, hgt = m.groups()
    RAISED.append((n, zone, float(x0), float(z0), float(x1), float(z1), ev(hgt)))
assert len(RAISED) == 5

# ---------------- obstacles ----------------
class Ob:
    def __init__(s, x0, z0, x1, z1, y0, y1):
        s.x0, s.z0, s.x1, s.z1, s.y0, s.y1 = x0, z0, x1, z1, y0, y1

OBS = []
def wall_rects(w):
    axis, line, a, b, y0, h, gaps = w
    cuts = []
    for pos, what, *_ in gaps:
        width = {"door": 2.4, "pdoor": 2.4, "pgate": 2.4, "gate": 2.4, "Gap::Window": 1.6}.get(what, 4.0)
        cuts.append((pos - width / 2, pos + width / 2, what))
    cuts.sort()
    spans = []
    cur = a
    for lo, hi, what in cuts:
        spans.append((cur, lo, y0, y0 + h))
        if what == "Gap::Window":
            spans.append((lo, hi, y0, y0 + 0.9))  # sill
        cur = hi
    spans.append((cur, b, y0, y0 + h))
    out = []
    for lo, hi, ya, yb in spans:
        if hi - lo < 0.01:
            continue
        if axis == "x":
            out.append(Ob(lo, line - 0.2, hi, line + 0.2, ya, yb))
        else:
            out.append(Ob(line - 0.2, lo, line + 0.2, hi, ya, yb))
    return out
for w in WALLS:
    OBS += wall_rects(w)
for kind, zone, x, y, z, w_, h, d in PROPS:
    OBS.append(Ob(x - w_ / 2, z - d / 2, x + w_ / 2, z + d / 2, y, y + h))
# pockets: 4x4 box beyond each window plus its walls
POCKETS = []
for i, (zone, x, y, z, o) in enumerate(WINDOWS):
    cx, cz = x + o[0] * 2.0, z + o[1] * 2.0
    OBS.append(Ob(cx - 2.0 if o[0] == 0 else min(x, x + o[0] * 4), cz - 2.0 if o[1] == 0 else min(z, z + o[1] * 4),
                  cx + 2.0 if o[0] == 0 else max(x, x + o[0] * 4), cz + 2.0 if o[1] == 0 else max(z, z + o[1] * 4), y, y + 2.6))
# fixtures: perk machines, the Pack-a-Punch machine, Box Locations, plus the chalks' fronts
FIX = [(5, -39.2, 0), (-38, 23.2, 0), (34.8, 20, 0), (-8, 47.2, 0), (0, 7.8, 0.6)]
for fx, fz, fy in FIX:
    OBS.append(Ob(fx - 1.0, fz - 1.0, fx + 1.0, fz + 1.0, fy, fy + 2.0))
BOXES = [(-79.35, -26.0, 0), (0, -55.35, 0), (-55.35, 20, 0), (57.35, 20, 0), (50, 47.35, 0), (79.35, 30, 0)]
for fx, fz, fy in BOXES:
    OBS.append(Ob(fx - 1.2, fz - 1.2, fx + 1.2, fz + 1.2, fy, fy + 1.2))
# raised floors are solid blocks: nothing on the ground lattice is inside them
for (_n, _z, _x0, _z0, _x1, _z1, _h) in RAISED:
    OBS.append(Ob(_x0, _z0, _x1, _z1, 0.0, _h))
# stair footprints
SLOPE_RECTS = []  # (x0,z0,x1,z1, y_lo, y_hi)
def slope(x, y, z, up, run, rise, width):
    dx, dz = up
    ex, ez = x + dx * run, z + dz * run
    hw = width / 2
    if dx != 0:
        r = (min(x, ex), z - hw, max(x, ex), z + hw)
    else:
        r = (x - hw, min(z, ez), x + hw, max(z, ez))
    SLOPE_RECTS.append((*r, y, y + rise))
S_ = {"Y": (0, 1), "NEG_Y": (0, -1), "X": (1, 0), "NEG_X": (-1, 0)}
for m in re.finditer(r"Slope \{ zone: ZoneId::(\w+), foot: at\((-?[\d.]+), ([^,]+), ([^)]+)\), up: Vec2::(\w+), run: ([^,]+), rise: ([^,]+), width: ([^,]+),", SRC):
    zone, x, y, z, up, run, rise, width = m.groups()
    slope(float(x), ev(y), ev(z), S_[up], ev(run), ev(rise), ev(width))

def water_stairs():
    out = []
    for m in re.finditer(r"water_stair\(ZoneId::(\w+), (-?[\d.]+), (-?[\d.]+), Vec2::(\w+)\)", SRC):
        zone, x, bank, up = m.groups()
        x, bank = float(x), float(bank)
        dx, dz = S_[up]
        foot = bank - dz * 3.5
        slope(x, -1.6, foot, (dx, dz), 3.5, 1.6, 2.0)
    return out
water_stairs()
# ramps from berm to channel (generated, also appear in polder.rs as Slopes)
RAMPS = []
for zone, sx, x0, x1, zlo, zhi, own_north in FORDS:
    b0, b1 = zlo + 3.5, zhi - 3.5
    RAMPS.append((zone, sx, b0 + 1.5, "NEG_Y"))   # south ramp: foot at b0+1.5 rises toward -z
    RAMPS.append((zone, sx, b1 - 1.5, "Y"))        # north ramp: foot at b1-1.5 rises toward +z
for zone, sx, z, up in RAMPS:
    slope(sx, -2.2, z, S_[up], 1.5, 0.6, 2.0)

# ---------------- geometry helpers ----------------
def seg_rect_dist(p, q, r):
    # min distance between segment pq and rect r (x0,z0,x1,z1)
    x0, z0, x1, z1 = r
    n = 24
    best = 1e9
    for i in range(n + 1):
        t = i / n
        x = p[0] + (q[0] - p[0]) * t
        z = p[1] + (q[1] - p[1]) * t
        dx = max(x0 - x, 0, x - x1)
        dz = max(z0 - z, 0, z - z1)
        best = min(best, math.hypot(dx, dz))
    return best

def blocks(ob, level):
    return ob.y0 < level + 1.0 and ob.y1 > level + 0.35

def node_ok(x, z, level, margin, extra=()):
    for ob in OBS:
        if blocks(ob, level):
            dx = max(ob.x0 - x, 0, x - ob.x1); dz = max(ob.z0 - z, 0, z - ob.z1)
            if math.hypot(dx, dz) < margin:
                return False
    for (x0, z0, x1, z1, ya, yb) in SLOPE_RECTS:
        if abs(level - ya) < 5 or True:
            dx = max(x0 - x, 0, x - x1); dz = max(z0 - z, 0, z - z1)
            if math.hypot(dx, dz) < margin + 0.3:
                return False
    for s in SUNKEN:
        zone, x0, z0, x1, z1, depth = s
        if depth < 0.5:
            continue
        floor_ = ZONES[zone][4]
        if level > floor_ - depth + 0.5 or True:
            dx = max(x0 - x, 0, x - x1); dz = max(z0 - z, 0, z - z1)
            if math.hypot(dx, dz) < margin + 0.2 and level > floor_ - depth + 0.5:
                return False
    return True

def edge_ok(p, q, level_p, level_q, clear=0.8):
    lv = max(level_p, level_q) if abs(level_p - level_q) < 0.01 else None
    for ob in OBS:
        yl = min(level_p, level_q), max(level_p, level_q)
        if (ob.y0 < yl[1] + 1.0 and ob.y1 > yl[0] + 0.35):
            if seg_rect_dist(p, q, (ob.x0, ob.z0, ob.x1, ob.z1)) < clear - 0.0:
                return False
    for (x0, z0, x1, z1, ya, yb) in SLOPE_RECTS:
        if seg_rect_dist(p, q, (x0, z0, x1, z1)) < clear - 0.1 and abs(level_p - level_q) < 0.01:
            return False
    for s in SUNKEN:
        zone, x0, z0, x1, z1, depth = s
        floor_ = ZONES[zone][4]
        if depth >= 0.5 and min(level_p, level_q) > floor_ - depth + 0.5:
            if seg_rect_dist(p, q, (x0, z0, x1, z1)) < clear - 0.1 and abs(level_p - level_q) < 0.01:
                return False
    return True

NODES = collections.OrderedDict()  # name -> (x, y, z, zone)
EDGES = []  # (a, b, gate)
LATT = collections.defaultdict(list)  # zone -> [(name, x, z, level)]

def add_node(name, x, y, z):
    assert name not in NODES, name
    NODES[name] = (round(x, 2), round(y, 3), round(z, 2))
    return name

def lattice(zone, level, x0, x1, z0, z1, spacing, margin, tag, ob_margin=0.95):
    xs0, xs1, zs0, zs1 = x0 + margin, x1 - margin, z0 + margin, z1 - margin
    if xs1 < xs0 or zs1 < zs0:
        return []
    nx = max(1, int((xs1 - xs0) / spacing) + 1) if xs1 > xs0 else 1
    nz = max(1, int((zs1 - zs0) / spacing) + 1) if zs1 > zs0 else 1
    if xs1 > xs0 and (xs1 - xs0) / spacing > nx - 1 + 0.5: nx += 1
    if zs1 > zs0 and (zs1 - zs0) / spacing > nz - 1 + 0.5: nz += 1
    out = []
    code = ZONES[zone][5]
    grid = {}
    for i in range(nx):
        for j in range(nz):
            x = xs0 + (xs1 - xs0) * (i / (nx - 1) if nx > 1 else 0.5)
            z = zs0 + (zs1 - zs0) * (j / (nz - 1) if nz > 1 else 0.5)
            if not node_ok(x, z, level, ob_margin):
                continue
            name = add_node(f"{code}{tag}_{i}_{j}", x, level, z)
            grid[(i, j)] = name
            out.append((name, x, z, level))
    for (i, j), a in grid.items():
        for di, dj in [(1, 0), (0, 1), (1, 1), (1, -1)]:
            b = grid.get((i + di, j + dj))
            if b:
                pa, pb = NODES[a], NODES[b]
                if edge_ok((pa[0], pa[2]), (pb[0], pb[2]), level, level):
                    EDGES.append((a, b, "OPEN"))
    LATT[zone] += out
    return out

def link(name, level, zone=None, radius=8.0, maxn=3, clear=0.8, same_zone=True):
    x, y, z = NODES[name][:3]
    z_ = zone
    cands = []
    for zn, lst in LATT.items():
        for (n, nx_, nz_, lv) in lst:
            if n == name: continue
            d = math.hypot(nx_ - x, nz_ - z)
            if d <= radius and abs(lv - y) < 0.2:
                cands.append((d, n, nx_, nz_, lv))
    cands.sort()
    made = 0
    for d, n, nx_, nz_, lv in cands:
        if edge_ok((x, z), (nx_, nz_), y, lv, clear):
            EDGES.append((name, n, "OPEN"))
            made += 1
            if made >= maxn:
                break
    return made

# ---------------- ground lattice ----------------
SPACING = {"SluiceHouse": 3.5}
RAISED_RECTS = [(zone, x0, z0, x1, z1, h) for (_, zone, x0, z0, x1, z1, h) in RAISED]
for zone, (x0, x1, z0, z1, fl, code) in ZONES.items():
    lattice(zone, float(fl), x0, x1, z0, z1, SPACING.get(zone, 6.0), 1.5, "")
# raised-floor lattices
for n, zone, x0, z0, x1, z1, h in RAISED:
    if n in ("HUMPBACK",):
        continue
    lattice(zone, h, x0, x1, z0, z1, 2.6, 0.9, "r" + n[0].lower(), ob_margin=0.9)

def dist(a, b):
    return math.hypot(NODES[a][0] - NODES[b][0], NODES[a][2] - NODES[b][2])

# ---------------- doors ----------------
ZN = {"TownSquare", "WeighHouse", "Brewery", "CheeseMarket", "AlmshouseCourt", "Windmill", "Warehouse", "LockYard", "SluiceHouse", "TollQuay", "Shipyard", "CoalYard", "HerringQuay", "CustomsHouse", "DykeRoad", "PumpingStation"}
doorN = 0
SPECIAL_DOORS = {}  # handled by hand: keyed by (axis, line, pos)
def door_edge(a, b, za, zb):
    EDGES.append((a, b, f"DOOR {za} {zb}"))

# Doors handled specially (bridge pens, water gates, humpback, dyke ways, PD1)
special = {
    ("x", -16.0, -22.0), ("x", -16.0, 44.0), ("x", 20.0, -24.0), ("x", 20.0, 24.0),     # bridge pens
    ("x", 0.0, -43.0), ("x", 24.0, -48.0), ("x", 0.0, 26.0), ("x", 24.0, 47.0),         # water gates
    ("x", -0.2, -68.0),                                                                   # humpback
    ("x", 48.0, -70.0), ("x", 48.0, -20.0), ("x", 48.0, 68.0),                           # dyke ways
    ("z", -13.0, 2.5), ("z", 13.0, 21.5),                                                 # PD1, PD2
}
door_nodes = {}
for (axis, line_, pos, za, zb, y0, price) in DOORS:
    key = (axis, line_, pos)
    if key in special:
        continue
    global_doorN = None
    doorN += 1
    if axis == "x":
        pa = (pos, line_ - 2.0); pb = (pos, line_ + 2.0)
    else:
        pa = (line_ - 2.0, pos); pb = (line_ + 2.0, pos)
    # a is the side in zone za
    if zone_of(*pa) != za:
        pa, pb = pb, pa
    assert zone_of(*pa) == za and zone_of(*pb) == zb, (key, za, zb, zone_of(*pa), zone_of(*pb))
    na = add_node(f"d{doorN}_a", pa[0], y0 if y0 else 0.0, pa[1])
    nb = add_node(f"d{doorN}_b", pb[0], y0 if y0 else 0.0, pb[1])
    door_edge(na, nb, za, zb)
    for n in (na, nb):
        LATT[zone_of(NODES[n][0], NODES[n][2])].append((n, NODES[n][0], NODES[n][2], 0.0))

def manual(name, x, y, z, zone=None):
    add_node(name, x, y, z)
    return name

def both(a, b, gate="OPEN"):
    EDGES.append((a, b, gate))

def reg(name):
    x, y, z = NODES[name]
    LATT[zone_of(x, z, y + 0.1)].append((name, x, z, y))

# ---- bridges ----
def bridge(tag, x, po_z, pi_z, far_z, za, zb, piece):
    po = manual(f"{tag}_po", x, 0.0, po_z)
    pi = manual(f"{tag}_pi", x, 0.0, pi_z)
    fl = manual(f"{tag}_far", x, 0.0, far_z)
    for n in (po, pi, fl): reg(n)
    door_edge(po, pi, za, zb)
    both(pi, fl, f"REST {piece}")
bridge("mb", -22.0, -18.0, -13.8, 1.5, "TownSquare", "LockYard", 0)
bridge("cb", 44.0, -18.0, -13.8, 1.5, "CheeseMarket", "Shipyard", 2)
bridge("hb", -24.0, 18.0, 22.2, 37.5, "LockYard", "HerringQuay", 1)
bridge("tb", 24.0, 18.0, 22.2, 37.5, "TollQuay", "CustomsHouse", 3)

# ---- the Humpback Bridge ----
manual("hump_a", -68, 0.0, -21.0); reg("hump_a")
manual("hump_b", -68, 1.5, -10.0)
manual("hump_c", -68, 1.5, -2.2)
manual("hump_d", -68, 1.1625, 1.8); reg("hump_d")
manual("hump_e", -68, 0.0, 9.5); reg("hump_e")
both("hump_a", "hump_b"); both("hump_b", "hump_c"); door_edge("hump_c", "hump_d", "Brewery", "Windmill"); both("hump_d", "hump_e")

# ---- the dyke ways ----
def dyke(tag, x, lower_zone, upper_zone, landing, pdoor_):
    foot_z = 48.2 + landing
    d = manual(f"{tag}_lo", x, 0.0, 45.5); reg(d)
    u0 = manual(f"{tag}_u0", x, 0.0 if landing else 0.2, 49.2 if landing else 48.5)
    mid_z = foot_z + 3.0
    u1 = manual(f"{tag}_u1", x, 2.0, mid_z)
    u2 = manual(f"{tag}_u2", x, 4.0, foot_z + 6.0 + 1.4)
    door_edge(d, u0, lower_zone, upper_zone)
    both(u0, u1); both(u1, u2)
    reg(u2)
dyke("wr", -70, "Windmill", "DykeRoad", 0.0, False)
dyke("ds", -20, "HerringQuay", "DykeRoad", 3.0, False)
dyke("cs", 68, "CoalYard", "PumpingStation", 3.0, True)

# ---- drops beside each Door ----
for tag, x, lo_zone, hi_zone, topz in (("dw", -62.0, "Windmill", "DykeRoad", 50.5), ("dh", -12.0, "HerringQuay", "DykeRoad", 50.5), ("dc", 60.0, "CoalYard", "PumpingStation", 50.5)):
    t = manual(f"{tag}_top", x, 4.0, topz); reg(t)
    b = manual(f"{tag}_bot", x, 0.0, 45.5); reg(b)
    EDGES.append((t, b, f"DROP {lo_zone} {hi_zone}"))

# ---- the Windmill's stage ----
manual("ws_foot", -68, 0.0, 17.2); reg("ws_foot")
manual("ws_top", -68, 4.0, 26.2); reg("ws_top")
both("ws_foot", "ws_top")

# ---- Pack-a-Punch's steps, the daises' ----
manual("pap_foot", 0, 0.0, 15.4); reg("pap_foot")
manual("pap_top", 0, 0.6, 12.6); reg("pap_top")
both("pap_foot", "pap_top")
manual("sc_foot", -42, 0.0, -23.6); reg("sc_foot")
manual("sc_top", -42, 0.4, -26.7); reg("sc_top")
both("sc_foot", "sc_top")
manual("wd_foot_w", 26.0, 0.0, -29.0); reg("wd_foot_w")
manual("wd_top_w", 28.8, 0.5, -29.0); reg("wd_top_w")
both("wd_foot_w", "wd_top_w")
manual("wd_foot_e", 40.0, 0.0, -29.0); reg("wd_foot_e")
manual("wd_top_e", 37.2, 0.5, -29.0); reg("wd_top_e")
both("wd_foot_e", "wd_top_e")

# ---- PD1 and PD2: the Lock Gates' wicket Doors and walkways ----
# The Door is the gate's gate Door; nav runs quay -> wicket node (Door) -> island landing (Rest of
# the gate Shut). No node sits on a Leaf.
def lock_gate(tag, quay_x, wicket_x, landing_x, z, quay_zone, piece):
    manual(f"{tag}_a", quay_x, 0.0, z); reg(f"{tag}_a")
    manual(f"{tag}_w", wicket_x, 0.0, z)
    manual(f"{tag}_b", landing_x, 0.0, z); reg(f"{tag}_b")
    door_edge(f"{tag}_a", f"{tag}_w", quay_zone, "SluiceHouse")
    both(f"{tag}_w", f"{tag}_b", f"REST {piece}")
# the corner by the Market Bridge's landing, which its deck's rail end and a shelf close off from
# the lattice (and which the wicket node would otherwise crowd out of the nearest few)
manual("ly_qa", -19.2, 0.0, 1.9); reg("ly_qa")
both("ly_qa", "ly_2_0")
lock_gate("pd1", -15.0, -13.5, -4.0, 2.5, "LockYard", 4)
lock_gate("pd2", 15.0, 13.5, 4.0, 21.5, "TollQuay", 5)

# ---- the Fords ----
DOOR_OF_FORD = {"WeighHouse": ("Warehouse", 0.0, -1), "CheeseMarket": ("TollQuay", 0.0, -1), "HerringQuay": ("Warehouse", 24.0, 1), "CustomsHouse": ("Shipyard", 24.0, 1)}
for zone, sx, x0, x1, zlo, zhi, own_north in FORDS:
    far_zone, far_wall, far_dir = DOOR_OF_FORD[zone]
    t = f"f{int(abs(sx))}"
    b0, b1 = zlo + 3.5, zhi - 3.5
    if own_north:
        own_bank, own_edge, own_dir = zhi, b1, 1      # stair rises toward +z to the quay at zhi
        far_bank, far_edge = zlo, b0
    else:
        own_bank, own_edge, own_dir = zlo, b0, -1
        far_bank, far_edge = zhi, b1
    # own stair, from the quay down to the berm; its landing (-0.3) is 2.84 m up from the berm's edge
    ot = manual(f"{t}_ot", sx, 0.0, own_bank + own_dir * 2.0); reg(ot)
    ol = manual(f"{t}_ol", sx, -0.3, own_edge + own_dir * 2.84)
    of = manual(f"{t}_of", sx, -1.52, own_edge + own_dir * 0.2)
    both(ot, ol); both(ol, of)
    cc = manual(f"{t}_c", sx, -2.2, (b0 + b1) / 2)
    both(of, cc)
    for dx_, nm_ in ((-4.0, 'w'), (4.0, 'e')):
        n_ = manual(f"{t}_c{nm_}", sx + dx_, -2.2, (b0 + b1) / 2)
        both(cc, n_)
        LATT[zone].append((n_, sx + dx_, (b0 + b1) / 2, -2.2))
    far_dir_z = -own_dir
    ff = manual(f"{t}_ff", sx, -1.52, far_edge + far_dir_z * 0.2)
    fl = manual(f"{t}_fl", sx, -0.3, far_edge + far_dir_z * 2.84)
    both(cc, ff); both(ff, fl)
    w = manual(f"{t}_w", sx, 0.0, far_wall + (2.0 if far_wall == 0.0 else -2.0)); reg(w)
    EDGES.append((fl, w, f"DOOR {zone} {far_zone}"))
    # berm nodes
    for (edge, name, dirn) in ((own_edge, "o", own_dir), (far_edge, "f", far_dir_z)):
        zc = edge + dirn * 0.7
        for dx, nm in ((-3.5, "w"), (3.5, "e")):
            n = manual(f"{t}_b{name}{nm}", sx + dx, -1.6, zc)
            both(n, of if name == "o" else ff)
            LATT[zone].append((n, sx + dx, zc, -1.6))

# ---- windows ----
for i, (zone, x, y, z, o) in enumerate(WINDOWS):
    nin = manual(f"w{i}_in", x - o[0] * 1.6, y, z - o[1] * 1.6)
    nout = manual(f"w{i}_out", x + o[0] * 2.0, y, z + o[1] * 2.0)
    EDGES.append((nout, nin, f"BARRIER {i}"))
    LATT[zone].append((nin, NODES[nin][0], NODES[nin][2], y))

# ---- link manual nodes to lattice ----
manual_links = [n for n in NODES if re.match(r"(mb|cb|hb|tb)_(po|far)|hump_[ae]|d\d+_[ab]|ws_foot|ws_top|pap_foot|pap_top|sc_foot|sc_top|wd_foot|wd_top|pd[12]_[ab]|(wr|ds|cs)_(lo|u2)|(dw|dh|dc)_(top|bot)|f\d+_(ot|w)|w\d+_in", n)]
nolink = []
for n in manual_links:
    y = NODES[n][1]
    if n.startswith("mb_pi") or n.startswith("cb_pi"):
        continue
    made = link(n, y, radius=9.0, maxn=3)
    if not made:
        nolink.append(n)
# the raised floors' own nodes, around their machines
manual("pap_p", 0, 0.6, 10.4); both("pap_top", "pap_p")
manual("sc_a", -44.3, 0.4, -27.4); manual("sc_b", -39.7, 0.4, -27.4); both("sc_top", "sc_a"); both("sc_top", "sc_b")
for n_, x_, z_ in (("wd_a", 31.5, -27.5), ("wd_b", 37.0, -27.5), ("wd_c", 31.5, -30.5), ("wd_d", 37.0, -30.5)):
    manual(n_, x_, 0.5, z_)
both("wd_top_w", "wd_a"); both("wd_a", "wd_b"); both("wd_b", "wd_top_e"); both("wd_top_w", "wd_c"); both("wd_c", "wd_d"); both("wd_d", "wd_top_e")
for n_ in list(nolink):
    if link(n_, NODES[n_][1], radius=12.0, maxn=2, clear=0.5):
        nolink.remove(n_)
# round the winch platform inside the sluice house
manual("sh_arch", 0, 0.0, 6.0)
for sd, nm in ((-3.7, "w"), (3.7, "e")):
    manual(f"sh_c{nm}1", sd, 0.0, 6.0); manual(f"sh_c{nm}2", sd, 0.0, 15.0)
    both("sh_arch", f"sh_c{nm}1"); both(f"sh_c{nm}1", f"sh_c{nm}2"); both(f"sh_c{nm}2", "pap_foot")
both("sh_arch", "sh_3_0"); both("sh_arch", "sh_4_0")
# the island's north apron, beyond the culvert grate's strip, and a nook by the Cheese Bridge's pen
for nm, x_, z_ in (("sh_na_w", -3.8, 19.0), ("sh_na_e", 3.8, 19.0), ("sh_nb_w", -3.8, 22.5), ("sh_nb_e", 3.8, 22.5)):
    manual(nm, x_, 0.0, z_)
both("w16_in", "sh_na_w"); both("w16_in", "sh_na_e"); both("sh_na_w", "sh_nb_w"); both("sh_na_e", "sh_nb_e")
both("pd2_b", "sh_na_e"); both("pd2_b", "sh_nb_e")
manual("cm_nook", 48.8, 0.0, -13.2); reg("cm_nook"); link("cm_nook", 0.0, radius=12.0, maxn=3, clear=0.6)
# the Toll Quay's north-east corner, round the tollhouse window's strip
manual("tq_na", 28.2, 0.0, 18.8); manual("tq_nb", 28.2, 0.0, 22.4); manual("tq_nc", 32.0, 0.0, 22.0)
both("tq_na", "tq_nb"); both("tq_nb", "tq_nc"); reg("tq_na"); link("tq_na", 0.0, radius=10.0, maxn=3, clear=0.6)
# pen inside nodes only join their own far and po (done)
# raised-floor top nodes join their raised lattices
for n in ("ws_top", "pap_top", "sc_top", "wd_top_w", "wd_top_e"):
    y = NODES[n][1]
    link(n, y, radius=6.0, maxn=3, clear=0.6)

# ---------------- drop what the start can't reach ----------------
adj = collections.defaultdict(set)
for a_, b_, g_ in EDGES:
    adj[a_].add(b_); adj[b_].add(a_)
seen_ = {"ts_0_1"}; todo = ["ts_0_1"]
while todo:
    n_ = todo.pop()
    for m_ in adj[n_]:
        if m_ not in seen_:
            seen_.add(m_); todo.append(m_)
dropped = [n_ for n_ in NODES if n_ not in seen_]
if dropped:
    print("DROPPED:", dropped, file=sys.stderr)
for n_ in dropped:
    del NODES[n_]
EDGES[:] = [e for e in EDGES if e[0] in NODES and e[1] in NODES]
# ---------------- coverage report ----------------
if nolink:
    print("UNLINKED:", nolink, file=sys.stderr)

# ---------------- lamps ----------------
LAMPS = []
def land(x, z, zone):
    for s in SUNKEN:
        if s[0] == zone and s[5] >= 1.5 and s[1] <= x <= s[3] and s[2] <= z <= s[4]:
            return False
    return True
for zone, (x0, x1, z0, z1, fl, code) in ZONES.items():
    cell = 7.5 if zone in INDOOR else 22.0
    nx = max(1, math.ceil((x1 - x0) / cell)); nz = max(1, math.ceil((z1 - z0) / cell))
    for i in range(nx):
        for j in range(nz):
            cx0 = x0 + (x1 - x0) * i / nx; cx1 = x0 + (x1 - x0) * (i + 1) / nx
            cz0 = z0 + (z1 - z0) * j / nz; cz1 = z0 + (z1 - z0) * (j + 1) / nz
            pts = [(cx0 + (cx1 - cx0) * (a + 0.5) / 4, cz0 + (cz1 - cz0) * (b + 0.5) / 4) for a in range(4) for b in range(4)]
            pts = [p for p in pts if land(p[0], p[1], zone)]
            if len(pts) < 3:
                continue
            mx = sum(p[0] for p in pts) / len(pts); mz = sum(p[1] for p in pts) / len(pts)
            LAMPS.append((zone, round(mx, 1), fl + 3.4, round(mz, 1)))

# ---------------- emit ----------------
def f(v):
    s = f"{v:.2f}".rstrip("0").rstrip(".")
    if "." not in s: s += ".0"
    return s
def fy(y):
    s = f"{y:.3f}".rstrip("0").rstrip(".")
    if "." not in s: s += ".0"
    return s

out = []
out.append("//! The Polder's pools, sunken floors, lamps and path-node graph, laid out from `polder.rs`'s walls,\n//! Props and canals (Doors, bridges, stairs, Fords and drops by hand). Rewritten by the layout script.\n")
out.append("use bevy::prelude::*;\n\nuse crate::enemies::nav::Gate;\nuse crate::maps::shifting::PieceId;\nuse crate::maps::map::{BarrierId, Level, Pool, Sunken};\nuse crate::maps::tide::TideMask;\nuse crate::maps::water::CanalId;\nuse crate::maps::zone::ZoneId;\n\nuse super::{OPEN, W0};\n")
out.append("const fn v(x: f32, z: f32) -> Vec2 {\n    Vec2::new(x, z)\n}\n")
out.append("const fn barrier(i: u8) -> Gate {\n    Gate::Barrier(BarrierId(W0 + i))\n}\n")
out.append("const fn sunk(zone: ZoneId, x0: f32, z0: f32, x1: f32, z1: f32, depth: f32) -> Sunken {\n    Sunken { zone, min: v(x0, z0), max: v(x1, z1), depth, rim: false }\n}\n")
out.append("/// A pool of one of the Polder's Canals: the sea past z 60, the Harbour from z 3 to its north bank, the Town Canal south of that (and the lock's seaward chamber, which touches both).\nconst fn pool(zone: ZoneId, x0: f32, z0: f32, x1: f32, z1: f32, bed: f32) -> Pool {\n    let canal = if z0 >= 60.0 {\n        2\n    } else if z0 >= 3.0 {\n        1\n    } else {\n        0\n    };\n    Pool { zone, min: v(x0, z0), max: v(x1, z1), level: Level::Canal(CanalId(canal)), bed }\n}\n\n/// An edge walkable only at the Tide levels in `bits` (1 Low, 2 Mid, 4 High).\nconst fn tide(bits: u8) -> Gate {\n    Gate::Tide(TideMask(bits))\n}\n")
out.append(f"/// The canals' and the sea's beds, and the stairwells up the dyke.\npub(super) const SUNKEN: [Sunken; {len(SUNKEN)}] = [\n" + "".join(f"    sunk(ZoneId::{z}, {f(a)}, {f(b)}, {f(c)}, {f(d)}, {f(e)}),\n" for z, a, b, c, d, e in SUNKEN) + "];\n")
out.append(f"/// The pools: every Deep reach, the Fords' berms and channels, the lock's chambers and the sea.\npub(super) const POOLS: [Pool; {len(POOLS)}] = [\n" + "".join(f"    pool(ZoneId::{z}, {f(a)}, {f(b)}, {f(c)}, {f(d)}, {f(e)}),\n" for z, a, b, c, d, e in POOLS) + "];\n")
out.append(f"/// The lamps: a grid under every roof, and a few along the open quays.\npub(super) const LAMPS: [(ZoneId, Vec3); {len(LAMPS)}] = [\n" + "".join(f"    (ZoneId::{z}, Vec3::new({f(x)}, {fy(y)}, {f(zz)})),\n" for z, x, y, zz in LAMPS) + "];\n")
names = list(NODES)
out.append(f"/// The path nodes: a name, then where a Zombie's feet stand there (x, y, z).\npub(super) const NODES: &[(&str, f32, f32, f32)] = &[\n" + "".join(f'    ("{n}", {f(NODES[n][0])}, {fy(NODES[n][1])}, {f(NODES[n][2])}),\n' for n in names) + "];\n")
def gate(g):
    if g == "OPEN": return "OPEN"
    t = g.split()
    if t[0] == "DOOR": return f"Gate::Door(ZoneId::{t[1]}, ZoneId::{t[2]})"
    if t[0] == "DROP": return f"Gate::DoorDrop(ZoneId::{t[1]}, ZoneId::{t[2]})"
    if t[0] == "BARRIER": return f"barrier({t[1]})"
    if t[0] == "REST": return f"Gate::Rest(PieceId({t[1]}), 0)"
    if t[0] == "TIDE": return f"tide({t[1]})"
    raise ValueError(g)
seen = set()
E2 = []
for a, b, g in EDGES:
    if (a, b) in seen or (b, a) in seen and g == "OPEN":
        continue
    seen.add((a, b))
    assert a in NODES and b in NODES, (a, b)
    E2.append((a, b, g))
# ---- the Tide: which levels each walked edge is clear at ----
CANAL_LEVELS = (-1.7, -1.1, -0.4)
def deep_at(x, y, z, lvl):
    for zone, x0, z0, x1, z1, bed in POOLS:
        surface = CANAL_LEVELS[lvl] if True else 0
        if x0 <= x <= x1 and z0 <= z <= z1 and y < surface and y >= bed - 0.5 and surface - bed >= 1.0:
            return True
    return False
def node_mask(name):
    y = NODES[name][1]
    if y <= -2.0: return 1
    if y <= -1.4: return 3
    return 7
def edge_mask(a, b):
    m = node_mask(a) & node_mask(b)
    (ax, ay, az), (bx, by, bz) = NODES[a], NODES[b]
    for lvl in range(3):
        for k in range(41):
            t = k / 40
            if deep_at(ax + (bx - ax) * t, ay + (by - ay) * t, az + (bz - az) * t, lvl):
                m &= ~(1 << lvl)
                break
    return m
E3 = []
for a, b, g in E2:
    if g == "OPEN":
        m = edge_mask(a, b)
        assert m != 0, (a, b)
        if m != 7:
            g = f"TIDE {m}"
    E3.append((a, b, g))
E2 = E3
out.append(f"/// The edges, by node name. Barrier edges and drops run from the first node to the second.\npub(super) const EDGES: &[(&str, &str, Gate)] = &[\n" + "".join(f'    ("{a}", "{b}", {gate(g)}),\n' for a, b, g in E2) + "];\n")
os.makedirs(f"{ROOT}/src/maps/map/polder", exist_ok=True)
open(f"{ROOT}/src/maps/map/polder/data.rs", "w").write("\n".join(out))
print(f"nodes {len(NODES)} edges {len(E2)} pools {len(POOLS)} sunken {len(SUNKEN)} lamps {len(LAMPS)} ramps {len(RAMPS)}", file=sys.stderr)
