// @name Island
// @group Components
component('island', {
  name: 'Island',
  params: [
    { name: 'length',    label: 'Length', kind: 'length', value: 1.80, min: 0.60, max: 4.00 },
    { name: 'depth',     label: 'Depth',  kind: 'length', value: 0.90, min: 0.60, max: 1.40 },
    { name: 'overhang',  label: 'Overhang', kind: 'length', value: 0.30, min: 0, max: 0.60 },
    { name: 'seats',     label: 'Seats', kind: 'derived' }
  ],
  rules: function (p) {
    // Sixty centimetres a person, and only if there is somewhere to put your knees.
    p.seats = p.overhang >= 0.25 ? Math.floor(p.length / 0.60) : 0;
    return p;
  },
  build: function (p) {
    var carcass = p.depth - p.overhang;
    return [
      box({ x: -p.length / 2 + 0.05, y: 0, z: 0.05,
            w: p.length - 0.10, h: 0.12, d: carcass - 0.10 }),
      box({ x: -p.length / 2, y: 0.12, z: 0, w: p.length, h: 0.72, d: carcass }),
      box({ x: -p.length / 2 - 0.02, y: 0.84, z: -0.02,
            w: p.length + 0.04, h: 0.04, d: p.depth + 0.04, material: 'Worktop' })
    ];
  }
});
