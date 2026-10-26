// @name Base unit
// @group Kitchen
component('base-unit', {
  name: 'Base unit',
  params: [
    { name: 'width',     label: 'Width',          kind: 'length', value: 0.60, min: 0.15, max: 1.20 },
    { name: 'depth',     label: 'Depth',          kind: 'length', value: 0.60, min: 0.30, max: 0.70 },
    { name: 'height',    label: 'Carcass height', kind: 'length', value: 0.72, min: 0.40, max: 0.90 },
    { name: 'plinth',    label: 'Plinth',         kind: 'length', value: 0.12, min: 0.00, max: 0.30 },
    { name: 'front',     label: 'Front',          kind: 'choice', value: 'door',
      choices: ['door', 'drawers', 'open', 'none'] },
    { name: 'appliance', label: 'Appliance',      kind: 'choice', value: 'none',
      choices: ['none', 'sink', 'hob', 'dishwasher', 'washer', 'oven'] },
    { name: 'surface',   label: 'Worktop surface (m)', kind: 'derived' }
  ],
  rules: function (p) {
    p.surface = Math.round((p.plinth + p.height + 0.04) * 1000) / 1000;
    return p;
  },
  worktop: function (p) { return { at: p.plinth + p.height, depth: p.depth }; },
  wants: 0.90,
  needs: function (p) {
    if (p.appliance === 'sink') return ['water', 'waste'];
    if (p.appliance === 'dishwasher' || p.appliance === 'washer') return ['water', 'waste', 'power'];
    if (p.appliance === 'hob') return ['circuit', 'extractor'];
    if (p.appliance === 'oven') return ['circuit'];
    return [];
  },
  symbol: function (p) { return p.appliance === 'none' ? null : p.appliance; },
  build: function (p) {
    var out = [], r = 0.003, f = 0.019, half = p.width / 2;
    var low = p.plinth, top = p.plinth + p.height, surface = top + 0.04;
    if (p.plinth > 0.01) {
      // Set back from the front, so there is room for your toes.
      out.push(box({ x: -half, y: 0, z: 0, w: p.width, h: p.plinth, d: p.depth - 0.05 }));
    }
    // A hair narrower than the front, so the front reads as applied to it.
    out.push(box({ x: -half + r, y: low, z: 0, w: p.width - 2 * r, h: p.height, d: p.depth - f }));
    var leaves = [];
    if (p.front === 'drawers' && p.height > 0.5) {
      var shallow = p.height * 0.18, deep = (p.height - shallow) / 2;
      leaves = [deep, deep, shallow];          // from the floor up: the shallow one on top
    } else if (p.front === 'door' || p.front === 'drawers') {
      leaves = [p.height];
    }
    var bar = Math.min(p.width - 0.12, 0.32), y = low;
    for (var i = 0; i < leaves.length; i++) {
      var h = leaves[i];
      out.push(box({ x: -half + r, y: y + r, z: p.depth - f, w: p.width - 2 * r, h: h - 2 * r, d: f,
                     material: p.appliance === 'oven' ? 'Appliance' : 'Front' }));
      if (bar > 0.05) {
        var hy = p.front === 'drawers' ? y + h - 0.06 : y + h / 2;
        out.push(box({ x: -bar / 2, y: hy - 0.008, z: p.depth, w: bar, h: 0.016, d: 0.016,
                       material: 'Handle' }));
      }
      y += h;
    }
    var iw = p.width - 0.10, idp = p.depth - 0.14;
    if (p.appliance === 'hob' && iw > 0.05) {
      out.push(box({ x: -iw / 2, y: surface - 0.002, z: 0.06, w: iw, h: 0.008, d: idp,
                     material: 'Appliance' }));
    }
    if (p.appliance === 'sink' && iw > 0.05) {
      // A steel rim round the bowl, and the bowl seen through it: a sink from above.
      var rim = 0.03;
      out.push(box({ x: -iw / 2,       y: surface, z: 0.06,             w: iw,  h: 0.004, d: rim, material: 'Handle' }));
      out.push(box({ x: -iw / 2,       y: surface, z: 0.06 + idp - rim, w: iw,  h: 0.004, d: rim, material: 'Handle' }));
      out.push(box({ x: -iw / 2,       y: surface, z: 0.06,             w: rim, h: 0.004, d: idp, material: 'Handle' }));
      out.push(box({ x: iw / 2 - rim,  y: surface, z: 0.06,             w: rim, h: 0.004, d: idp, material: 'Handle' }));
      out.push(box({ x: -iw / 2 + rim, y: surface + 0.001, z: 0.06 + rim,
                     w: iw - 2 * rim, h: 0.002, d: idp - 2 * rim, material: 'Steel' }));
      out.push(box({ x: -0.012, y: surface,        z: 0.015, w: 0.024, h: 0.30, d: 0.024, material: 'Handle' }));
      out.push(box({ x: -0.012, y: surface + 0.28, z: 0.015, w: 0.024, h: 0.02, d: 0.18,  material: 'Handle' }));
    }
    return out;
  }
});
