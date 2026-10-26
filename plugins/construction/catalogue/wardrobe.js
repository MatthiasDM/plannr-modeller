// @name Wardrobe
// @group Components
component('wardrobe', {
  name: 'Wardrobe',
  params: [
    { name: 'width',  label: 'Width',  kind: 'length', value: 2.00, min: 0.45, max: 6.00 },
    { name: 'height', label: 'Height', kind: 'length', value: 2.40, min: 1.20, max: 3.00 },
    { name: 'depth',  label: 'Depth',  kind: 'length', value: 0.60, min: 0.35, max: 0.90 },
    { name: 'doors',  label: 'Doors',  kind: 'derived' },
    { name: 'leaf',   label: 'Door width', kind: 'derived' }
  ],
  rules: function (p) {
    // Doors come in a width a person can open, so the count follows the run.
    p.doors = Math.max(1, Math.ceil(p.width / 0.60));
    p.leaf = p.width / p.doors;
    return p;
  },
  build: function (p) {
    var t = 0.018, reveal = 0.003, out = [];
    var half = p.width / 2;
    out.push(box({ x: -half, y: 0, z: 0, w: p.width, h: t, d: p.depth }));
    out.push(box({ x: -half, y: p.height - t, z: 0, w: p.width, h: t, d: p.depth }));
    for (var i = 0; i < p.doors; i++) {
      var x = -half + i * p.leaf;
      out.push(box({ x: x + reveal, y: t, z: p.depth - 0.019,
                     w: p.leaf - 2 * reveal, h: p.height - 2 * t, d: 0.019,
                     material: 'Front' }));
      // The handle, on the opening edge.
      out.push(box({ x: x + p.leaf - 0.06, y: p.height * 0.45, z: p.depth,
                     w: 0.016, h: 0.28, d: 0.016, material: 'Handle' }));
    }
    for (var j = 1; j < p.doors; j++) {
      out.push(box({ x: -half + j * p.leaf - t / 2, y: t, z: 0,
                     w: t, h: p.height - 2 * t, d: p.depth }));
    }
    return out;
  }
});
