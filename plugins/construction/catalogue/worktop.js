// @name Worktop
// @group Kitchen
component('worktop', {
  name: 'Worktop', wants: 0,
  params: [
    { name: 'width',     label: 'Length',    kind: 'length', value: 1.20, min: 0.30, max: 4.20 },
    { name: 'depth',     label: 'Depth',     kind: 'length', value: 0.62, min: 0.30, max: 1.20 },
    { name: 'at',        label: 'Underside off the floor', kind: 'length', value: 0.86, min: 0.40, max: 1.20 },
    { name: 'thickness', label: 'Thickness', kind: 'length', value: 0.04, min: 0.02, max: 0.10 }
  ],
  build: function (p) {
    return [ box({ x: -p.width / 2, y: p.at, z: 0, w: p.width, h: p.thickness, d: p.depth, material: 'Worktop' }) ];
  }
});
