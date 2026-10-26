// @name Basin
// @group Bathroom
component('basin', {
  name: 'Basin', symbol: 'basin', needs: ['water', 'waste'], wants: 0.55,
  params: [
    { name: 'width', label: 'Width', kind: 'length', value: 0.55, min: 0.35, max: 0.90 },
    { name: 'depth', label: 'Depth', kind: 'length', value: 0.48, min: 0.30, max: 0.60 },
    { name: 'lift',  label: 'Underside off the floor', kind: 'length', value: 0.72, min: 0.60, max: 0.85 },
    { name: 'rim',   label: 'Rim height (m)', kind: 'derived' }
  ],
  rules: function (p) { p.rim = Math.round((p.lift + 0.18) * 100) / 100; return p; },
  build: function (p) {
    var h = p.width / 2, r = 0.05, s = 'Sanitary', top = p.lift + 0.18;
    return [
      box({ x: -h,     y: p.lift, z: 0,           w: r,               h: 0.18, d: p.depth, material: s }),
      box({ x: h - r,  y: p.lift, z: 0,           w: r,               h: 0.18, d: p.depth, material: s }),
      box({ x: -h + r, y: p.lift, z: 0,           w: p.width - 2 * r, h: 0.18, d: r,       material: s }),
      box({ x: -h + r, y: p.lift, z: p.depth - r, w: p.width - 2 * r, h: 0.18, d: r,       material: s }),
      box({ x: -h + r, y: p.lift + 0.05, z: r,    w: p.width - 2 * r, h: 0.04, d: p.depth - 2 * r, material: s }),
      box({ x: -0.012, y: top,        z: 0.015, w: 0.024, h: 0.16, d: 0.024, material: 'Handle' }),
      box({ x: -0.012, y: top + 0.14, z: 0.015, w: 0.024, h: 0.02, d: 0.10,  material: 'Handle' })
    ];
  }
});
