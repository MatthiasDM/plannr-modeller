//! Small geometry kernel: vectors, planes, polygons, transforms, tessellation.

pub type P3 = [f64; 3];
/// Column-major 4×4 matrix (three.js / glTF layout).
pub type Mat4 = [f64; 16];

pub const EPS: f64 = 1e-9;
/// Points within this distance of a plane are "on" it (1 mm).
pub const PLANE_TOL: f64 = 1e-3;
/// Two points closer than this are the same vertex.
pub const VERTEX_TOL: f64 = 1e-4;

pub fn add(a: P3, b: P3) -> P3 { [a[0] + b[0], a[1] + b[1], a[2] + b[2]] }
pub fn sub(a: P3, b: P3) -> P3 { [a[0] - b[0], a[1] - b[1], a[2] - b[2]] }
pub fn scale(a: P3, s: f64) -> P3 { [a[0] * s, a[1] * s, a[2] * s] }
pub fn dot(a: P3, b: P3) -> f64 { a[0] * b[0] + a[1] * b[1] + a[2] * b[2] }
pub fn cross(a: P3, b: P3) -> P3 { [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]] }
pub fn len(a: P3) -> f64 { dot(a, a).sqrt() }
pub fn dist(a: P3, b: P3) -> f64 { len(sub(a, b)) }
pub fn normalize(a: P3) -> P3 { let l = len(a); if l < EPS { [0.0, 0.0, 0.0] } else { scale(a, 1.0 / l) } }
pub fn same_point(a: P3, b: P3) -> bool { dist(a, b) < VERTEX_TOL }
pub fn lerp(a: P3, b: P3, t: f64) -> P3 { add(a, scale(sub(b, a), t)) }

/// Newell's method: area-weighted normal of a (possibly non-convex) planar polygon.
pub fn newell(poly: &[P3]) -> P3 {
    let mut n = [0.0; 3];
    for i in 0..poly.len() {
        let a = poly[i];
        let b = poly[(i + 1) % poly.len()];
        n[0] += (a[1] - b[1]) * (a[2] + b[2]);
        n[1] += (a[2] - b[2]) * (a[0] + b[0]);
        n[2] += (a[0] - b[0]) * (a[1] + b[1]);
    }
    n
}
pub fn normal(poly: &[P3]) -> P3 { normalize(newell(poly)) }
pub fn area(poly: &[P3]) -> f64 { len(newell(poly)) / 2.0 }

/// Max distance of any point from the plane through `origin` with `normal`.
pub fn plane_deviation(poly: &[P3], origin: P3, n: P3) -> f64 {
    poly.iter().map(|p| dot(sub(*p, origin), n).abs()).fold(0.0, f64::max)
}
pub fn is_planar(poly: &[P3]) -> bool {
    if poly.len() < 3 { return false; }
    let n = normal(poly);
    if len(n) < 0.5 { return false; }
    plane_deviation(poly, poly[0], n) <= PLANE_TOL
}

/// Orthonormal in-plane basis (u, v) for a normal.
pub fn plane_basis(n: P3) -> (P3, P3) {
    let n = normalize(n);
    let helper = if n[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 0.0, 1.0] };
    let u = normalize(cross(helper, n));
    let v = cross(n, u);
    (u, v)
}

/// Project a point onto the plane through `origin` with normal `n`.
pub fn project_to_plane(p: P3, origin: P3, n: P3) -> P3 {
    let n = normalize(n);
    sub(p, scale(n, dot(sub(p, origin), n)))
}

/// Offset a planar polygon in its plane; positive = outward (area grows).
pub fn offset_polygon(poly: &[P3], distance: f64) -> Vec<P3> {
    let n = normal(poly);
    let k = poly.len();
    let attempt = |sign: f64| -> Vec<P3> {
        // offset lines per edge
        let lines: Vec<(P3, P3)> = (0..k)
            .map(|i| {
                let a = poly[i];
                let b = poly[(i + 1) % k];
                let d = normalize(sub(b, a));
                let out = scale(normalize(cross(d, n)), sign * distance);
                (add(a, out), d)
            })
            .collect();
        (0..k)
            .map(|i| {
                let (p1, d1) = lines[(i + k - 1) % k];
                let (p2, d2) = lines[i];
                // intersect line p1 + t d1 with line p2 + s d2 (in-plane)
                let c = cross(d1, d2);
                let cl = len(c);
                if cl < 1e-6 {
                    // parallel: fall back to the vertex normal
                    let vn = normalize(add(cross(d1, n), cross(d2, n)));
                    add(poly[i], scale(vn, sign * distance))
                } else {
                    let w = sub(p2, p1);
                    let t = dot(cross(w, d2), c) / (cl * cl);
                    add(p1, scale(d1, t))
                }
            })
            .collect()
    };
    let a = attempt(1.0);
    if distance > 0.0 && area(&a) < area(poly) || distance < 0.0 && area(&a) > area(poly) { attempt(-1.0) } else { a }
}

/// Triangulate a planar polygon with holes; returns indices into the concatenated point list
/// (outer first, then holes in order).
pub fn tessellate(outer: &[P3], holes: &[Vec<P3>]) -> Vec<u32> {
    if outer.len() < 3 { return vec![]; }
    let n = normal(outer);
    let (u, v) = plane_basis(n);
    let o = outer[0];
    let mut flat: Vec<f64> = Vec::with_capacity((outer.len() + holes.iter().map(|h| h.len()).sum::<usize>()) * 2);
    let mut hole_idx = vec![];
    for p in outer { let d = sub(*p, o); flat.push(dot(d, u)); flat.push(dot(d, v)); }
    for h in holes {
        hole_idx.push(flat.len() / 2);
        for p in h { let d = sub(*p, o); flat.push(dot(d, u)); flat.push(dot(d, v)); }
    }
    match earcutr::earcut(&flat, &hole_idx, 2) {
        Ok(idx) => {
            // earcut returns triangles wound for the (u,v) basis; make them follow `n`
            let mut out: Vec<u32> = idx.into_iter().map(|i| i as u32).collect();
            let pts: Vec<P3> = outer.iter().chain(holes.iter().flatten()).copied().collect();
            if out.len() >= 3 {
                let t = [pts[out[0] as usize], pts[out[1] as usize], pts[out[2] as usize]];
                if dot(newell(&t), n) < 0.0 {
                    for tri in out.chunks_mut(3) { tri.swap(1, 2); }
                }
            }
            out
        }
        Err(_) => vec![],
    }
}

// ---- matrices -------------------------------------------------------------------

pub fn mat_identity() -> Mat4 { [1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.] }
pub fn mat_translation(d: P3) -> Mat4 { let mut m = mat_identity(); m[12] = d[0]; m[13] = d[1]; m[14] = d[2]; m }
pub fn mat_scale(s: P3) -> Mat4 { let mut m = mat_identity(); m[0] = s[0]; m[5] = s[1]; m[10] = s[2]; m }
/// Rotation about a unit axis through the origin (right-handed, angle in radians).
pub fn mat_rotation(axis: P3, angle: f64) -> Mat4 {
    let [x, y, z] = normalize(axis);
    let (s, c) = angle.sin_cos();
    let t = 1.0 - c;
    [
        t * x * x + c, t * x * y + s * z, t * x * z - s * y, 0.0,
        t * x * y - s * z, t * y * y + c, t * y * z + s * x, 0.0,
        t * x * z + s * y, t * y * z - s * x, t * z * z + c, 0.0,
        0.0, 0.0, 0.0, 1.0,
    ]
}
/// a * b (apply b first, then a).
pub fn mat_mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut m = [0.0; 16];
    for col in 0..4 {
        for row in 0..4 {
            m[col * 4 + row] = (0..4).map(|k| a[k * 4 + row] * b[col * 4 + k]).sum();
        }
    }
    m
}
pub fn mat_apply(m: &Mat4, p: P3) -> P3 {
    [
        m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12],
        m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13],
        m[2] * p[0] + m[6] * p[1] + m[10] * p[2] + m[14],
    ]
}
pub fn mat_apply_dir(m: &Mat4, d: P3) -> P3 {
    [
        m[0] * d[0] + m[4] * d[1] + m[8] * d[2],
        m[1] * d[0] + m[5] * d[1] + m[9] * d[2],
        m[2] * d[0] + m[6] * d[1] + m[10] * d[2],
    ]
}
/// Transform `about` an origin: T(o) · m · T(-o).
pub fn mat_about(m: &Mat4, origin: P3) -> Mat4 {
    mat_mul(&mat_translation(origin), &mat_mul(m, &mat_translation(scale(origin, -1.0))))
}
/// Determinant of the upper-left 3×3 (negative for a reflection).
pub fn mat_det3(m: &Mat4) -> f64 {
    m[0] * (m[5] * m[10] - m[9] * m[6]) - m[4] * (m[1] * m[10] - m[9] * m[2]) + m[8] * (m[1] * m[6] - m[5] * m[2])
}
/// Reflection across the plane through `origin` with unit normal `n`.
pub fn mat_reflection(origin: P3, n: P3) -> Mat4 {
    let [x, y, z] = normalize(n);
    let r: Mat4 = [
        1.0 - 2.0 * x * x, -2.0 * x * y, -2.0 * x * z, 0.0,
        -2.0 * x * y, 1.0 - 2.0 * y * y, -2.0 * y * z, 0.0,
        -2.0 * x * z, -2.0 * y * z, 1.0 - 2.0 * z * z, 0.0,
        0.0, 0.0, 0.0, 1.0,
    ];
    mat_about(&r, origin)
}
/// Circumcentre of three points (None when collinear).
pub fn circumcenter(a: P3, b: P3, c: P3) -> Option<P3> {
    let ab = sub(b, a);
    let ac = sub(c, a);
    let n = cross(ab, ac);
    let d = 2.0 * dot(n, n);
    if d < 1e-18 { return None; }
    let t = scale(sub(scale(cross(n, ab), dot(ac, ac)), scale(cross(n, ac), dot(ab, ab))), 1.0 / d);
    Some(add(a, t))
}
pub fn mat_is_identity(m: &Mat4) -> bool {
    m.iter().zip(mat_identity().iter()).all(|(a, b)| (a - b).abs() < 1e-12)
}
pub fn mat_inverse(m: &Mat4) -> Mat4 {
    let a = m;
    let mut inv = [0.0; 16];
    inv[0] = a[5] * a[10] * a[15] - a[5] * a[11] * a[14] - a[9] * a[6] * a[15] + a[9] * a[7] * a[14] + a[13] * a[6] * a[11] - a[13] * a[7] * a[10];
    inv[4] = -a[4] * a[10] * a[15] + a[4] * a[11] * a[14] + a[8] * a[6] * a[15] - a[8] * a[7] * a[14] - a[12] * a[6] * a[11] + a[12] * a[7] * a[10];
    inv[8] = a[4] * a[9] * a[15] - a[4] * a[11] * a[13] - a[8] * a[5] * a[15] + a[8] * a[7] * a[13] + a[12] * a[5] * a[11] - a[12] * a[7] * a[9];
    inv[12] = -a[4] * a[9] * a[14] + a[4] * a[10] * a[13] + a[8] * a[5] * a[14] - a[8] * a[6] * a[13] - a[12] * a[5] * a[10] + a[12] * a[6] * a[9];
    inv[1] = -a[1] * a[10] * a[15] + a[1] * a[11] * a[14] + a[9] * a[2] * a[15] - a[9] * a[3] * a[14] - a[13] * a[2] * a[11] + a[13] * a[3] * a[10];
    inv[5] = a[0] * a[10] * a[15] - a[0] * a[11] * a[14] - a[8] * a[2] * a[15] + a[8] * a[3] * a[14] + a[12] * a[2] * a[11] - a[12] * a[3] * a[10];
    inv[9] = -a[0] * a[9] * a[15] + a[0] * a[11] * a[13] + a[8] * a[1] * a[15] - a[8] * a[3] * a[13] - a[12] * a[1] * a[11] + a[12] * a[3] * a[9];
    inv[13] = a[0] * a[9] * a[14] - a[0] * a[10] * a[13] - a[8] * a[1] * a[14] + a[8] * a[2] * a[13] + a[12] * a[1] * a[10] - a[12] * a[2] * a[9];
    inv[2] = a[1] * a[6] * a[15] - a[1] * a[7] * a[14] - a[5] * a[2] * a[15] + a[5] * a[3] * a[14] + a[13] * a[2] * a[7] - a[13] * a[3] * a[6];
    inv[6] = -a[0] * a[6] * a[15] + a[0] * a[7] * a[14] + a[4] * a[2] * a[15] - a[4] * a[3] * a[14] - a[12] * a[2] * a[7] + a[12] * a[3] * a[6];
    inv[10] = a[0] * a[5] * a[15] - a[0] * a[7] * a[13] - a[4] * a[1] * a[15] + a[4] * a[3] * a[13] + a[12] * a[1] * a[7] - a[12] * a[3] * a[5];
    inv[14] = -a[0] * a[5] * a[14] + a[0] * a[6] * a[13] + a[4] * a[1] * a[14] - a[4] * a[2] * a[13] - a[12] * a[1] * a[6] + a[12] * a[2] * a[5];
    inv[3] = -a[1] * a[6] * a[11] + a[1] * a[7] * a[10] + a[5] * a[2] * a[11] - a[5] * a[3] * a[10] - a[9] * a[2] * a[7] + a[9] * a[3] * a[6];
    inv[7] = a[0] * a[6] * a[11] - a[0] * a[7] * a[10] - a[4] * a[2] * a[11] + a[4] * a[3] * a[10] + a[8] * a[2] * a[7] - a[8] * a[3] * a[6];
    inv[11] = -a[0] * a[5] * a[11] + a[0] * a[7] * a[9] + a[4] * a[1] * a[11] - a[4] * a[3] * a[9] - a[8] * a[1] * a[7] + a[8] * a[3] * a[5];
    inv[15] = a[0] * a[5] * a[10] - a[0] * a[6] * a[9] - a[4] * a[1] * a[10] + a[4] * a[2] * a[9] + a[8] * a[1] * a[6] - a[8] * a[2] * a[5];
    let det = a[0] * inv[0] + a[1] * inv[4] + a[2] * inv[8] + a[3] * inv[12];
    if det.abs() < 1e-18 { return mat_identity(); }
    for v in inv.iter_mut() { *v /= det; }
    inv
}

/// Clip a convex polygon against the half-space `dot(p - origin, n) <= 0` (Sutherland–Hodgman).
pub fn clip_polygon(poly: &[P3], origin: P3, n: P3) -> Vec<P3> {
    let mut out = vec![];
    let k = poly.len();
    for i in 0..k {
        let a = poly[i];
        let b = poly[(i + 1) % k];
        let da = dot(sub(a, origin), n);
        let db = dot(sub(b, origin), n);
        if da <= 0.0 { out.push(a); }
        if (da < 0.0 && db > 0.0) || (da > 0.0 && db < 0.0) {
            let t = da / (da - db);
            out.push(lerp(a, b, t));
        }
    }
    out
}
/// Clip a segment to the kept half-space; None when fully cut away.
pub fn clip_segment(a: P3, b: P3, origin: P3, n: P3) -> Option<(P3, P3)> {
    let da = dot(sub(a, origin), n);
    let db = dot(sub(b, origin), n);
    if da <= 0.0 && db <= 0.0 { return Some((a, b)); }
    if da > 0.0 && db > 0.0 { return None; }
    let t = da / (da - db);
    let m = lerp(a, b, t);
    if da <= 0.0 { Some((a, m)) } else { Some((m, b)) }
}

pub fn hex_to_rgb(hex: &str) -> [f32; 3] {
    let h = hex.trim_start_matches('#');
    let c = |i: usize| u8::from_str_radix(h.get(i..i + 2).unwrap_or("80"), 16).unwrap_or(128) as f32 / 255.0;
    if h.len() == 3 {
        let c3 = |i: usize| u8::from_str_radix(&h[i..i + 1].repeat(2), 16).unwrap_or(128) as f32 / 255.0;
        return [c3(0), c3(1), c3(2)];
    }
    [c(0), c(2), c(4)]
}
pub fn is_hex_color(s: &str) -> bool {
    let h = s.strip_prefix('#');
    matches!(h, Some(h) if (h.len() == 6 || h.len() == 3) && h.chars().all(|c| c.is_ascii_hexdigit()))
}
