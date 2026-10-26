// @name Tall unit
// @group Kitchen
component('tall-unit', {
  name: 'Tall unit', wants: 0.90,
  params: [
    { name: 'width',     label: 'Width',          kind: 'length', value: 0.60, min: 0.30, max: 0.90 },
    { name: 'depth',     label: 'Depth',          kind: 'length', value: 0.60, min: 0.30, max: 0.70 },
    { name: 'height',    label: 'Carcass height', kind: 'length', value: 2.00, min: 1.20, max: 2.40 },
    { name: 'plinth',    label: 'Plinth',         kind: 'length', value: 0.12, min: 0.00, max: 0.30 },
    { name: 'appliance', label: 'Appliance',      kind: 'choice', value: 'none',
      choices: ['none', 'oven', 'fridge'] }
  ],
  needs: function (p) { return p.appliance === 'oven' ? ['circuit'] : p.appliance === 'fridge' ? ['power'] : []; },
  symbol: function (p) { return p.appliance === 'none' ? null : p.appliance; },
  build: function (p) {
    var out = [], r = 0.003, f = 0.019, half = p.width / 2;
    var low = p.plinth, top = p.plinth + p.height;
    if (p.plinth > 0.01) out.push(box({ x: -half, y: 0, z: 0, w: p.width, h: p.plinth, d: p.depth - 0.05 }));
    out.push(box({ x: -half + r, y: low, z: 0, w: p.width - 2 * r, h: p.height, d: p.depth - f }));
    // An oven goes where you can see into it without bending, between two doors.
    var ovenTop = Math.min(1.45, top), ovenBottom = Math.min(0.85, ovenTop - 0.30);
    var leaves = p.appliance === 'oven'
      ? [{ from: low, to: ovenBottom, m: 'Front' }, { from: ovenBottom, to: ovenTop, m: 'Appliance' },
         { from: ovenTop, to: top, m: 'Front' }]
      : [{ from: low, to: top, m: 'Front' }];
    for (var i = 0; i < leaves.length; i++) {
      var l = leaves[i];
      if (l.to - l.from < 0.02) continue;
      out.push(box({ x: -half + r, y: l.from + r, z: p.depth - f, w: p.width - 2 * r,
                     h: l.to - l.from - 2 * r, d: f, material: l.m }));
      if (l.m === 'Appliance') {
        out.push(box({ x: -0.18, y: l.to - 0.07, z: p.depth, w: 0.36, h: 0.016, d: 0.016, material: 'Handle' }));
      } else {
        // A long bar on the opening edge of a tall door.
        var len = Math.min(0.40, (l.to - l.from) - 0.20);
        if (len > 0.05) {
          out.push(box({ x: half - 0.06, y: (l.from + l.to) / 2 - len / 2, z: p.depth,
                         w: 0.016, h: len, d: 0.016, material: 'Handle' }));
        }
      }
    }
    return out;
  }
});
