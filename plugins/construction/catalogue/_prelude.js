// Studio component prelude (verbatim): component(), box(), shape() and the __ helpers.
var __types = {};

function component(id, definition) { __types[id] = definition; }

/// A rectangular lump. Everything is one of these; a script that wants a curve makes
/// several. It says what shapes it wants, never what vertices.
function box(b) {
  var x = b.x || 0, y = b.y || 0, z = b.z || 0;
  var w = b.w || 0, h = b.h || 0, d = b.d || 0;
  return {
    plan: [{x: x, y: z}, {x: x + w, y: z}, {x: x + w, y: z + d}, {x: x, y: z + d}],
    low: y, high: y + h, material: b.material || 'Carcass'
  };
}

/// Somewhere between two heights, following corners you give yourself — any convex
/// outline, so a ring of them is a round top.
function shape(corners, low, high, material) {
  return { plan: corners, low: low, high: high, material: material || 'Carcass' };
}

/// Anything a type declares about itself may be a value or a function of its options.
function __meta(id, given) {
  var type = __types[id];
  if (!type) return null;
  var values = __resolve(id, given).values;
  function ask(v) { return typeof v === 'function' ? v(values) : v; }
  var wants = ask(type.wants);
  return { name: type.name || id, needs: ask(type.needs) || [],
           wants: typeof wants === 'number' ? wants : 0, symbol: ask(type.symbol) || null };
}

function __worktop(id, given) {
  var type = __types[id];
  if (!type || !type.worktop) return null;
  var values = __resolve(id, given).values;
  var top = typeof type.worktop === 'function' ? type.worktop(values) : type.worktop;
  return top ? { at: top.at, depth: top.depth } : null;
}

function __resolve(id, given) {
  var type = __types[id];
  if (!type) throw new Error('no component type called ' + id);
  var params = (type.params || []).slice();
  var values = {};
  params.forEach(function (p) {
    values[p.name] = (given && given[p.name] !== undefined && given[p.name] !== null)
      ? given[p.name] : (p.value !== undefined ? p.value : 0);
  });
  if (type.rules) {
    var after = type.rules(values);
    if (after) values = after;
  }
  return { type: type, params: params, values: values };
}

function __params(id, given) {
  var r = __resolve(id, given);
  var out = [];
  for (var i = 0; i < r.params.length; i++) {
    var p = r.params[i];
    out.push({
      name: p.name, label: p.label || p.name, kind: p.kind || 'number',
      value: typeof r.values[p.name] === 'number' ? r.values[p.name] : 0,
      text: typeof r.values[p.name] === 'string' ? r.values[p.name] : undefined,
      min: p.min, max: p.max, choices: p.choices || []
    });
  }
  return out;
}

function __solids(id, given) {
  var r = __resolve(id, given);
  if (!r.type.build) throw new Error(id + ' has nothing to build');
  var made = r.type.build(r.values) || [];
  return made.filter(function (s) { return s && s.plan && s.plan.length >= 3; });
}
