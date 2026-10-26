// @name Shelving
// @group Components
component('shelving', {
  name: 'Shelving',
  params: [
    { name: 'width',   label: 'Width',         kind: 'length',  value: 0.90, min: 0.30, max: 2.40 },
    { name: 'height',  label: 'Height',        kind: 'length',  value: 1.80, min: 0.40, max: 2.60 },
    { name: 'depth',   label: 'Depth',         kind: 'length',  value: 0.30, min: 0.15, max: 0.60 },
    { name: 'spacing', label: 'Shelf spacing', kind: 'length',  value: 0.34, min: 0.18, max: 0.60 },
    { name: 'shelves', label: 'Shelves',       kind: 'derived' }
  ],
  rules: function (p) {
    // The whole point: the gap stays what you asked for and the count follows.
    p.shelves = Math.max(1, Math.round((p.height - 0.018) / p.spacing));
    return p;
  },
  build: function (p) {
    var t = 0.018, out = [];
    var half = p.width / 2;
    out.push(box({ x: -half, y: 0, z: 0, w: t, h: p.height, d: p.depth }));
    out.push(box({ x: half - t, y: 0, z: 0, w: t, h: p.height, d: p.depth }));
    for (var i = 0; i <= p.shelves; i++) {
      var y = (p.height - t) * i / p.shelves;
      out.push(box({ x: -half + t, y: y, z: 0, w: p.width - 2 * t, h: t, d: p.depth }));
    }
    return out;
  }
});
