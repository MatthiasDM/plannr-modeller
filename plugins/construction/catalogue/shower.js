// @name Shower
// @group Bathroom
component('shower', {
  name: 'Shower', symbol: 'shower', needs: ['water', 'waste'], wants: 0.70,
  params: [
    { name: 'width',  label: 'Width',  kind: 'length', value: 0.90, min: 0.70, max: 1.60 },
    { name: 'depth',  label: 'Depth',  kind: 'length', value: 0.90, min: 0.70, max: 1.40 },
    { name: 'screen', label: 'Screen, seen from in front', kind: 'choice', value: 'right',
      choices: ['right', 'left', 'none'] },
    { name: 'screenHeight', label: 'Screen height', kind: 'length', value: 2.00, min: 1.60, max: 2.40 }
  ],
  build: function (p) {
    var h = p.width / 2, tray = 0.06, t = 0.008;
    var out = [ box({ x: -h, y: 0, z: 0, w: p.width, h: tray, d: p.depth, material: 'Sanitary' }) ];
    if (p.screen !== 'none') {
      // Local x runs to the left of someone facing it, so their right is -x.
      var x = p.screen === 'left' ? h - t : -h;
      out.push(box({ x: x, y: tray, z: 0, w: t, h: p.screenHeight - tray, d: p.depth, material: 'Glass' }));
      // The black edge a walk-in screen has, along its top and its open end.
      out.push(box({ x: x - 0.004, y: p.screenHeight - 0.02, z: 0,
                     w: t + 0.008, h: 0.02, d: p.depth, material: 'Frame' }));
      out.push(box({ x: x - 0.004, y: tray, z: p.depth - 0.02,
                     w: t + 0.008, h: p.screenHeight - tray, d: 0.02, material: 'Frame' }));
    }
    return out;
  }
});
