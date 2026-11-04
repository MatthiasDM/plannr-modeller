//! Connectivity: pipe ends, fitting ports and plumbing fixtures in world coordinates, and what a
//! pipe end is attached to. Geometry decides (an end within `TOL` of a port is connected).

use plannr_core::geom::{add, dist, dot, len, scale, sub, P3};
use plannr_core::model::Document;

use crate::data::*;
use crate::fittings;

/// A pipe end counts as connected to a port or another pipe end within this distance (m).
pub const TOL: f64 = 0.015;
/// … and to a construction fixture within this distance (its marker centre is not the exact connection point).
pub const FIXTURE_TOL: f64 = 0.3;

#[derive(Debug, Clone)]
pub struct PipeEnd { pub pipe: String, pub end: End, pub pos: P3, pub dir: P3, pub dn: f64, pub system: String, pub level: usize }
#[derive(Debug, Clone)]
pub struct PortW { pub fitting: String, pub index: usize, pub pos: P3, pub dir: P3, pub dn: f64, pub name: &'static str, pub tapware: bool, pub kind: String, pub system: String, pub joint: String }

#[derive(Debug, Clone)]
pub enum Attach {
    Port(PortW),
    PipeEnd(PipeEnd),
    /// On the inside of another pipe's segment: (pipe id, segment index, point on the axis, unit segment direction).
    PipeBody(String, usize, P3, P3),
    Fixture(Fx),
    Open,
}

pub struct Net { pub h: House, pub ends: Vec<PipeEnd>, pub ports: Vec<PortW>, pub pipes: Vec<(String, Pipe)>, pub fittings: Vec<(String, Fitting)> }

impl Net {
    pub fn read(doc: &Document) -> Net {
        let h = House::read(doc);
        let pipes = pipes(doc); let fits = fittings(doc);
        let mut ends = vec![];
        for (id, p) in &pipes {
            if p.points.len() < 2 { continue; }
            let y0 = h.base(p.level);
            for end in [End::Start, End::End] {
                let q = match end { End::Start => p.start(), End::End => p.end() };
                ends.push(PipeEnd { pipe: id.clone(), end, pos: add(q, [0.0, y0, 0.0]), dir: p.end_dir(end), dn: p.dn, system: p.system.clone(), level: p.level });
            }
        }
        let mut ports = vec![];
        for (id, f) in &fits {
            let y0 = h.base(f.level);
            for (i, pt) in fittings::ports(f).into_iter().enumerate() {
                ports.push(PortW { fitting: id.clone(), index: i, pos: add(pt.pos, [0.0, y0, 0.0]), dir: pt.dir, dn: pt.dn, name: pt.name, tapware: f.kind_info().tapware, kind: f.kind.clone(), system: f.system.clone(), joint: f.joint.clone() });
            }
        }
        Net { h, ends, ports, pipes, fittings: fits }
    }
    pub fn world(&self, level: usize, p: P3) -> P3 { add(p, [0.0, self.h.base(level), 0.0]) }
    pub fn rel(&self, level: usize, p: P3) -> P3 { sub(p, [0.0, self.h.base(level), 0.0]) }

    /// Nearest port to a world point within `tol`, optionally only of one fitting.
    pub fn port_near(&self, p: P3, tol: f64, only: Option<&str>) -> Option<&PortW> {
        self.ports.iter().filter(|q| only.map(|o| q.fitting == o).unwrap_or(true) && q.name != "thread").map(|q| (q, dist(q.pos, p))).filter(|(_, d)| *d <= tol).min_by(|a, b| a.1.partial_cmp(&b.1).unwrap()).map(|(q, _)| q)
    }
    pub fn end_near(&self, p: P3, tol: f64, skip_pipe: &str, only: Option<&str>) -> Option<&PipeEnd> {
        self.ends.iter().filter(|e| e.pipe != skip_pipe && only.map(|o| e.pipe == o).unwrap_or(true)).map(|e| (e, dist(e.pos, p))).filter(|(_, d)| *d <= tol).min_by(|a, b| a.1.partial_cmp(&b.1).unwrap()).map(|(e, _)| e)
    }
    /// Nearest point on the inside of another pipe's segments (not within 2 socket lengths of its ends).
    pub fn body_near(&self, p: P3, tol: f64, skip_pipe: &str, only: Option<&str>) -> Option<(String, usize, P3, P3)> {
        let mut best: Option<(f64, (String, usize, P3, P3))> = None;
        for (id, pipe) in &self.pipes {
            if id == skip_pipe || only.map(|o| o != id).unwrap_or(false) { continue; }
            let y0 = self.h.base(pipe.level);
            for (i, w) in pipe.points.windows(2).enumerate() {
                let a = add(w[0], [0.0, y0, 0.0]); let b = add(w[1], [0.0, y0, 0.0]);
                let d = sub(b, a); let l = len(d); if l < 1e-6 { continue; }
                let u = scale(d, 1.0 / l); let t = dot(sub(p, a), u);
                let margin = (pipe.dn / 1000.0 * 2.0).max(0.04);
                if t < margin || t > l - margin { continue; }
                let q = add(a, scale(u, t)); let dd = dist(q, p);
                if dd <= tol && best.as_ref().map(|b| dd < b.0).unwrap_or(true) { best = Some((dd, (id.clone(), i, q, u))); }
            }
        }
        best.map(|b| b.1)
    }
    pub fn fixture_near(&self, p: P3, tol: f64, only: Option<&str>) -> Option<&Fx> {
        self.h.fixtures.iter().filter(|f| only.map(|o| f.id == o).unwrap_or(true)).map(|f| (f, dist(self.world(f.level, f.position), p))).filter(|(_, d)| *d <= tol).min_by(|a, b| a.1.partial_cmp(&b.1).unwrap()).map(|(f, _)| f)
    }

    /// What a pipe end (world point) is attached to. `target` restricts the search to one entity id
    /// (a fitting, pipe or fixture) and widens the tolerance.
    pub fn attach_at(&self, pipe_id: &str, p: P3, target: Option<&str>) -> Attach {
        let tol = if target.is_some() { 0.6 } else { TOL };
        if let Some(q) = self.port_near(p, tol, target) { return Attach::Port(q.clone()); }
        if let Some(e) = self.end_near(p, if target.is_some() { tol } else { 0.05 }, pipe_id, target) { return Attach::PipeEnd(e.clone()); }
        if let Some((id, i, q, u)) = self.body_near(p, if target.is_some() { 0.6 } else { 0.03 }, pipe_id, target) { return Attach::PipeBody(id, i, q, u); }
        if let Some(f) = self.fixture_near(p, if target.is_some() { 1.5 } else { FIXTURE_TOL }, target) { return Attach::Fixture(f.clone()); }
        Attach::Open
    }

    /// Is a pipe end connected to anything (port, pipe end, pipe body, fixture)? An end whose pipe
    /// names a construction fixture (`from` / `to`) counts as connected to it while it stays within
    /// 1.2 m — a boiler's or radiator's connection is not at the fixture's marker centre.
    pub fn end_attachment(&self, e: &PipeEnd) -> Attach {
        let a = self.attach_at(&e.pipe, e.pos, None);
        if !matches!(a, Attach::Open) { return a; }
        let link = self.pipe(&e.pipe).and_then(|p| match e.end { End::Start => p.from.clone(), End::End => p.to.clone() });
        if let Some(id) = link { if let Some(fx) = self.fixture_near(e.pos, 1.5, Some(&id)) { return Attach::Fixture(fx.clone()); } }
        a
    }
    /// Pipe ends sitting on a port.
    pub fn ends_on_port(&self, port: &PortW) -> Vec<&PipeEnd> { self.ends.iter().filter(|e| dist(e.pos, port.pos) <= TOL).collect() }
    pub fn pipe(&self, id: &str) -> Option<&Pipe> { self.pipes.iter().find(|p| p.0 == id).map(|p| &p.1) }
}
