// @name Radiator
// @group Components
component('radiator', {
  name: 'Radiator',
  params: [
    { name: 'length', label: 'Length', kind: 'length', value: 1.00, min: 0.30, max: 3.00 },
    { name: 'height', label: 'Height', kind: 'length', value: 0.60, min: 0.20, max: 2.00 },
    { name: 'panels', label: 'Panels', kind: 'choice', value: 2, choices: ['1', '2', '3'] },
    { name: 'lift',   label: 'Off the floor', kind: 'length', value: 0.12, min: 0.05, max: 0.60 },
    { name: 'fins',   label: 'Fins', kind: 'derived' },
    { name: 'watts',  label: 'Output (W)', kind: 'derived' }
  ],
  rules: function (p) {
    var panels = parseInt(p.panels, 10) || 2;
    p.fins = Math.max(2, Math.round(p.length / 0.05));
    // Roughly what a panel radiator of this size gives out, which is the sort of
    // thing you want a component to tell you rather than look up.
    p.watts = Math.round(p.length * p.height * panels * 1100);
    return p;
  },
  build: function (p) {
    var panels = parseInt(p.panels, 10) || 2;
    var t = 0.035, gap = 0.022, out = [];
    for (var i = 0; i < panels; i++) {
      out.push(box({ x: -p.length / 2, y: p.lift, z: i * (t + gap),
                     w: p.length, h: p.height, d: t, material: 'Radiator' }));
    }
    return out;
  }
});
