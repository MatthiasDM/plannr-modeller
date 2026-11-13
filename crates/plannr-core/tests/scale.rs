//! One engine for a house and for a printed part. The document is in metres: a part of a few millimetres has
//! walls, holes and letters well under one, where a tolerance meant for a house would eat the detail; a house
//! has corners picked on screen 20 m from the origin, good to a micron, where a tolerance meant for a part
//! would stop things sticking.

use std::collections::HashMap;

use plannr_core::geom::*;
use plannr_core::model::{Document, Entity, Face};
use plannr_core::{apply, Op, Registry};

const MM: f64 = 1e-3;
const UP: P3 = [0.0, 1.0, 0.0];

fn reg() -> Registry { Registry::new() }
fn run(doc: &mut Document, op: Op) -> plannr_core::OpResult { apply(doc, &op, &reg()).unwrap() }
fn rect(doc: &mut Document, a: P3, b: P3) -> plannr_core::OpResult {
    run(doc, Op::DrawRectangle { corner: a, opposite: b, normal: UP, u: None, layer: None, material: None, group: None })
}
fn circle(doc: &mut Document, center: P3, radius: f64, segments: usize) -> plannr_core::OpResult {
    run(doc, Op::DrawCircle { center, normal: UP, radius, segments: Some(segments), layer: None, material: None, group: None })
}
fn pull(doc: &mut Document, face: &str, distance: f64) -> plannr_core::OpResult {
    run(doc, Op::PushPull { face: face.to_string(), distance, new_face: false })
}
fn solid_box(doc: &mut Document, min: P3, max: P3) -> String {
    run(doc, Op::SolidBox { min, max, name: None, layer: None, material: None }).created[0].clone()
}
fn boolean(doc: &mut Document, mode: &str, target: &str, tools: &[String]) {
    run(doc, Op::Boolean { mode: mode.into(), target: target.into(), tools: tools.to_vec(), keep_tools: false, material: None });
}
fn text(doc: &mut Document, text: &str, position: P3, height: f64, depth: f64) -> String {
    run(doc, Op::Text3d { text: text.into(), position, height: Some(height), depth: Some(depth), font: None, bold: false, normal: Some(UP), u: Some([1.0, 0.0, 0.0]), align: None, layer: None, material: None, group: None }).created[0].clone()
}

fn faces(list: &[Entity]) -> Vec<&Face> { list.iter().filter_map(|e| if let Entity::Face(f) = e { Some(f) } else { None }).collect() }
fn group<'a>(doc: &'a Document, id: &str) -> &'a [Entity] { match doc.find(id).unwrap() { Entity::Group(g) => &g.entities, _ => panic!("not a group") } }
fn net(f: &Face) -> f64 { area(&f.outer) - f.holes.iter().map(|h| area(h)).sum::<f64>() }
fn flat(f: &Face, y: f64) -> bool { f.outer.iter().all(|p| (p[1] - y).abs() < 1e-9) }

/// The faces as a triangle mesh with corners closer than a micron welded: its volume, and how many edges are not
/// shared by exactly two triangles running opposite ways once every corner lying on another triangle's edge has
/// been made a corner of that triangle too (what a slicer's repair does). Zero for a closed solid.
fn shell(list: &[Entity]) -> (f64, usize) {
    let mut ids: HashMap<[i64; 3], usize> = HashMap::new(); let mut v: Vec<P3> = vec![]; let mut tris: Vec<[usize; 3]> = vec![];
    let mut vid = |p: P3, v: &mut Vec<P3>| { let k = [(p[0] * 1e6).round() as i64, (p[1] * 1e6).round() as i64, (p[2] * 1e6).round() as i64]; *ids.entry(k).or_insert_with(|| { v.push(p); v.len() - 1 }) };
    for f in faces(list) {
        let pts: Vec<P3> = f.outer.iter().chain(f.holes.iter().flatten()).copied().collect();
        for t in tessellate(&f.outer, &f.holes).chunks(3) {
            let t = [vid(pts[t[0] as usize], &mut v), vid(pts[t[1] as usize], &mut v), vid(pts[t[2] as usize], &mut v)];
            if t[0] != t[1] && t[1] != t[2] && t[0] != t[2] { tris.push(t); }
        }
    }
    let volume: f64 = tris.iter().map(|t| dot(v[t[0]], cross(v[t[1]], v[t[2]])) / 6.0).sum();
    // corners in the middle of an unmatched edge split that edge
    let count = |tris: &Vec<[usize; 3]>| {
        let mut edges: HashMap<(usize, usize), i32> = HashMap::new();
        for t in tris { for k in 0..3 { let (a, b) = (t[k], t[(k + 1) % 3]); if a < b { *edges.entry((a, b)).or_default() += 1; } else { *edges.entry((b, a)).or_default() -= 1; } } }
        edges.retain(|_, c| *c != 0);
        edges
    };
    for _ in 0..64 {
        let bad = count(&tris);
        if bad.is_empty() { break; }
        let loose: Vec<usize> = { let mut l: Vec<usize> = bad.keys().flat_map(|(a, b)| [*a, *b]).collect(); l.sort(); l.dedup(); l };
        let mut next: Vec<[usize; 3]> = vec![]; let mut any = false;
        for t in &tris {
            let hit = (0..3).find_map(|k| {
                let (ia, ib) = (t[k], t[(k + 1) % 3]);
                if !bad.contains_key(&(ia.min(ib), ia.max(ib))) { return None; }
                let (a, b) = (v[ia], v[ib]); let d = sub(b, a); let l2 = dot(d, d);
                loose.iter().copied().find(|i| !t.contains(i) && { let s = dot(sub(v[*i], a), d) / l2; s > 1e-9 && s < 1.0 - 1e-9 && dist(lerp(a, b, s), v[*i]) < 1e-6 }).map(|i| (k, i))
            });
            match hit {
                Some((k, i)) => { let (a, b, c) = (t[k], t[(k + 1) % 3], t[(k + 2) % 3]); next.push([a, i, c]); next.push([i, b, c]); any = true; }
                None => next.push(*t),
            }
        }
        tris = next;
        if !any { break; }
    }
    (volume, count(&tris).len())
}

#[test]
fn a_rectangle_on_a_plate_one_millimetre_thick_leaves_the_bottom_alone() {
    for thick in [1.0 * MM, 0.4 * MM, 0.2 * MM] {
        let mut doc = Document::default();
        let top = rect(&mut doc, [0.0; 3], [40.0 * MM, 0.0, 30.0 * MM]).created[0].clone();
        pull(&mut doc, &top, thick);
        rect(&mut doc, [10.0 * MM, thick, 10.0 * MM], [20.0 * MM, thick, 20.0 * MM]);
        let all = faces(&doc.entities);
        let bottom: Vec<&&Face> = all.iter().filter(|f| flat(f, 0.0)).collect();
        assert_eq!(bottom.len(), 1, "{thick}: one bottom face");
        assert!(bottom[0].holes.is_empty() && (net(bottom[0]) - 1200.0 * MM * MM).abs() < 1e-12, "{thick}: the bottom is whole");
        let tops: Vec<f64> = all.iter().filter(|f| flat(f, thick)).map(|f| net(f) / (MM * MM)).collect();
        assert_eq!(tops.len(), 2, "{thick}: the top is the square and the rest, {tops:?}");
        assert!((tops.iter().sum::<f64>() - 1200.0).abs() < 1e-6, "{thick}: {tops:?}");
    }
}

#[test]
fn a_small_circle_drawn_on_a_face_is_one_face() {
    for (r, segs) in [(0.5 * MM, 24), (0.5 * MM, 48), (0.3 * MM, 24), (0.2 * MM, 32), (1.5 * MM, 48)] {
        let mut doc = Document::default();
        let top = rect(&mut doc, [0.0; 3], [20.0 * MM, 0.0, 20.0 * MM]).created[0].clone();
        pull(&mut doc, &top, 3.0 * MM);
        let c = circle(&mut doc, [10.0 * MM, 3.0 * MM, 10.0 * MM], r, segs);
        let tops: Vec<&Face> = faces(&doc.entities).into_iter().filter(|f| flat(f, 3.0 * MM)).collect();
        assert_eq!(tops.len(), 2, "r {r} / {segs}: the disc and the face around it");
        let disc = tops.iter().find(|f| f.holes.is_empty()).expect("a disc");
        assert_eq!(disc.outer.len(), segs, "r {r} / {segs}: every side of the circle is kept");
        assert!(c.created.contains(&disc.id));
        let disc = disc.id.clone();
        // pushed through, it is a hole in a closed part
        pull(&mut doc, &disc, -3.0 * MM);
        let (vol, bad) = shell(&doc.entities);
        let want = 20.0 * 20.0 * 3.0 - 0.5 * segs as f64 * (r / MM).powi(2) * (std::f64::consts::TAU / segs as f64).sin() * 3.0;
        assert_eq!(bad, 0, "r {r} / {segs}: closed");
        assert!((vol / MM.powi(3) - want).abs() < 1e-6, "r {r} / {segs}: {} mm³, not {want}", vol / MM.powi(3));
    }
}

#[test]
fn curved_letters_join_and_cut_a_plate_cleanly_at_print_size() {
    for height in [3.0 * MM, 8.0 * MM, 20.0 * MM] {
        for (mode, y, depth) in [("union", 2.0 * MM, 1.0 * MM), ("subtract", 1.4 * MM, 1.0 * MM)] {
            let mut doc = Document::default();
            let plate = solid_box(&mut doc, [0.0; 3], [height * 6.0, 2.0 * MM, height * 2.0]);
            // (reading along x on a plane facing up, the letters rise towards -z)
            let t = text(&mut doc, "SHT45", [height * 0.5, y, height * 1.5], height, depth);
            let before = shell(group(&doc, &plate)).0;
            let letters = shell(group(&doc, &t));
            assert_eq!(letters.1, 0, "the letters themselves are closed");
            boolean(&mut doc, mode, &plate, &[t]);
            let (vol, bad) = shell(group(&doc, &plate));
            assert_eq!(bad, 0, "{mode} at {} mm: closed", height / MM);
            // raised letters add all of themselves; engraved ones take away the 0.6 mm that is inside the plate
            let want = if mode == "union" { before + letters.0 } else { before - letters.0 * 0.6 };
            assert!((vol - want).abs() < want * 1e-6, "{mode} at {} mm: {} mm³, not {}", height / MM, vol / MM.powi(3), want / MM.powi(3));
        }
    }
}

#[test]
fn details_under_a_tenth_of_a_millimetre_keep_their_corners() {
    // a rectangle 0.05 mm wide, and a line whose ends are 0.02 mm apart
    let mut doc = Document::default();
    let r = rect(&mut doc, [0.0; 3], [0.05 * MM, 0.0, 5.0 * MM]);
    assert_eq!(faces(&doc.entities).len(), 1);
    let f = faces(&doc.entities)[0];
    assert_eq!((r.created.len(), f.outer.len()), (1, 4));
    assert!((net(f) - 0.25 * MM * MM).abs() < 1e-15);
    let l = run(&mut doc, Op::DrawLine { points: vec![[1.0 * MM, 0.0, 0.0], [1.02 * MM, 0.0, 0.0]], close: false, layer: None, group: None });
    assert_eq!(l.created.len(), 1);
}

// ---- house size: what the looser tolerances were for still works --------------------------------------------

/// A point as the browser gives it back: its scene is in single precision.
fn picked(p: P3) -> P3 { p.map(|x| x as f32 as f64) }

#[test]
fn points_picked_in_single_precision_stick_at_house_size() {
    let mut doc = Document::default();
    let top = rect(&mut doc, [13.1, 0.0, 17.3], [19.7, 0.0, 23.9]).created[0].clone();
    pull(&mut doc, &top, 17.3);
    let whole = 6.6 * 6.6;
    // a rectangle on the top, both corners a little off its plane
    let (a, b) = (picked([14.123456789, 17.3, 18.987654321]), picked([15.2222222, 17.3, 20.1111111]));
    assert!((a[1] - 17.3).abs() > 1e-7, "the picked point is not on the plane to the last digit");
    rect(&mut doc, a, b);
    let tops = |doc: &Document| faces(&doc.entities).into_iter().filter(|f| f.outer.iter().all(|p| (p[1] - 17.3).abs() < 1e-5)).map(net).collect::<Vec<f64>>();
    let t = tops(&doc);
    assert_eq!(t.len(), 2, "the rectangle is cut into the top: {t:?}");
    assert!((t.iter().sum::<f64>() - whole).abs() < 1e-4, "{t:?}");
    // a line from one edge of the top to the other, its ends picked on the edges
    run(&mut doc, Op::DrawLine { points: vec![picked([17.0000001, 17.3, 17.3]), picked([17.0000001, 17.3, 23.9])], close: false, layer: None, group: None });
    let t = tops(&doc);
    assert_eq!(t.len(), 3, "the line divides the top: {t:?}");
    assert!((t.iter().sum::<f64>() - whole).abs() < 1e-4, "{t:?}");
    // loose lines whose ends were picked one from the other still close into a face
    let mut doc = Document::default();
    let p = [[13.3333333333, 0.0, 17.7777777777], [19.1111111111, 0.0, 17.7777777777], [19.1111111111, 0.0, 22.2222222222], [13.3333333333, 0.0, 22.2222222222]];
    for i in 0..4 {
        let (a, b) = (if i % 2 == 0 { p[i] } else { picked(p[i]) }, if i % 2 == 0 { picked(p[(i + 1) % 4]) } else { p[(i + 1) % 4] });
        run(&mut doc, Op::DrawLine { points: vec![a, b], close: false, layer: None, group: None });
    }
    assert_eq!(faces(&doc.entities).len(), 1, "four lines make a face");
}

#[test]
fn a_face_with_rounded_corners_still_takes_a_drawing() {
    // a roof plane given to the millimetre: its corners are half a millimetre out of flat
    let mut doc = Document::default();
    let c = [[0.0, 0.0, 0.0], [4.0, 0.0, 0.0], [4.0, 1.333, 2.001], [0.0, 1.334, 2.0]];
    let roof = run(&mut doc, Op::DrawPolygon { points: c.to_vec(), layer: None, material: None, group: None }).created[0].clone();
    let at = |u: f64, v: f64| lerp(lerp(c[0], c[1], u), lerp(c[3], c[2], u), v);
    run(&mut doc, Op::DrawPolygon { points: vec![at(0.25, 0.25), at(0.5, 0.25), at(0.5, 0.6), at(0.25, 0.6)], layer: None, material: None, group: None });
    let all = faces(&doc.entities);
    assert_eq!(all.len(), 2, "the roof light is cut into the roof");
    assert_eq!(all.iter().find(|f| f.id == roof).unwrap().holes.len(), 1);
}

#[test]
fn solids_placed_by_picked_points_join_cleanly() {
    // the second box starts at a corner of the first as the browser gave it back: a tenth of a micron inside it
    // or outside it, which at this size is the same place
    for x in [3.3f64 as f32 as f64, 3.3 + 1.2e-7] {
        let mut doc = Document::default();
        let a = solid_box(&mut doc, [0.0; 3], [3.3, 1.0, 1.0]);
        let b = solid_box(&mut doc, [x, 0.0, 0.0], [5.0, 1.0, 1.0]);
        assert!(x != 3.3);
        boolean(&mut doc, "union", &a, &[b]);
        let g = group(&doc, &a);
        let (vol, bad) = shell(g);
        assert_eq!(bad, 0, "{x}: closed");
        assert!((vol - 5.0).abs() < 1e-5, "{x}: {vol}");
        assert_eq!(g.len(), 6, "{x}: the wall between them is gone and the sides are one face each");
    }
}
