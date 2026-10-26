// @name Bath
// @group Bathroom
component('bath', {
  name: 'Bath', symbol: 'bath', needs: ['water', 'waste'], wants: 0.70,
  params: [
    { name: 'width',  label: 'Width',  kind: 'length', value: 1.70, min: 1.40, max: 1.90 },
    { name: 'depth',  label: 'Depth',  kind: 'length', value: 0.70, min: 0.60, max: 0.90 },
    { name: 'height', label: 'Height', kind: 'length', value: 0.55, min: 0.40, max: 0.65 },
    { name: 'rim',    label: 'Rim',    kind: 'length', value: 0.06, min: 0.03, max: 0.15 }
  ],
  build: function (p) {
    // A shell with a hollow in it: four sides round a recess, and its floor.
    var h = p.width / 2, r = p.rim, s = 'Sanitary';
    return [
      box({ x: -h,         y: 0, z: 0,           w: r,               h: p.height, d: p.depth, material: s }),
      box({ x: h - r,      y: 0, z: 0,           w: r,               h: p.height, d: p.depth, material: s }),
      box({ x: -h + r,     y: 0, z: 0,           w: p.width - 2 * r, h: p.height, d: r,       material: s }),
      box({ x: -h + r,     y: 0, z: p.depth - r, w: p.width - 2 * r, h: p.height, d: r,       material: s }),
      box({ x: -h + r,     y: 0, z: r,           w: p.width - 2 * r, h: 0.10,     d: p.depth - 2 * r, material: s })
    ];
  }
});
