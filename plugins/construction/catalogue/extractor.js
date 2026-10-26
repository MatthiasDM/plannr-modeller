// @name Extractor
// @group Kitchen
component('extractor', {
  name: 'Extractor', needs: ['power', 'vent'], wants: 0,
  params: [
    { name: 'width',   label: 'Width',   kind: 'length', value: 0.60, min: 0.50, max: 1.20 },
    { name: 'depth',   label: 'Depth',   kind: 'length', value: 0.50, min: 0.30, max: 0.60 },
    { name: 'lift',    label: 'Underside off the floor', kind: 'length', value: 1.55, min: 1.40, max: 1.90 },
    { name: 'chimney', label: 'Chimney', kind: 'length', value: 0.70, min: 0.00, max: 1.20 }
  ],
  build: function (p) {
    var out = [ box({ x: -p.width / 2, y: p.lift, z: 0, w: p.width, h: 0.08, d: p.depth, material: 'Steel' }) ];
    if (p.chimney > 0.01) {
      out.push(box({ x: -0.14, y: p.lift + 0.08, z: 0, w: 0.28, h: p.chimney, d: 0.26, material: 'Steel' }));
    }
    return out;
  }
});
