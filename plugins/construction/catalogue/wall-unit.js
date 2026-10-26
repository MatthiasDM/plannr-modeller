// @name Wall cupboard
// @group Kitchen
component('wall-unit', {
  name: 'Wall cupboard', wants: 0,
  params: [
    { name: 'width',  label: 'Width',  kind: 'length', value: 0.60, min: 0.15, max: 1.20 },
    { name: 'depth',  label: 'Depth',  kind: 'length', value: 0.37, min: 0.20, max: 0.45 },
    { name: 'height', label: 'Height', kind: 'length', value: 0.72, min: 0.30, max: 1.20 },
    { name: 'lift',   label: 'Underside off the floor', kind: 'length', value: 1.40, min: 1.20, max: 1.90 },
    { name: 'front',  label: 'Front',  kind: 'choice', value: 'door', choices: ['door', 'glass', 'open'] }
  ],
  build: function (p) {
    var out = [], r = 0.003, f = 0.019, half = p.width / 2, t = 0.018;
    if (p.front === 'open') {
      // The same box with nothing on the front: sides, top, bottom, a shelf, a back.
      out.push(box({ x: -half,     y: p.lift, z: 0, w: t, h: p.height, d: p.depth }));
      out.push(box({ x: half - t,  y: p.lift, z: 0, w: t, h: p.height, d: p.depth }));
      out.push(box({ x: -half + t, y: p.lift, z: 0, w: p.width - 2 * t, h: t, d: p.depth }));
      out.push(box({ x: -half + t, y: p.lift + p.height - t, z: 0, w: p.width - 2 * t, h: t, d: p.depth }));
      out.push(box({ x: -half + t, y: p.lift + p.height / 2, z: 0, w: p.width - 2 * t, h: t, d: p.depth }));
      out.push(box({ x: -half + t, y: p.lift, z: 0, w: p.width - 2 * t, h: p.height, d: 0.008 }));
      return out;
    }
    out.push(box({ x: -half + r, y: p.lift, z: 0, w: p.width - 2 * r, h: p.height, d: p.depth - f }));
    var fx = -half + r, fy = p.lift + r, fw = p.width - 2 * r, fh = p.height - 2 * r, fz = p.depth - f;
    if (p.front === 'glass') {
      // A display cupboard: a frame of the front's own board round a pane, and a
      // shelf behind the glass.
      var rail = Math.min(0.05, fw / 4, fh / 4);
      out.push(box({ x: fx, y: fy, z: fz, w: rail, h: fh, d: f, material: 'Front' }));
      out.push(box({ x: fx + fw - rail, y: fy, z: fz, w: rail, h: fh, d: f, material: 'Front' }));
      out.push(box({ x: fx + rail, y: fy, z: fz, w: fw - 2 * rail, h: rail, d: f, material: 'Front' }));
      out.push(box({ x: fx + rail, y: fy + fh - rail, z: fz, w: fw - 2 * rail, h: rail, d: f, material: 'Front' }));
      out.push(box({ x: fx + rail, y: p.lift + p.height / 2 - 0.009, z: fz - 0.003,
                     w: fw - 2 * rail, h: 0.018, d: 0.003, material: 'Front' }));
      out.push(box({ x: fx + rail, y: fy + rail, z: fz + f / 2, w: fw - 2 * rail, h: fh - 2 * rail,
                     d: 0.004, material: 'Glass' }));
    } else {
      out.push(box({ x: fx, y: fy, z: fz, w: fw, h: fh, d: f, material: 'Front' }));
    }
    var bar = Math.min(p.width - 0.12, 0.32);
    if (bar > 0.05) {
      // Near the bottom, where a hand reaching up finds it.
      out.push(box({ x: -bar / 2, y: p.lift + 0.05, z: p.depth, w: bar, h: 0.016, d: 0.016,
                     material: 'Handle' }));
    }
    return out;
  }
});
