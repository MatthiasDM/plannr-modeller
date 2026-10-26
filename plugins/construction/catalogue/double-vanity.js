// @name Vanity
// @group Bathroom
component('double-vanity', {
  name: 'Vanity', needs: ['water', 'waste'], wants: 0.55,
  params: [
    { name: 'width',  label: 'Width',          kind: 'length', value: 1.20, min: 0.50, max: 2.40 },
    { name: 'depth',  label: 'Depth',          kind: 'length', value: 0.48, min: 0.35, max: 0.60 },
    { name: 'height', label: 'Cabinet height', kind: 'length', value: 0.50, min: 0.30, max: 0.80 },
    { name: 'lift',   label: 'Off the floor',  kind: 'length', value: 0.30, min: 0.00, max: 0.60 },
    { name: 'basins', label: 'Basins',         kind: 'derived' },
    { name: 'rim',    label: 'Basin rim (m)',  kind: 'derived' }
  ],
  rules: function (p) {
    // A basin wants about sixty centimetres of elbow room; the count follows.
    p.basins = Math.max(1, Math.min(3, Math.floor(p.width / 0.60)));
    // What you lean over, which is the number people actually argue about.
    p.rim = Math.round((p.lift + p.height + 0.03 + 0.14) * 100) / 100;
    return p;
  },
  build: function (p) {
    var out = [], half = p.width / 2, top = p.lift + p.height;
    var section = p.width / p.basins;
    out.push(box({ x: -half, y: p.lift, z: 0, w: p.width, h: p.height, d: p.depth - 0.019 }));
    out.push(box({ x: -half, y: top, z: 0, w: p.width, h: 0.03, d: p.depth, material: 'Sanitary' }));
    for (var i = 0; i < p.basins; i++) {
      var left = -half + i * section, mid = left + section / 2;
      var upper = p.height * 0.4, lower = p.height - upper;
      out.push(box({ x: left + 0.003, y: p.lift + lower + 0.003, z: p.depth - 0.019,
                     w: section - 0.006, h: upper - 0.006, d: 0.019, material: 'Front' }));
      out.push(box({ x: left + 0.003, y: p.lift + 0.003, z: p.depth - 0.019,
                     w: section - 0.006, h: lower - 0.006, d: 0.019, material: 'Front' }));
      out.push(box({ x: mid - 0.10, y: p.lift + lower + upper * 0.5, z: p.depth,
                     w: 0.20, h: 0.012, d: 0.015, material: 'Handle' }));
      out.push(box({ x: mid - 0.10, y: p.lift + lower * 0.5, z: p.depth,
                     w: 0.20, h: 0.012, d: 0.015, material: 'Handle' }));
      // The bowl sits on the top: a floor and four walls.
      var bw = Math.min(0.46, section - 0.10), bd = Math.min(0.36, p.depth - 0.10);
      var wall = 0.02, bh = 0.14, bx = mid - bw / 2, bz = p.depth - bd - 0.03, by = top + 0.03;
      out.push(box({ x: bx, y: by, z: bz, w: bw, h: 0.02, d: bd, material: 'Sanitary' }));
      out.push(box({ x: bx, y: by, z: bz, w: wall, h: bh, d: bd, material: 'Sanitary' }));
      out.push(box({ x: bx + bw - wall, y: by, z: bz, w: wall, h: bh, d: bd, material: 'Sanitary' }));
      out.push(box({ x: bx, y: by, z: bz, w: bw, h: bh, d: wall, material: 'Sanitary' }));
      out.push(box({ x: bx, y: by, z: bz + bd - wall, w: bw, h: bh, d: wall, material: 'Sanitary' }));
      // The tap, behind the bowl, with its spout reaching over it.
      out.push(box({ x: mid - 0.012, y: by, z: bz - 0.05, w: 0.024, h: 0.22, d: 0.024, material: 'Handle' }));
      out.push(box({ x: mid - 0.012, y: by + 0.20, z: bz - 0.05, w: 0.024, h: 0.02, d: 0.12, material: 'Handle' }));
    }
    return out;
  }
});
