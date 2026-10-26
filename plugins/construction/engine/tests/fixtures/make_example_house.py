#!/usr/bin/env python3
"""Generate the example-house test fixtures.

The house is invented: a detached two-storey brick cavity-wall house of 9.6 x 7.4 m on
"Example Street 1, 1000 Brussels", with an attached L-shaped garage (lower walls, lean-to roof)
and a separate garden studio. Nothing here describes a real building.

Writes (paths relative to the repository root):

  plugins/construction/engine/tests/fixtures/house.json          an early, simpler sketch of the
                                                                  house (rooms, one polyRoom, a porch,
                                                                  roof + roofVolumes, a few fixtures,
                                                                  an `electrical` block)
  plugins/construction/engine/tests/fixtures/house_current.json  the full house (free walls, stair,
                                                                  fixtures, components + component
                                                                  scripts, site shapes, guides, ...)
  plugins/electrical/engine/tests/fixtures/house.json            the full house without its
                                                                  components / scripts / site
  plugins/electrical/engine/tests/fixtures/electrical.json       the distribution board and circuits

Plan coordinates are [x, z] in metres (z grows towards the street, south); room outlines run on
the wall centre lines. Ids come from a seeded generator, so re-running the script reproduces the
same files (and the ids the tests refer to).

    python3 plugins/construction/engine/tests/fixtures/make_example_house.py
"""

import json
import math
import os
import random
import uuid

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", "..", "..", ".."))
CONSTRUCTION = HERE
ELECTRICAL = os.path.join(ROOT, "plugins", "electrical", "engine", "tests", "fixtures")

RNG = random.Random(9674)


def uid():
    return str(uuid.UUID(int=RNG.getrandbits(128), version=4)).upper()


def r4(v):
    return round(v + 0.0, 4)


H0, H1 = 2.75, 2.6              # storey heights: ground floor, first floor
HEIGHTS = [H0, H1]
ADDRESS = "Example Street 1, 1000 Brussels"
FLOOR_IDS = [uid(), uid()]


# ---- rooms ---------------------------------------------------------------------------------------

class Room:
    def __init__(self, name, level, rect=None, poly=None, wall_height=None, has_ceiling=False):
        self.id = uid()
        self.name, self.level = name, level
        self.rect, self.poly = rect, poly
        self.wall_height, self.has_ceiling = wall_height, has_ceiling
        if rect:
            (x0, z0), (w, d) = rect
            self.points = [(x0, z0), (x0 + w, z0), (x0 + w, z0 + d), (x0, z0 + d)]
        else:
            self.points = [tuple(p) for p in poly]
        n = len(self.points)
        self.signed_area = sum(self.points[i][0] * self.points[(i + 1) % n][1] - self.points[(i + 1) % n][0] * self.points[i][1] for i in range(n)) / 2

    @property
    def area(self):
        return abs(self.signed_area)

    def edge(self, e):
        n = len(self.points)
        return self.points[e % n], self.points[(e + 1) % n]

    def edge_frame(self, e):
        """(a, unit direction, length, inward unit normal) of an edge."""
        a, b = self.edge(e)
        l = math.hypot(b[0] - a[0], b[1] - a[1])
        d = ((b[0] - a[0]) / l, (b[1] - a[1]) / l)
        inward = (-d[1], d[0]) if self.signed_area > 0 else (d[1], -d[0])
        return a, d, l, inward

    def host(self, e):
        return {("roomEdge" if self.rect else "polyEdge"): {"edge": e, "room": self.id}}

    def json(self):
        if self.rect:
            v = {"hasCeiling": self.has_ceiling, "hasFloor": True, "id": self.id, "level": self.level, "name": self.name,
                 "origin": list(self.rect[0]), "size": list(self.rect[1])}
        else:
            v = {"hasFloor": True, "id": self.id, "level": self.level, "name": self.name, "points": [list(p) for p in self.points]}
        if self.wall_height is not None:
            v["wallHeight"] = self.wall_height
        return v


def opening(room, e, along, kind, width, height, sill=0.0):
    _, _, l, _ = room.edge_frame(e)
    assert width / 2 <= along <= l - width / 2 + 1e-9, f"{kind} on {room.name} edge {e} does not fit: along {along}, width {width}, length {l}"
    return {"along": along, "height": height, "id": uid(), "kind": kind, "level": room.level, "sill": sill,
            "wall": room.host(e), "width": width}


def wall_opening(wall, along, kind, width, height, sill=0.0):
    return {"along": along, "height": height, "id": uid(), "kind": kind, "level": wall["level"], "sill": sill,
            "wall": {"wall": wall["id"]}, "width": width}


def free_wall(level, a, b, height=None, exterior=None):
    v = {"a": list(a), "b": list(b), "id": uid(), "level": level}
    if height is not None:
        v["height"] = height
    if exterior is not None:
        v["exterior"] = exterior
    return v


# ---- fixtures ------------------------------------------------------------------------------------

def rotation_for(n):
    """Fixture rotation (radians) whose facing [sin r, -cos r] is the plan normal n."""
    return r4(math.atan2(n[0], -n[1]))


def on_wall(kind, room, e, along, y, note="", outside=False):
    """A fixture on a room edge, `along` metres from the edge's first point, `y` above the floor.
    Stored 15 cm into the room from the wall's centre line (20 cm out of it for outdoor points):
    the construction plugin puts it on the nearest wall face."""
    a, d, l, inward = room.edge_frame(e)
    assert 0.25 <= along <= l - 0.25, f"{kind} too close to the end of {room.name} edge {e}: {along} of {l}"
    n = (-inward[0], -inward[1]) if outside else inward
    off = 0.2 if outside else 0.15
    x, z = a[0] + d[0] * along + n[0] * off, a[1] + d[1] * along + n[1] * off
    v = {"id": uid(), "kind": kind, "level": room.level, "note": note, "position": [r4(x), y, r4(z)], "rotation": rotation_for(n)}
    if not outside:
        v["wall"] = room.host(e)
    return v


def on_ceiling(room, x, z, note=""):
    return {"id": uid(), "kind": "ceilingLight", "level": room.level, "note": note, "position": [x, HEIGHTS[room.level], z], "rotation": 0}


def free(kind, level, x, y, z, facing, note=""):
    """A fixture that is not mounted on a wall (radiator, boiler, floor drain): position as given."""
    return {"id": uid(), "kind": kind, "level": level, "note": note, "position": [x, y, z], "rotation": rotation_for(facing)}


N, S, E, W = (0, -1), (0, 1), (1, 0), (-1, 0)   # facings in plan (north is -z)


# ---- components ----------------------------------------------------------------------------------

def unit(name, type_, level, origin, bearing, values=None, choices=None):
    return {"bearing": bearing, "choices": choices or {}, "id": uid(), "level": level, "name": name,
            "origin": [r4(origin[0]), r4(origin[1])], "type": type_, "values": values or {}}


SOFA = r"""// @name Sofa
// @group Living
// A box-built sofa: feet, a base, seat and back cushions, arms on either side.
component('sofa', {
  name: 'Sofa', wants: 0.80,
  params: [
    { name: 'width',      label: 'Width',       kind: 'length', value: 2.00, min: 0.80, max: 3.20 },
    { name: 'depth',      label: 'Depth',       kind: 'length', value: 0.90, min: 0.70, max: 1.10 },
    { name: 'seatHeight', label: 'Seat height', kind: 'length', value: 0.44, min: 0.36, max: 0.52 },
    { name: 'arms',   label: 'Arms',   kind: 'choice', value: 'both', choices: ['both', 'left', 'right', 'none'] },
    { name: 'fabric', label: 'Fabric', kind: 'choice', value: 'sage', choices: ['sage', 'charcoal', 'sand'] },
    { name: 'seats',  label: 'Seats',  kind: 'derived' }
  ],
  rules: function (p) {
    var arms = p.arms === 'both' ? 2 : (p.arms === 'none' ? 0 : 1);
    p.seats = Math.max(1, Math.floor((p.width - arms * 0.16) / 0.6 + 0.001));   // about 60 cm a person
    return p;
  },
  build: function (p) {
    var fabric = { sage: '#8A9A7B', charcoal: '#4A4D52', sand: '#CDBA96' }[p.fabric] || '#8A9A7B';
    var arm = 0.16, back = 0.20, feet = 0.08, top = 0.86, half = p.width / 2, out = [];
    // +x is the left of someone standing in front of the sofa
    var left = p.arms === 'both' || p.arms === 'left', right = p.arms === 'both' || p.arms === 'right';
    var xs = [-half + 0.03, half - 0.07], zs = [0.03, p.depth - 0.07];
    for (var i = 0; i < 2; i++) for (var j = 0; j < 2; j++)
      out.push(box({ x: xs[i], y: 0, z: zs[j], w: 0.04, h: feet, d: 0.04, material: '#3B2F25' }));
    var base = p.seatHeight - 0.14;
    out.push(box({ x: -half, y: feet, z: 0, w: p.width, h: base - feet, d: p.depth, material: fabric }));
    out.push(box({ x: -half, y: base, z: 0, w: p.width, h: top - base, d: back, material: fabric }));
    var x0 = -half + (right ? arm : 0), x1 = half - (left ? arm : 0);
    if (right) out.push(box({ x: -half, y: base, z: back, w: arm, h: 0.62 - base, d: p.depth - back, material: fabric }));
    if (left) out.push(box({ x: half - arm, y: base, z: back, w: arm, h: 0.62 - base, d: p.depth - back, material: fabric }));
    var seat = (x1 - x0) / p.seats;
    for (var k = 0; k < p.seats; k++) {
      var x = x0 + k * seat;
      out.push(box({ x: x + 0.005, y: base, z: back, w: seat - 0.01, h: 0.14, d: p.depth - back - 0.02, material: fabric }));
      out.push(box({ x: x + 0.005, y: p.seatHeight, z: back, w: seat - 0.01, h: 0.38, d: 0.14, material: fabric }));
    }
    return out;
  }
});
"""

DINING_TABLE = r"""// @name Dining table
// @group Living
// A four-legged table, optionally with chairs along both long sides. The origin is the back of
// the chairs on one side, so the footprint includes the room they need.
component('dining-table', {
  name: 'Dining table', wants: 0.90,
  params: [
    { name: 'length', label: 'Length', kind: 'length', value: 1.80, min: 0.80, max: 3.00 },
    { name: 'width',  label: 'Width',  kind: 'length', value: 0.90, min: 0.60, max: 1.20 },
    { name: 'height', label: 'Height', kind: 'length', value: 0.75, min: 0.65, max: 0.80 },
    { name: 'chairs', label: 'Chairs', kind: 'choice', value: 'yes', choices: ['yes', 'no'] },
    { name: 'places', label: 'Places', kind: 'derived' }
  ],
  rules: function (p) {
    p.places = p.chairs === 'yes' ? 2 * Math.max(1, Math.floor(p.length / 0.6)) : 0;
    return p;
  },
  build: function (p) {
    var oak = '#B08A5E', out = [], half = p.length / 2, leg = 0.06, top = 0.03;
    var z0 = p.chairs === 'yes' ? 0.45 : 0, z1 = z0 + p.width;
    out.push(box({ x: -half, y: p.height - top, z: z0, w: p.length, h: top, d: p.width, material: oak }));
    var lx = [-half + 0.05, half - 0.05 - leg], lz = [z0 + 0.05, z1 - 0.05 - leg];
    for (var i = 0; i < 2; i++) for (var j = 0; j < 2; j++)
      out.push(box({ x: lx[i], y: 0, z: lz[j], w: leg, h: p.height - top, d: leg, material: oak }));
    if (p.chairs === 'yes') {
      var per = Math.max(1, Math.floor(p.length / 0.6)), pitch = p.length / per;
      for (var k = 0; k < per; k++) {
        var cx = -half + pitch * (k + 0.5) - 0.21;
        // a chair on the near side (back at z = 0) and one facing it across the table
        out.push(box({ x: cx, y: 0.42, z: 0.04, w: 0.42, h: 0.04, d: 0.40, material: 'Timber' }));
        out.push(box({ x: cx, y: 0.46, z: 0.0, w: 0.42, h: 0.42, d: 0.04, material: 'Timber' }));
        out.push(box({ x: cx, y: 0.42, z: z1 + 0.01, w: 0.42, h: 0.04, d: 0.40, material: 'Timber' }));
        out.push(box({ x: cx, y: 0.46, z: z1 + 0.41, w: 0.42, h: 0.42, d: 0.04, material: 'Timber' }));
        out.push(box({ x: cx + 0.02, y: 0, z: 0.06, w: 0.38, h: 0.42, d: 0.36, material: '#7A6248' }));
        out.push(box({ x: cx + 0.02, y: 0, z: z1 + 0.03, w: 0.38, h: 0.42, d: 0.36, material: '#7A6248' }));
      }
    }
    return out;
  }
});
"""

BED = r"""// @name Bed
// @group Bedroom
// A bed frame with a headboard against the wall, mattress, duvet and pillows.
component('bed', {
  name: 'Bed', wants: 0.70,
  params: [
    { name: 'mattress',  label: 'Mattress width', kind: 'choice', value: '1.6', choices: ['0.9', '1.2', '1.4', '1.6', '1.8'] },
    { name: 'length',    label: 'Mattress length', kind: 'length', value: 2.00, min: 1.90, max: 2.20 },
    { name: 'headboard', label: 'Headboard height', kind: 'length', value: 1.00, min: 0.60, max: 1.30 },
    { name: 'overall',   label: 'Overall width (m)', kind: 'derived' }
  ],
  rules: function (p) {
    p.overall = Math.round((parseFloat(p.mattress) + 0.10) * 1000) / 1000;
    return p;
  },
  build: function (p) {
    var m = parseFloat(p.mattress) || 1.6, half = m / 2 + 0.05, out = [];
    var frame = 'Timber', linen = '#F1EEE6', duvet = '#9FB4C7';
    out.push(box({ x: -half, y: 0, z: 0, w: 2 * half, h: p.headboard, d: 0.06, material: frame }));
    out.push(box({ x: -half, y: 0.10, z: 0.06, w: 2 * half, h: 0.20, d: p.length + 0.04, material: frame }));
    out.push(box({ x: -m / 2, y: 0.30, z: 0.08, w: m, h: 0.20, d: p.length, material: linen }));
    out.push(box({ x: -m / 2 - 0.01, y: 0.50, z: 0.60, w: m + 0.02, h: 0.06, d: p.length - 0.50, material: duvet }));
    var pillows = m >= 1.2 ? 2 : 1, pw = (m - 0.10) / pillows;
    for (var i = 0; i < pillows; i++)
      out.push(box({ x: -m / 2 + 0.05 + i * pw + 0.02, y: 0.50, z: 0.12, w: pw - 0.04, h: 0.10, d: 0.40, material: linen }));
    var lx = [-half, half - 0.06], lz = [0.06, p.length + 0.04];
    for (var a = 0; a < 2; a++) for (var b = 0; b < 2; b++)
      out.push(box({ x: lx[a], y: 0, z: lz[b], w: 0.06, h: 0.10, d: 0.06, material: frame }));
    return out;
  }
});
"""

COMPONENT_TYPES = [
    {"group": "Living", "id": "sofa", "name": "Sofa", "source": SOFA},
    {"group": "Living", "id": "dining-table", "name": "Dining table", "source": DINING_TABLE},
    {"group": "Bedroom", "id": "bed", "name": "Bed", "source": BED},
]


# ---- site shapes -------------------------------------------------------------------------------------

class Brep:
    def __init__(self):
        self.points, self.edges, self.faces = [], [], []

    def _pt(self, p):
        self.points.append([r4(p[0]), r4(p[1]), r4(p[2])])
        return len(self.points) - 1

    def loop(self, pts):
        idx = [self._pt(p) for p in pts]
        for i in range(len(idx)):
            self.edges.append({"a": idx[i], "b": idx[(i + 1) % len(idx)]})
        return idx

    def face(self, outer, holes=(), finish=None):
        f = {"outer": self.loop(outer)}
        if holes:
            f["holes"] = [self.loop(h) for h in holes]
        if finish:
            f["finish"] = finish
        self.faces.append(f)

    def prism(self, plan, y0, y1, finish=None):
        """Top and sides of a vertical prism over a plan polygon (x, z)."""
        self.face([(x, y1, z) for x, z in plan], finish=finish)
        n = len(plan)
        for i in range(n):
            (ax, az), (bx, bz) = plan[i], plan[(i + 1) % n]
            self.face([(ax, y0, az), (bx, y0, bz), (bx, y1, bz), (ax, y1, az)], finish=finish)

    def json(self):
        return {"edges": self.edges, "faces": self.faces, "points": self.points}


def rect_plan(x0, z0, x1, z1):
    return [(x0, z0), (x1, z0), (x1, z1), (x0, z1)]


def ring(cx, cz, r, n=8):
    return [(cx + r * math.cos(2 * math.pi * (i + 0.5) / n), cz + r * math.sin(2 * math.pi * (i + 0.5) / n)) for i in range(n)]


def shape(name, brep, finish=None, role=None, level=True):
    v = {"brep": brep.json(), "id": uid(), "name": name}
    if level:
        v["level"] = 0
    if finish:
        v["finish"] = finish
    if role:
        v["role"] = role
    return v


def flat(name, plan, y, finish, holes=()):
    b = Brep()
    b.face([(x, y, z) for x, z in plan], holes=[[(x, y, z) for x, z in h] for h in holes])
    return shape(name, b, finish)


def tree(name, x, z, trunk, crown, height):
    b = Brep()
    b.prism(ring(x, z, trunk), 0, height * 0.4, finish="bark")
    b.prism(ring(x, z, crown), height * 0.4, height, finish="foliage")
    return shape(name, b)


def site(main_roof):
    shapes = []
    house_outline = [(-3.8, 0.6), (-0.2, 0.6), (-0.2, -0.2), (3.4, -0.2), (3.4, -3.2), (9.8, -3.2), (9.8, 7.6),
                     (-0.8, 7.6), (-0.8, 12.5), (-3.8, 12.5)]
    studio_outline = rect_plan(1.8, -9.2, 6.2, -4.6)
    shapes.append(flat("Lawn", rect_plan(-9.0, -13.0, 15.0, 13.0), -0.02, "grass", holes=[house_outline, studio_outline]))
    shapes.append(flat("Driveway", rect_plan(-3.8, 8.6, -0.8, 12.5), 0.0, "pavement"))
    shapes.append(flat("Terrace", rect_plan(3.4, -3.2, 9.8, -0.2), 0.0, "pavement"))
    shapes.append(flat("Front path", rect_plan(1.2, 7.6, 2.2, 13.0), 0.0, "pavement"))
    shapes.append(flat("Garden path", rect_plan(4.0, -4.6, 5.0, -3.2), 0.0, "gravel"))
    shapes.append(flat("Studio deck", rect_plan(1.8, -5.4, 6.2, -4.6), 0.02, "wood"))
    shapes.append(flat("Street", rect_plan(-9.0, 13.0, 15.0, 19.0), 0.0, "pavement"))
    for name, plan, h in [("Hedge east", rect_plan(14.6, -13.0, 15.0, 13.0), 1.6), ("Hedge west", rect_plan(-9.0, -13.0, -8.6, 8.0), 1.4),
                          ("Hedge north", rect_plan(-8.6, -13.0, 14.6, -12.6), 1.8)]:
        b = Brep()
        b.prism(plan, 0, h)
        shapes.append(shape(name, b, "hedge"))
    shapes.append(tree("Birch", 11.5, -8.0, 0.12, 1.4, 7.0))
    shapes.append(tree("Apple tree", -5.0, -6.0, 0.10, 1.8, 4.5))
    shapes.append(tree("Field maple", 12.0, 9.0, 0.15, 2.2, 8.0))
    b = Brep()
    b.prism(rect_plan(2.4, 12.6, 2.55, 12.75), 0, 1.1)
    shapes.append(shape("Letterbox", b, "black"))
    b = Brep()
    b.prism(rect_plan(-6.5, 10.5, -4.9, 11.3), 0, 1.3)
    shapes.append(shape("Bin store", b, "wood"))
    # ten solar panels on the south slope of the main roof, 15 cm above the covering
    (x0, z0), (x1, z1), rise, ridge_z, eave_z, top = main_roof
    k = rise / (eave_z - ridge_z)
    def slope_y(z):
        return top + (eave_z - z) * k + 0.15
    b = Brep()
    pz0, pz1 = ridge_z + 0.5, eave_z - 0.6
    b.face([(1.0, slope_y(pz1), pz1), (8.6, slope_y(pz1), pz1), (8.6, slope_y(pz0), pz0), (1.0, slope_y(pz0), pz0)], finish="black")
    shapes.append(shape("Solar panels", b, "black"))
    # a surveyor's boundary line (edges only, nothing to show) and a cut-out that is not built
    b = Brep()
    b.points = [[-9.0, 0, 19.0], [15.0, 0, 19.0], [15.0, 0, -13.0]]
    b.edges = [{"a": 0, "b": 1}, {"a": 1, "b": 2}]
    shapes.append(shape("Boundary", b, level=False))
    shapes.append(flat("Pond (planned)", ring(-5.0, 2.0, 1.2), -0.3, "gravel"))
    shapes[-1]["role"] = "void"
    return shapes


# ---- the full house -----------------------------------------------------------------------------------

def build_full():
    # ground floor
    utility = Room("Utility room", 0, rect=((0, 0), (3.6, 2.2)))
    hall = Room("Hall", 0, rect=((0, 2.2), (2.4, 5.2)))
    wc = Room("WC", 0, rect=((2.4, 2.2), (1.2, 1.8)))
    kitchen = Room("Kitchen", 0, rect=((2.4, 4.0), (4.2, 3.4)))
    living = Room("Living room", 0, poly=[(3.6, 0), (9.6, 0), (9.6, 7.4), (6.6, 7.4), (6.6, 4.0), (3.6, 4.0)])
    garage = Room("Garage", 0, poly=[(-3.6, 0.8), (0, 0.8), (0, 6.4), (-1.0, 6.4), (-1.0, 8.4), (-3.6, 8.4)], wall_height=2.5)
    studio = Room("Garden studio", 0, rect=((2.0, -9.0), (4.0, 3.4)), wall_height=2.5, has_ceiling=True)
    # first floor
    bathroom = Room("Bathroom", 1, rect=((0, 0), (3.0, 2.2)))
    study = Room("Study", 1, rect=((3.0, 0), (2.6, 3.2)))
    bed1 = Room("Bedroom 1", 1, rect=((5.6, 0), (4.0, 4.05)))
    bed2 = Room("Bedroom 2", 1, rect=((5.6, 4.05), (4.0, 3.35)))
    bed3 = Room("Bedroom 3", 1, rect=((2.4, 4.9), (3.2, 2.5)))
    shower = Room("Shower room", 1, rect=((0, 4.9), (2.4, 2.5)))
    landing = Room("Landing", 1, poly=[(0, 2.2), (3.0, 2.2), (3.0, 3.2), (5.6, 3.2), (5.6, 4.9), (0, 4.9)])

    rects = [utility, hall, wc, kitchen, studio, bathroom, study, bed1, bed2, bed3, shower]
    polys = [garage, living, landing]
    rooms = {r.name + "@" + str(r.level): r for r in rects + polys}

    # free walls: a store in the garage, a linen cupboard on the landing, two garden walls
    store_a = free_wall(0, (-3.6, 2.0), (-2.4, 2.0))
    store_b = free_wall(0, (-2.4, 0.8), (-2.4, 2.0))
    cupboard_w = free_wall(1, (3.2, 3.2), (3.2, 3.8))
    cupboard_s = free_wall(1, (3.2, 3.8), (4.0, 3.8))
    cupboard_e = free_wall(1, (4.0, 3.2), (4.0, 3.8))
    garden_wall = free_wall(0, (-4.4, 9.0), (-4.4, 12.5), height=1.0, exterior=True)
    boundary_wall = free_wall(0, (10.6, -2.0), (10.6, 6.0), height=1.8, exterior=True)
    walls = [store_a, store_b, garden_wall, boundary_wall, cupboard_w, cupboard_s, cupboard_e]

    D, WIN, PASS, GAR = "door", "window", "passage", "garage"
    openings = [
        opening(utility, 0, 0.95, D, 0.9, 2.1),                 # back door to the garden
        opening(utility, 0, 2.8, WIN, 0.8, 1.0, 1.1),
        opening(utility, 1, 1.1, D, 0.8, 2.05),                 # to the living room
        opening(hall, 2, 0.7, D, 1.0, 2.2),                     # front door
        opening(hall, 1, 1.3, D, 0.73, 2.05),                   # WC
        opening(hall, 1, 3.2, D, 0.83, 2.05),                   # kitchen
        opening(hall, 3, 0.5, WIN, 0.6, 1.2, 1.0),
        opening(kitchen, 2, 2.55, WIN, 0.9, 1.1, 1.05),         # over the sink
        opening(kitchen, 0, 2.5, PASS, 1.4, 2.2),               # open to the living room
        opening(living, 0, 2.6, D, 2.4, 2.2),                   # sliding door to the terrace
        opening(living, 0, 4.9, WIN, 1.6, 1.5, 0.7),
        opening(living, 1, 2.0, WIN, 1.2, 1.5, 0.8),
        opening(living, 1, 5.6, WIN, 1.2, 1.4, 0.9),
        opening(living, 2, 1.5, WIN, 1.8, 1.5, 0.7),
        opening(garage, 4, 1.3, GAR, 2.3, 2.1),
        opening(garage, 3, 1.0, D, 0.9, 2.05),
        opening(garage, 5, 4.0, WIN, 1.0, 0.8, 1.3),
        opening(garage, 1, 0.85, D, 0.8, 2.05),                 # garage to utility room
        wall_opening(store_b, 0.6, D, 0.7, 2.0),
        opening(studio, 2, 2.0, D, 1.6, 2.1),
        opening(studio, 0, 2.0, WIN, 2.0, 1.2, 0.9),
        opening(studio, 1, 1.7, WIN, 0.9, 1.1, 1.0),
        opening(landing, 0, 2.47, D, 0.73, 2.05),               # bathroom
        opening(landing, 2, 2.0, D, 0.73, 2.05),                # study
        opening(landing, 3, 0.42, D, 0.73, 2.05),               # bedroom 1
        opening(landing, 3, 1.28, D, 0.73, 2.05),               # bedroom 2
        opening(landing, 4, 1.0, D, 0.73, 2.05),                # bedroom 3
        opening(landing, 4, 3.6, D, 0.73, 2.05),                # shower room
        opening(landing, 5, 1.0, WIN, 0.6, 1.0, 1.2),           # over the stair
        wall_opening(cupboard_s, 0.4, D, 0.6, 2.0),
        opening(bathroom, 0, 1.5, WIN, 0.8, 1.0, 1.3),
        opening(study, 0, 1.3, WIN, 1.2, 1.3, 0.9),
        opening(bed1, 0, 2.0, WIN, 1.6, 1.3, 0.9),
        opening(bed2, 2, 2.0, WIN, 1.6, 1.3, 0.9),
        opening(bed3, 2, 1.6, WIN, 1.4, 1.3, 0.9),
        opening(shower, 2, 1.4, WIN, 0.6, 0.9, 1.4),
    ]

    F = {}   # named electrical fixtures, for the circuits

    def add(key, f):
        F[key] = f
        return f

    fx = [
        # utility room: the board, the router, laundry
        add("panel", on_wall("panel", utility, 3, 1.8, 1.3, "Distribution board")),
        add("u.light", on_ceiling(utility, 1.8, 1.1)),
        add("u.switch", on_wall("switchPlate", utility, 1, 1.75, 1.1)),
        add("u.washer", on_wall("outlet", utility, 2, 2.6, 1.0, "Washing machine")),
        add("u.dryer", on_wall("outlet", utility, 2, 2.0, 1.0, "Tumble dryer")),
        add("u.boiler", on_wall("outlet", utility, 2, 0.5, 0.9, "Boiler")),
        add("u.socket", on_wall("outlet", utility, 0, 1.9, 1.1, "double")),
        add("u.data", on_wall("dataPoint", utility, 3, 1.2, 1.3, "Router")),
        on_wall("tapCold", utility, 2, 2.6, 0.9, "Laundry supply"),
        on_wall("drainPoint", utility, 2, 2.6, 0.6, "Laundry waste"),
        on_wall("tapCold", utility, 0, 2.8, 0.55),
        on_wall("tapHot", utility, 0, 2.8, 0.55),
        free("boiler", 0, 3.1, 1.5, 2.05, N, "Gas condensing boiler"),
        # hall
        add("h.light1", on_ceiling(hall, 1.6, 3.6)),
        add("h.light2", on_ceiling(hall, 1.7, 6.3)),
        add("h.switch1", on_wall("switchPlate", hall, 1, 4.9, 1.1)),
        add("h.switch2", on_wall("switchPlate", hall, 3, 2.6, 1.1)),
        add("h.socket", on_wall("outlet", hall, 1, 2.0, 0.3)),
        add("porch.light", on_wall("wallLight", hall, 2, 1.6, 2.0, "Porch light", outside=True)),
        free("radiator", 0, 2.25, 0.12, 6.4, W),
        # WC
        add("wc.light", on_ceiling(wc, 3.0, 3.1)),
        add("wc.switch", on_wall("switchPlate", wc, 2, 0.85, 1.1)),
        on_wall("toiletSupply", wc, 1, 0.8, 0.15),
        on_wall("drainPoint", wc, 1, 0.8, 0.1, "WC waste"),
        on_wall("tapCold", wc, 0, 0.5, 0.55),
        on_wall("drainPoint", wc, 0, 0.5, 0.45),
        # kitchen
        add("k.light1", on_ceiling(kitchen, 3.6, 5.4)),
        add("k.light2", on_ceiling(kitchen, 5.4, 5.4)),
        add("k.switch", on_wall("switchPlate", kitchen, 3, 1.35, 1.1)),
        add("k.socket1", on_wall("outlet", kitchen, 2, 3.25, 1.1, "double")),
        add("k.socket2", on_wall("outlet", kitchen, 2, 0.3, 1.1, "double")),
        add("k.fridge", on_wall("outlet", kitchen, 2, 3.85, 0.3, "Fridge")),
        add("k.socket3", on_wall("outlet", kitchen, 1, 1.0, 0.3)),
        add("k.extractor", on_wall("outlet", kitchen, 2, 1.15, 2.3, "Extractor hood")),
        add("k.dishwasher", on_wall("outlet", kitchen, 2, 1.85, 0.3, "Dishwasher")),
        add("k.oven", on_wall("outlet", kitchen, 2, 0.4, 0.3, "Oven")),
        add("k.hob", on_wall("outlet", kitchen, 2, 1.15, 0.5, "Induction hob")),
        on_wall("tapCold", kitchen, 2, 2.55, 0.55),
        on_wall("tapHot", kitchen, 2, 2.55, 0.55),
        on_wall("drainPoint", kitchen, 2, 2.55, 0.45, "Sink waste"),
        free("radiator", 0, 2.5, 0.12, 4.5, E),
        # living room
        add("l.light1", on_ceiling(living, 5.1, 1.9, "Over the dining table")),
        add("l.light2", on_ceiling(living, 8.1, 1.9)),
        add("l.light3", on_ceiling(living, 8.1, 5.7)),
        add("l.wall1", on_wall("wallLight", living, 0, 0.7, 1.9)),
        add("l.wall2", on_wall("wallLight", living, 1, 4.6, 1.9)),
        add("l.switch1", on_wall("switchPlate", living, 4, 0.7, 1.1)),
        add("l.switch2", on_wall("switchPlate", living, 5, 2.4, 1.1)),
        add("l.switch3", on_wall("switchPlate", living, 2, 2.6, 1.1)),
        add("l.a1", on_wall("outlet", living, 0, 1.1, 0.3, "double")),
        add("l.a2", on_wall("outlet", living, 0, 4.0, 0.3)),
        add("l.a3", on_wall("outlet", living, 1, 1.0, 0.3)),
        add("l.a4", on_wall("outlet", living, 1, 3.4, 0.3, "double")),
        add("l.tv", on_wall("dataPoint", living, 1, 3.7, 0.3, "TV")),
        add("l.b1", on_wall("outlet", living, 1, 6.6, 0.3)),
        add("l.b2", on_wall("outlet", living, 2, 2.4, 0.3)),
        add("l.b3", on_wall("outlet", living, 3, 1.5, 0.3, "double")),
        add("l.b4", on_wall("outlet", living, 5, 3.6, 0.3)),
        add("garden.socket", on_wall("outlet", living, 0, 5.6, 0.5, "Garden socket", outside=True)),
        add("terrace.light", on_wall("wallLight", living, 0, 1.0, 2.2, "Terrace light", outside=True)),
        on_wall("thermostat", living, 3, 2.8, 1.5),
        free("radiator", 0, 8.5, 0.12, 0.2, S),
        free("radiator", 0, 8.1, 0.12, 7.2, N),
        # garage
        add("g.light1", on_ceiling(garage, -2.3, 3.4)),
        add("g.light2", on_ceiling(garage, -2.3, 6.0)),
        add("g.switch", on_wall("switchPlate", garage, 1, 1.45, 1.1)),
        add("g.socket1", on_wall("outlet", garage, 5, 2.0, 1.1, "Workbench, double")),
        add("g.socket2", on_wall("outlet", garage, 5, 4.5, 1.1)),
        add("g.socket3", on_wall("outlet", garage, 0, 2.8, 1.1)),
        # landing
        add("la.light", on_ceiling(landing, 4.2, 4.05)),
        add("la.switch1", on_wall("switchPlate", landing, 1, 0.5, 1.1)),
        add("la.switch2", on_wall("switchPlate", landing, 4, 2.5, 1.1)),
        add("la.socket", on_wall("outlet", landing, 4, 3.0, 0.3)),
        # bathroom
        add("b.light", on_ceiling(bathroom, 1.5, 1.1)),
        add("b.mirror", on_wall("wallLight", bathroom, 1, 1.0, 1.9, "Mirror light")),
        add("b.switch", on_wall("switchPlate", bathroom, 2, 1.15, 1.1)),
        add("b.socket", on_wall("outlet", bathroom, 1, 1.75, 1.1, "Vanity")),
        add("b.fan", on_wall("outlet", bathroom, 2, 2.6, 2.3, "Ventilation fan")),
        on_wall("mixer", bathroom, 3, 1.1, 0.7, "Bath"),
        on_wall("tapCold", bathroom, 1, 1.0, 0.55),
        on_wall("tapHot", bathroom, 1, 1.0, 0.55),
        on_wall("drainPoint", bathroom, 1, 1.0, 0.45),
        on_wall("toiletSupply", bathroom, 0, 1.5, 0.15),
        on_wall("drainPoint", bathroom, 0, 1.5, 0.1, "WC waste"),
        free("radiator", 1, 1.6, 0.3, 2.1, N, "Towel radiator"),
        # shower room
        add("s.light", on_ceiling(shower, 1.2, 6.1)),
        add("s.switch", on_wall("switchPlate", shower, 1, 0.35, 1.1)),
        add("s.socket", on_wall("outlet", shower, 1, 1.0, 1.1, "Mirror cabinet")),
        on_wall("showerValve", shower, 3, 0.65, 1.1),
        on_wall("toiletSupply", shower, 3, 1.8, 0.15),
        on_wall("tapCold", shower, 1, 1.5, 0.55),
        on_wall("tapHot", shower, 1, 1.5, 0.55),
        on_wall("drainPoint", shower, 1, 1.5, 0.45),
        free("floorDrain", 1, 0.6, 0.0, 6.75, N),
        # study
        add("t.light", on_ceiling(study, 4.3, 1.6)),
        add("t.switch", on_wall("switchPlate", study, 2, 1.2, 1.1)),
        add("t.socket1", on_wall("outlet", study, 0, 0.45, 0.3)),
        add("t.socket2", on_wall("outlet", study, 1, 1.6, 0.3)),
        add("t.socket3", on_wall("outlet", study, 3, 1.6, 0.3, "double")),
        add("t.data", on_wall("dataPoint", study, 1, 1.9, 0.3)),
        free("radiator", 1, 4.3, 0.15, 0.2, S),
        # bedroom 1
        add("b1.light", on_ceiling(bed1, 7.6, 2.0)),
        add("b1.switch", on_wall("switchPlate", bed1, 3, 1.0, 1.1)),
        add("b1.socket1", on_wall("outlet", bed1, 0, 0.7, 0.3)),
        add("b1.socket2", on_wall("outlet", bed1, 1, 1.0, 0.6, "double")),
        add("b1.socket3", on_wall("outlet", bed1, 1, 3.1, 0.6, "double")),
        add("b1.socket4", on_wall("outlet", bed1, 2, 2.0, 0.3)),
        add("b1.data", on_wall("dataPoint", bed1, 2, 2.3, 0.3)),
        free("radiator", 1, 7.6, 0.15, 0.2, S),
        # bedroom 2
        add("b2.light", on_ceiling(bed2, 7.6, 5.7)),
        add("b2.switch", on_wall("switchPlate", bed2, 3, 2.3, 1.1)),
        add("b2.socket1", on_wall("outlet", bed2, 0, 0.8, 0.3)),
        add("b2.socket2", on_wall("outlet", bed2, 1, 0.75, 0.6, "double")),
        add("b2.socket3", on_wall("outlet", bed2, 1, 2.55, 0.6, "double")),
        free("radiator", 1, 7.6, 0.15, 7.2, N),
        # bedroom 3
        add("b3.light", on_ceiling(bed3, 4.0, 6.15)),
        add("b3.switch", on_wall("switchPlate", bed3, 0, 1.6, 1.1)),
        add("b3.socket1", on_wall("outlet", bed3, 0, 0.6, 0.3)),
        add("b3.socket2", on_wall("outlet", bed3, 3, 1.6, 0.6)),
        add("b3.socket3", on_wall("outlet", bed3, 1, 0.6, 0.3)),
        free("radiator", 1, 4.0, 0.15, 7.2, N),
    ]

    stair = {"bearing": 0, "going": 0.25, "id": uid(), "kind": "quarterTurn", "level": 0, "name": "Stair", "origin": [0.59, 4.025],
             "risers": 15, "turnAfter": 4, "turnsRight": True, "width": 0.9, "winders": 6}

    # components: built-in catalogue types plus the three scripts above
    kz = 7.4 - 0.12            # kitchen: inner face of the street wall
    components = [
        unit("Fridge", "tall-unit", 0, (2.75, kz), 0, {"width": 0.6}, {"appliance": "fridge"}),
        unit("Drawer unit", "base-unit", 0, (3.35, kz), 0, {"width": 0.6}, {"front": "drawers"}),
        unit("Sink unit", "base-unit", 0, (4.05, kz), 0, {"width": 0.8}, {"appliance": "sink"}),
        unit("Dishwasher", "base-unit", 0, (4.75, kz), 0, {"width": 0.6}, {"appliance": "dishwasher"}),
        unit("Hob unit", "base-unit", 0, (5.45, kz), 0, {"width": 0.8}, {"appliance": "hob", "front": "drawers"}),
        unit("Oven unit", "base-unit", 0, (6.2, kz), 0, {"width": 0.7}, {"appliance": "oven"}),
        unit("Wall cupboard", "wall-unit", 0, (3.3, kz), 0, {"width": 0.5}),
        unit("Wall cupboard", "wall-unit", 0, (4.8, kz), 0, {"width": 0.5}, {"front": "glass"}),
        unit("Extractor hood", "extractor", 0, (5.45, kz), 0, {"width": 0.8}),
        unit("Wall cupboard", "wall-unit", 0, (6.2, kz), 0, {"width": 0.7}),
        unit("Kitchen table", "dining-table", 0, (4.2, 5.0), 180, {"length": 1.2, "width": 0.8}, {"chairs": "no"}),
        unit("Washing machine", "base-unit", 0, (1.0, 2.15), 0, {"width": 0.6}, {"appliance": "washer"}),
        unit("Tumble dryer", "base-unit", 0, (1.6, 2.15), 0, {"width": 0.6}, {"appliance": "washer"}),
        unit("Laundry cupboard", "wall-unit", 0, (1.3, 2.15), 0, {"width": 1.2}),
        unit("Broom cupboard", "tall-unit", 0, (2.3, 2.15), 0, {"width": 0.6}),
        unit("Utility sink", "basin", 0, (2.8, 0.12), 180, {"width": 0.5}),
        unit("WC", "wc", 0, (3.55, 3.0), 270),
        unit("Hand basin", "basin", 0, (2.9, 2.25), 180, {"width": 0.4, "depth": 0.3}),
        unit("Shoe cabinet", "shelving", 0, (2.35, 4.35), 270, {"width": 0.8, "height": 1.0, "depth": 0.3}),
        unit("Sofa", "sofa", 0, (6.65, 5.6), 90, {"width": 2.2}, {"fabric": "sage"}),
        unit("Armchair", "sofa", 0, (9.48, 6.6), 270, {"width": 0.95}, {"fabric": "sand"}),
        unit("TV cabinet", "shelving", 0, (9.48, 4.4), 270, {"width": 1.2, "height": 0.5, "depth": 0.4}),
        unit("Dining table", "dining-table", 0, (5.1, 1.0), 180, {"length": 1.8}),
        unit("Bookcase", "shelving", 0, (3.65, 3.2), 90, {"width": 1.1, "height": 2.0}),
        unit("Workbench", "worktop", 0, (-3.48, 6.4), 90, {"width": 1.8, "depth": 0.7, "at": 0.86}),
        unit("Garage shelving", "shelving", 0, (-3.48, 3.0), 90, {"width": 1.8, "height": 2.0, "depth": 0.5}),
        unit("Garage cupboard", "tall-unit", 0, (-1.2, 0.92), 180, {"width": 0.6}),
        unit("Studio sofa", "sofa", 0, (4.0, -8.88), 180, {"width": 1.8}, {"fabric": "charcoal", "arms": "both"}),
        unit("Studio armchair", "sofa", 0, (2.12, -6.4), 90, {"width": 0.9}, {"fabric": "sand"}),
        unit("Studio desk", "worktop", 0, (5.88, -7.3), 270, {"width": 1.6, "depth": 0.7, "at": 0.72}),
        unit("Studio bookcase", "shelving", 0, (2.12, -8.0), 90, {"width": 1.2, "height": 2.0}),
        unit("Bath", "bath", 1, (0.12, 1.135), 90),
        unit("Vanity", "double-vanity", 1, (2.95, 1.0), 270, {"width": 1.2}),
        unit("Toilet", "wc", 1, (1.5, 0.12), 180),
        unit("Shower", "shower", 1, (0.12, 6.75), 90, {"width": 0.9, "depth": 0.9}, {"screen": "left"}),
        unit("Toilet", "wc", 1, (0.12, 5.6), 90),
        unit("Basin", "basin", 1, (2.35, 6.4), 270, {"width": 0.5}),
        unit("Desk", "worktop", 1, (4.3, 0.12), 180, {"width": 1.6, "depth": 0.7, "at": 0.72}),
        unit("Bookcase", "shelving", 1, (5.55, 2.575), 270, {"width": 1.1, "height": 2.2}),
        unit("Double bed", "bed", 1, (9.48, 2.05), 270, choices={"mattress": "1.6"}),
        unit("Wardrobe", "wardrobe", 1, (5.65, 1.5), 90, {"width": 2.4}),
        unit("Double bed", "bed", 1, (9.48, 5.7), 270, choices={"mattress": "1.4"}),
        unit("Wardrobe", "wardrobe", 1, (7.7, 4.1), 180, {"width": 1.8}),
        unit("Single bed", "bed", 1, (2.45, 6.4), 90, choices={"mattress": "0.9"}),
        unit("Wardrobe", "wardrobe", 1, (5.55, 6.7), 270, {"width": 1.2}),
    ]

    finishes = {}
    def fin(room, key, value):
        kind = "room" if room.rect else "poly"
        finishes[key.format(id=room.id, kind=kind)] = value
    for r in (kitchen, wc, bathroom, shower):
        fin(r, "floor.{id}", "tile")
    for r in (utility, garage):
        fin(r, "floor.{id}", "concrete")
    for r in (studio, study, bed1, bed2, bed3, landing):
        fin(r, "floor.{id}", "wood")
    for e in range(4):
        fin(bathroom, "wall.{kind}.{id}.%d.interior" % e, "paintBlue")
        fin(studio, "wall.{kind}.{id}.%d.exterior" % e, "cladding")
    for e in (0, 3, 4, 5):
        fin(garage, "wall.{kind}.{id}.%d.exterior" % e, "brickBuff")
    fin(living, "wall.{kind}.{id}.3.interior", "paintWarm")
    fin(bed2, "wall.{kind}.{id}.1.interior", "paintGreen")

    te = 0.24 / 2              # half the cavity wall: outer faces of the walls
    main_min, main_max = [-te, -te], [9.6 + te, 7.4 + te]
    roof_volumes = [
        {"id": uid(), "levels": 2, "max": main_max, "min": main_min, "overhang": 0.3, "pitchHeight": 2.7, "ridgeAlongX": True,
         "style": "gable", "verges": {"east": 0.05, "west": 0.05}},
        {"eave": 2.5, "frame": {"joist": [0.038, 0.184], "joistSpacing": 0.5, "purlin": [0.075, 0.175], "purlinsPerSlope": 1,
                                 "rafter": [0.063, 0.15], "rafterSpacing": 0.6, "wallPlate": [0.15, 0.075]},
         "highEdge": "east", "id": uid(), "levels": 1, "max": [0.0, 8.4 + te], "min": [-3.6 - te, 0.8 - te], "overhang": 0.2,
         "pitchHeight": 0.6, "ridgeAlongX": False, "style": "shed", "verges": {"north": 0.05, "south": 0.05}},
        {"id": uid(), "levels": 1, "max": [6.0 + te, -5.6 + te], "min": [2.0 - te, -9.0 - te], "overhang": 0.25, "pitchHeight": 1.0,
         "ridgeAlongX": True, "style": "gable"},
    ]
    guides = [
        {"id": uid(), "origin": [0.0, 0.0, 0.0]},
        {"direction": [1, 0, 0], "id": uid(), "origin": [0.0, 0.0, 7.4]},
        {"direction": [0, 0, 1], "id": uid(), "origin": [9.6, 0.0, 0.0]},
        {"id": uid(), "origin": [0.59, H0, 4.025]},
        {"direction": [0, 1, 0], "id": uid(), "origin": [4.0, 0.0, -7.3]},
    ]
    main_roof = (main_min, main_max, 2.7, (main_min[1] + main_max[1]) / 2, main_max[1], H0 + H1)
    house = {
        "cavity": {"cavity": 0.06, "enabled": True, "innerLeaf": 0.09, "outerLeaf": 0.09},
        "componentTypes": COMPONENT_TYPES,
        "components": components,
        "finishes": finishes,
        "fixtures": fx,
        "floorThickness": 0.06,
        "floors": [{"height": H0, "id": FLOOR_IDS[0], "name": "Ground floor"}, {"height": H1, "id": FLOOR_IDS[1], "name": "First floor"}],
        "guides": guides,
        "openings": openings,
        "polyRooms": [r.json() for r in polys],
        "roof": {"enabled": True, "overhang": 0.3, "pitchHeight": 2.7, "ridgeAlongX": True, "style": "gable", "thickness": 0.15},
        "roofVolumes": roof_volumes,
        "rooms": [r.json() for r in rects],
        "runs": [],
        "shapes": site(main_roof),
        "stairs": [stair],
        "storeyHeight": H0,
        "trim": {"brickGables": True, "chimney": False, "doorColour": "anthracite", "doorHandles": True, "downpipes": True,
                 "fasciaAndGutters": True, "fasciaColour": "anthracite", "frameColour": "anthracite", "gutterSize": 0.12,
                 "lintels": True, "sillDepth": 0.05, "windowSills": True},
        "units": [],
        "wallThickness": 0.1,
        "walls": walls,
    }

    # ---- the distribution board (Belgian 3N400 V, 40 A, 300 mA head) ----
    def c(name, type_, breaker, section, rcd, keys, **extra):
        v = {"breaker": breaker, "cableSection": section, "fixtureIDs": [F[k]["id"] for k in keys], "id": uid(), "name": name,
             "note": extra.pop("note", ""), "rcd": rcd, "type": type_}
        v.update(extra)
        return v
    circuits = [
        c("Lighting ground floor", "lighting", 16, 1.5, "none",
          ["u.light", "u.switch", "h.light1", "h.light2", "h.switch1", "h.switch2", "wc.light", "wc.switch", "k.light1", "k.light2", "k.switch"]),
        c("Lighting living room", "lighting", 16, 1.5, "none", ["l.light1", "l.light2", "l.light3", "l.wall1", "l.wall2", "l.switch1", "l.switch2", "l.switch3"]),
        c("Lighting first floor", "lighting", 16, 1.5, "none",
          ["la.light", "la.switch1", "la.switch2", "b.light", "b.mirror", "b.switch", "s.light", "s.switch", "t.light", "t.switch",
           "b1.light", "b1.switch", "b2.light", "b2.switch", "b3.light", "b3.switch"]),
        c("Lighting garage and outside", "lighting", 16, 1.5, "none", ["g.light1", "g.light2", "g.switch", "porch.light", "terrace.light"]),
        c("Kitchen sockets", "kitchen", 20, 2.5, "mA30", ["k.socket1", "k.socket2", "k.fridge", "k.socket3", "k.extractor"]),
        c("Dishwasher", "dishwasher", 16, 2.5, "mA30", ["k.dishwasher"]),
        c("Oven", "oven", 20, 2.5, "mA30", ["k.oven"]),
        c("Induction hob", "hob", 20, 4, "mA30", ["k.hob"], poles=4),
        c("Washing machine", "washingMachine", 16, 2.5, "mA30", ["u.washer"]),
        c("Tumble dryer", "dryer", 16, 2.5, "mA30", ["u.dryer"]),
        c("Sockets living room north", "sockets", 20, 2.5, "mA30", ["l.a1", "l.a2", "l.a3", "l.a4", "l.tv"]),
        c("Sockets living room south, hall, utility room", "sockets", 20, 2.5, "mA30",
          ["l.b1", "l.b2", "l.b3", "l.b4", "h.socket", "u.socket", "u.data", "u.boiler"]),
        c("Sockets garage and garden", "sockets", 20, 2.5, "mA30", ["g.socket1", "g.socket2", "g.socket3", "garden.socket"]),
        c("Bathroom and shower room", "bathroom", 20, 2.5, "mA30", ["b.socket", "b.fan", "s.socket"]),
        c("Sockets bedroom 1 and landing", "sockets", 20, 2.5, "mA30",
          ["b1.socket1", "b1.socket2", "b1.socket3", "b1.socket4", "b1.data", "la.socket"]),
        c("Sockets bedrooms 2 and 3", "sockets", 20, 2.5, "mA30", ["b2.socket1", "b2.socket2", "b2.socket3", "b3.socket1", "b3.socket2", "b3.socket3"]),
        c("Sockets study", "sockets", 20, 2.5, "mA30", ["t.socket1", "t.socket2", "t.socket3", "t.data"]),
        c("PV inverter", "pv", 20, 4, "mA300", [], curve="C", poles=4,
          note="Example installation: 10 panels of 400 Wp (4 kWp) on the south roof slope, three-phase inverter."),
    ]
    electrical = {"address": ADDRESS, "circuits": circuits, "defaultCableType": "XVB", "differentialType": "A", "earthing": "TT",
                  "hasMainDifferential": True, "mainBreaker": 40, "phases": 3, "shortCircuitKA": 6}
    return house, electrical, rooms


# ---- the simple house: an earlier sketch of the same plot ---------------------------------------------

def build_simple():
    utility = Room("Utility room", 0, rect=((0, 0), (3.6, 2.2)))
    hall = Room("Hall", 0, rect=((0, 2.2), (2.4, 5.2)))
    wc = Room("WC", 0, rect=((2.4, 2.2), (1.2, 1.8)))
    kitchen = Room("Kitchen", 0, rect=((2.4, 4.0), (4.2, 3.4)))
    living = Room("Living Room", 0, poly=[(3.6, 0), (9.6, 0), (9.6, 7.4), (6.6, 7.4), (6.6, 4.0), (3.6, 4.0)])
    porch = Room("Porch", 0, rect=((0.6, 7.4), (1.6, 1.2)), wall_height=2.4)
    bathroom = Room("Bathroom", 1, rect=((0, 0), (3.0, 2.2)))
    study = Room("Study", 1, rect=((3.0, 0), (2.6, 2.2)))
    landing = Room("Landing", 1, rect=((0, 2.2), (5.6, 2.7)))
    bed1 = Room("Bedroom 1", 1, rect=((5.6, 0), (4.0, 4.05)))
    bed2 = Room("Bedroom 2", 1, rect=((5.6, 4.05), (4.0, 3.35)))
    bed3 = Room("Bedroom 3", 1, rect=((2.4, 4.9), (3.2, 2.5)))
    shower = Room("Shower room", 1, rect=((0, 4.9), (2.4, 2.5)))
    rects = [utility, hall, wc, kitchen, porch, bathroom, study, landing, bed1, bed2, bed3, shower]

    D, WIN, PASS = "door", "window", "passage"
    openings = [
        opening(utility, 0, 0.95, D, 0.9, 2.1),
        opening(utility, 0, 2.8, WIN, 0.8, 1.0, 1.1),
        opening(living, 5, 2.9, D, 0.8, 2.05),                  # utility room to living room
        opening(hall, 2, 1.0, D, 0.9, 2.1),                     # hall to porch
        opening(hall, 1, 1.3, D, 0.73, 2.05),
        opening(hall, 1, 3.2, D, 0.83, 2.05),
        opening(hall, 3, 0.5, WIN, 0.6, 1.2, 1.0),
        opening(kitchen, 2, 2.55, WIN, 0.9, 1.1, 1.05),
        opening(kitchen, 0, 2.5, PASS, 1.4, 2.2),
        opening(living, 0, 2.6, D, 2.4, 2.2),
        opening(living, 0, 4.9, WIN, 1.6, 1.5, 0.7),
        opening(living, 1, 2.0, WIN, 1.2, 1.5, 0.8),
        opening(living, 1, 5.6, WIN, 1.2, 1.4, 0.9),
        opening(living, 2, 1.5, WIN, 1.8, 1.5, 0.7),
        opening(porch, 2, 0.8, D, 1.0, 2.2),                    # front door
        opening(porch, 1, 0.6, WIN, 0.5, 1.2, 0.9),
        opening(landing, 0, 2.47, D, 0.73, 2.05),
        opening(landing, 0, 4.3, D, 0.73, 2.05),
        opening(landing, 1, 1.42, D, 0.73, 2.05),
        opening(landing, 1, 2.28, D, 0.73, 2.05),
        opening(landing, 2, 1.0, D, 0.73, 2.05),
        opening(landing, 2, 3.6, D, 0.73, 2.05),
        opening(bathroom, 0, 1.5, WIN, 0.8, 1.0, 1.3),
        opening(study, 0, 1.3, WIN, 1.2, 1.3, 0.9),
        opening(bed1, 0, 2.0, WIN, 1.6, 1.3, 0.9),
        opening(bed2, 2, 2.0, WIN, 1.6, 1.3, 0.9),
        opening(bed3, 2, 1.6, WIN, 1.4, 1.3, 0.9),
        opening(shower, 2, 1.4, WIN, 0.6, 0.9, 1.4),
    ]
    fixtures = [
        on_wall("tapCold", kitchen, 2, 2.55, 0.55),
        on_wall("tapHot", kitchen, 2, 2.55, 0.55),
        on_wall("panel", utility, 3, 1.8, 1.3, "Distribution board"),
        on_ceiling(hall, 1.6, 4.4),
        on_wall("switchPlate", hall, 1, 4.9, 1.1),
        on_wall("outlet", kitchen, 2, 3.25, 1.1, "double"),
    ]
    light, switch, socket = fixtures[3], fixtures[4], fixtures[5]
    electrical = {"circuits": [
        {"breaker": 16, "cableSection": 1.5, "fixtureIDs": [light["id"], switch["id"]], "id": uid(), "name": "Lighting ground floor", "note": "", "rcd": "none", "type": "lighting"},
        {"breaker": 16, "cableSection": 1.5, "fixtureIDs": [], "id": uid(), "name": "Lighting first floor", "note": "", "rcd": "none", "type": "lighting"},
        {"breaker": 20, "cableSection": 2.5, "fixtureIDs": [socket["id"]], "id": uid(), "name": "Kitchen sockets", "note": "", "rcd": "mA30", "type": "kitchen"},
        {"breaker": 20, "cableSection": 2.5, "fixtureIDs": [], "id": uid(), "name": "Oven", "note": "", "rcd": "mA30", "type": "oven"},
        {"breaker": 20, "cableSection": 2.5, "fixtureIDs": [], "id": uid(), "name": "Sockets living room", "note": "", "rcd": "mA30", "type": "sockets"},
    ], "defaultCableType": "XVB", "differentialType": "A", "earthing": "TT", "hasMainDifferential": True, "mainBreaker": 40, "phases": 3}
    te = 0.12
    house = {
        "cavity": {"cavity": 0.06, "enabled": True, "innerLeaf": 0.09, "outerLeaf": 0.09},
        "electrical": electrical,
        "fixtures": fixtures,
        "floorThickness": 0.06,
        "floors": [{"height": H0, "id": FLOOR_IDS[0], "name": "Ground floor"}, {"height": H1, "id": FLOOR_IDS[1], "name": "First floor"}],
        "lot": {"driveway": True, "drivewayOffset": 0, "drivewayWidth": 3.0, "enabled": True, "hedge": True, "lawn": True,
                "max": [16, 14], "min": [-8, -10], "trees": True},
        "openings": openings,
        "polyRooms": [living.json()],
        "roof": {"enabled": True, "overhang": 0.3, "pitchHeight": 2.7, "ridgeAlongX": True, "style": "gable", "thickness": 0.15},
        "roofVolumes": [
            {"id": uid(), "levels": 2, "max": [9.6 + te, 7.4 + te], "min": [-te, -te], "overhang": 0.3, "pitchHeight": 2.7,
             "ridgeAlongX": True, "style": "gable"},
            {"id": uid(), "levels": 1, "max": [2.2 + te, 8.6 + te], "min": [0.6 - te, 7.4], "overhang": 0.15, "pitchHeight": 0.0,
             "ridgeAlongX": True, "style": "flat"},
        ],
        "rooms": [r.json() for r in rects],
        "runs": [],
        "savedViews": [{"id": uid(), "name": "House", "sliceEnabled": False, "sliceHeight": 1.3, "viewpoint": "iso"}],
        "storeyHeight": H0,
        "trim": {"brickGables": True, "chimney": False, "doorHandles": True, "downpipes": True, "fasciaAndGutters": True,
                 "gutterSize": 0.12, "lintels": True, "sillDepth": 0.05, "windowSills": True},
        "wallThickness": 0.1,
        "walls": [],
    }
    return house, living


def write(path, value):
    with open(path, "w") as f:
        json.dump(value, f, indent=1, sort_keys=True, ensure_ascii=False)
        f.write("\n")


def main():
    full, electrical, rooms = build_full()
    simple, living = build_simple()
    write(os.path.join(CONSTRUCTION, "house_current.json"), full)
    write(os.path.join(CONSTRUCTION, "house.json"), simple)
    elec_house = {k: v for k, v in full.items() if k not in ("components", "componentTypes", "shapes")}
    write(os.path.join(ELECTRICAL, "house.json"), elec_house)
    write(os.path.join(ELECTRICAL, "electrical.json"), electrical)

    # numbers the tests check, for reference
    print("simple house: %d rooms, %d openings, %d roofs, %d fixtures; living room %s"
          % (len(simple["rooms"]) + len(simple["polyRooms"]), len(simple["openings"]), len(simple["roofVolumes"]), len(simple["fixtures"]), living.id))
    print("full house: %d rooms, %d walls, %d openings, %d fixtures, %d roofs, %d stairs (%s), %d units, %d component types, %d shapes, %d guides"
          % (len(full["rooms"]) + len(full["polyRooms"]), len(full["walls"]), len(full["openings"]), len(full["fixtures"]),
             len(full["roofVolumes"]), len(full["stairs"]), full["stairs"][0]["id"], len(full["components"]), len(full["componentTypes"]),
             len(full["shapes"]), len(full["guides"])))
    print("garage area %.3f, circuits %d, circuit fixture ids %d"
          % (rooms["Garage@0"].area, len(electrical["circuits"]), sum(len(c["fixtureIDs"]) for c in electrical["circuits"])))
    print("floor area %.3f" % sum(r.area for r in rooms.values()))


if __name__ == "__main__":
    main()
