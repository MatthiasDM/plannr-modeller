// @name WC
// @group Bathroom
component('wc', {
  name: 'WC', symbol: 'wc', needs: ['water', 'waste'], wants: 0.60,
  params: [
    { name: 'width', label: 'Width', kind: 'length', value: 0.36, min: 0.34, max: 0.50 },
    { name: 'depth', label: 'Depth', kind: 'length', value: 0.54, min: 0.48, max: 0.80 },
    { name: 'hung',  label: 'Hung on the wall', kind: 'flag', value: 1 }
  ],
  build: function (p) {
    // A pan as it is: an egg-shaped bowl, wider at the back where it meets the
    // wall, the seat a ring on top and the lid up against the wall. Hung, the
    // cistern is in the wall behind a chrome flush plate; on the floor it has a
    // cistern on its back and a foot under the bowl.
    var s = 'Sanitary', w = p.width, d = p.depth, out = [];
    var tank = p.hung ? 0 : 0.18;            // depth of a visible cistern
    var back = tank, front = d;              // the bowl runs from the tank to the front
    var seatTop = 0.42, bowlTop = 0.38, bowlBottom = p.hung ? 0.15 : 0.02;
    // An egg in plan: a ring of corners, full width at the back third, tapering
    // to a rounded nose at the front.
    function egg(scale, n) {
      var corners = [], cz = back + (front - back) * 0.42, rz = (front - back) / 2;
      for (var i = 0; i < n; i++) {
        var t = i / n * Math.PI * 2;
        var x = Math.cos(t), z = Math.sin(t);
        var taper = z > 0 ? 1 - 0.28 * z : 1;   // narrower toward the nose
        corners.push({ x: x * (w / 2) * taper * scale, y: cz + z * rz * scale });
      }
      return corners;
    }
    out.push(shape(egg(0.78, 20), bowlBottom, bowlBottom + (bowlTop - bowlBottom) * 0.45, s));
    out.push(shape(egg(1.00, 20), bowlBottom + (bowlTop - bowlBottom) * 0.45, bowlTop, s));
    // The seat: a ring, made of convex slices between the outer and inner egg.
    var outer = egg(1.02, 20), inner = egg(0.72, 20);
    for (var i = 0; i < 20; i++) {
      var j = (i + 1) % 20;
      out.push(shape([outer[i], outer[j], inner[j], inner[i]], bowlTop, seatTop, s));
    }
    // The lid, up against the wall or the cistern.
    out.push(box({ x: -w / 2 * 0.98, y: seatTop, z: back + 0.02, w: w * 0.96, h: 0.36, d: 0.03, material: s }));
    if (p.hung) {
      out.push(box({ x: -0.125, y: 1.00, z: 0, w: 0.25, h: 0.16, d: 0.008, material: 'Handle' }));
    } else {
      out.push(box({ x: -w / 2, y: 0.40, z: 0, w: w, h: 0.40, d: tank, material: s }));
      out.push(box({ x: -w / 2 + 0.04, y: 0.80, z: 0.04, w: 0.06, h: 0.03, d: 0.06, material: 'Handle' }));
      out.push(shape(egg(0.55, 12), 0, 0.02, s));
      out.push(box({ x: -w * 0.22, y: 0, z: back, w: w * 0.44, h: bowlBottom + 0.01, d: front - back - 0.1, material: s }));
    }
    return out;
  }
});
