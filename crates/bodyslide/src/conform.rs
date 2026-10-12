//! Conforming: giving an outfit's shape what a body's sliders do, so that
//! it follows the body as the sliders move. Each vertex of the shape takes
//! what the body's vertices nearest to it do, the nearer the more.
//!
//! The way it is done here is this project's own. How close it comes to
//! the slider data the original program made for real outfits is measured
//! by `examples/conformfit.rs`.

use crate::osd::Diff;

/// How a shape's vertices are tied to a body's.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Options {
    /// How many of the body's vertices each vertex follows, at most.
    pub nearest: usize,
    /// A body's vertex further off than this is not followed. Nothing
    /// (zero or less) for any distance.
    pub reach: f32,
    /// How fast a body's vertex counts for less as it is further off: its
    /// share goes as one over the distance to this power.
    pub falloff: f32,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            nearest: 8,
            reach: 0.0,
            falloff: 2.0,
        }
    }
}

/// For each vertex of a shape, the body's vertices it follows, each with
/// its share. A vertex's shares add up to one; a vertex with nothing of the
/// body in reach follows nothing.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Ties {
    start: Vec<u32>,
    vertex: Vec<u32>,
    share: Vec<f32>,
}

impl Ties {
    /// The body's vertices that vertex `v` follows, each with its share.
    pub fn of(&self, v: usize) -> impl Iterator<Item = (u32, f32)> + '_ {
        let (from, to) = (self.start[v] as usize, self.start[v + 1] as usize);
        self.vertex[from..to]
            .iter()
            .copied()
            .zip(self.share[from..to].iter().copied())
    }

    pub fn len(&self) -> usize {
        self.start.len().saturating_sub(1)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

fn distance2(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

/// The body's vertices in a tree that halves them along their longest
/// spread at each step, to find the near ones without looking at them all.
struct Tree<'a> {
    points: &'a [[f32; 3]],
    /// The vertices, ordered so that each node's are together: the middle
    /// one is the node, those before it are on its low side.
    order: Vec<u32>,
    /// The axis each node splits on, by its place in `order`.
    axis: Vec<u8>,
}

impl<'a> Tree<'a> {
    fn new(points: &'a [[f32; 3]]) -> Tree<'a> {
        let mut tree = Tree {
            points,
            order: (0..points.len() as u32).collect(),
            axis: vec![0; points.len()],
        };
        tree.split(0, points.len());
        tree
    }

    fn split(&mut self, from: usize, to: usize) {
        if to - from < 2 {
            return;
        }
        let mut lo = [f32::INFINITY; 3];
        let mut hi = [f32::NEG_INFINITY; 3];
        for v in &self.order[from..to] {
            let p = self.points[*v as usize];
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        let axis = (0..3)
            .max_by(|a, b| (hi[*a] - lo[*a]).total_cmp(&(hi[*b] - lo[*b])))
            .unwrap_or(0);
        let middle = (from + to) / 2;
        let points = self.points;
        self.order[from..to].select_nth_unstable_by(middle - from, |a, b| {
            points[*a as usize][axis].total_cmp(&points[*b as usize][axis])
        });
        self.axis[middle] = axis as u8;
        self.split(from, middle);
        self.split(middle + 1, to);
    }

    /// The `count` vertices nearest `p`, nearest first, each with its
    /// distance squared.
    fn nearest(&self, p: [f32; 3], count: usize, into: &mut Vec<(f32, u32)>) {
        into.clear();
        if count > 0 {
            self.search(0, self.order.len(), p, count, into);
        }
    }

    fn search(
        &self,
        from: usize,
        to: usize,
        p: [f32; 3],
        count: usize,
        into: &mut Vec<(f32, u32)>,
    ) {
        if from >= to {
            return;
        }
        let middle = (from + to) / 2;
        let v = self.order[middle];
        let here = self.points[v as usize];
        let d2 = distance2(p, here);
        if into.len() < count || d2 < into[into.len() - 1].0 {
            let at = into.partition_point(|x| x.0 <= d2);
            into.insert(at, (d2, v));
            into.truncate(count);
        }
        let axis = self.axis[middle] as usize;
        let off = p[axis] - here[axis];
        let (near, far) = if off < 0.0 {
            ((from, middle), (middle + 1, to))
        } else {
            ((middle + 1, to), (from, middle))
        };
        self.search(near.0, near.1, p, count, into);
        // The other side only if it could hold something nearer.
        if into.len() < count || off * off < into[into.len() - 1].0 {
            self.search(far.0, far.1, p, count, into);
        }
    }
}

/// Tie each vertex of `shape` to the vertices of `body` nearest it.
pub fn tie(body: &[[f32; 3]], shape: &[[f32; 3]], options: Options) -> Ties {
    let tree = Tree::new(body);
    let mut ties = Ties {
        start: Vec::with_capacity(shape.len() + 1),
        ..Ties::default()
    };
    let mut near = Vec::new();
    let reach2 = if options.reach > 0.0 {
        options.reach * options.reach
    } else {
        f32::INFINITY
    };
    for p in shape {
        ties.start.push(ties.vertex.len() as u32);
        tree.nearest(*p, options.nearest.max(1), &mut near);
        near.retain(|(d2, _)| *d2 <= reach2);
        let Some((least, first)) = near.first().copied() else {
            continue;
        };
        // On top of a body's vertex, it does just what that one does.
        if least < 1e-10 {
            ties.vertex.push(first);
            ties.share.push(1.0);
            continue;
        }
        let from = ties.share.len();
        for (d2, v) in &near {
            ties.vertex.push(*v);
            ties.share.push(d2.powf(-options.falloff / 2.0));
        }
        let sum: f32 = ties.share[from..].iter().sum();
        for s in &mut ties.share[from..] {
            *s /= sum;
        }
    }
    ties.start.push(ties.vertex.len() as u32);
    ties
}

/// What a slider does to a shape tied to a body, from what it does to the
/// body. A vertex left moving less than `least` along every axis is left
/// out. `scratch` is room to work in, kept between calls.
pub fn carried(ties: &Ties, body: &[Diff], least: f32, scratch: &mut Vec<[f32; 3]>) -> Vec<Diff> {
    let most = body
        .iter()
        .map(|d| d.vertex as usize + 1)
        .max()
        .unwrap_or(0);
    scratch.clear();
    scratch.resize(most, [0.0; 3]);
    for d in body {
        scratch[d.vertex as usize] = d.offset;
    }
    let mut out = Vec::new();
    for v in 0..ties.len().min(u16::MAX as usize + 1) {
        let mut offset = [0.0f32; 3];
        for (from, share) in ties.of(v) {
            if let Some(o) = scratch.get(from as usize) {
                for k in 0..3 {
                    offset[k] += o[k] * share;
                }
            }
        }
        if offset.iter().any(|c| c.abs() >= least) {
            out.push(Diff {
                vertex: v as u16,
                offset,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_vertex_on_the_body_does_what_the_body_does_there() {
        let body = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let ties = tie(&body, &[[1.0, 0.0, 0.0]], Options::default());
        assert_eq!(ties.of(0).collect::<Vec<_>>(), vec![(1, 1.0)]);
        let moves = [Diff {
            vertex: 1,
            offset: [0.0, 0.0, 2.0],
        }];
        let got = carried(&ties, &moves, 0.0001, &mut Vec::new());
        assert_eq!(
            got,
            vec![Diff {
                vertex: 0,
                offset: [0.0, 0.0, 2.0]
            }]
        );
    }

    #[test]
    fn between_two_it_takes_more_from_the_nearer() {
        let body = [[0.0, 0.0, 0.0], [4.0, 0.0, 0.0]];
        let options = Options {
            nearest: 2,
            ..Options::default()
        };
        let ties = tie(&body, &[[1.0, 0.0, 0.0], [2.0, 0.0, 0.0]], options);
        let first: Vec<_> = ties.of(0).collect();
        // One and three away, by one over the distance squared: nine to one.
        assert_eq!(first[0].0, 0);
        assert!((first[0].1 - 0.9).abs() < 1e-6 && (first[1].1 - 0.1).abs() < 1e-6);
        let halfway: Vec<_> = ties.of(1).collect();
        assert!((halfway[0].1 - 0.5).abs() < 1e-6);
        let moves = [Diff {
            vertex: 0,
            offset: [0.0, 1.0, 0.0],
        }];
        let got = carried(&ties, &moves, 0.0001, &mut Vec::new());
        assert!((got[0].offset[1] - 0.9).abs() < 1e-6 && (got[1].offset[1] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn out_of_reach_follows_nothing_and_the_nearest_are_the_nearest() {
        let body: Vec<[f32; 3]> = (0..200)
            .map(|i| [(i % 20) as f32, (i / 20) as f32, 0.0])
            .collect();
        let far = Options {
            reach: 0.5,
            ..Options::default()
        };
        let ties = tie(&body, &[[50.0, 50.0, 50.0]], far);
        assert_eq!(ties.of(0).count(), 0);
        assert!(carried(&ties, &[], 0.0001, &mut Vec::new()).is_empty());
        // Checked against looking at every vertex.
        let options = Options {
            nearest: 5,
            ..Options::default()
        };
        for p in [
            [3.3, 4.6, 0.2],
            [-7.0, 2.0, 1.0],
            [19.9, 9.9, -3.0],
            [40.0, 4.0, 0.0],
        ] {
            let ties = tie(&body, &[p], options);
            let mut all: Vec<(f32, u32)> = body
                .iter()
                .enumerate()
                .map(|(i, b)| (distance2(p, *b), i as u32))
                .collect();
            all.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut want: Vec<f32> = all[..5].iter().map(|x| x.0).collect();
            let mut got: Vec<f32> = ties
                .of(0)
                .map(|(v, _)| distance2(p, body[v as usize]))
                .collect();
            want.sort_by(f32::total_cmp);
            got.sort_by(f32::total_cmp);
            assert_eq!(got, want, "{p:?}");
        }
        assert!(tie(&[], &[[0.0; 3]], options).of(0).next().is_none());
    }
}
