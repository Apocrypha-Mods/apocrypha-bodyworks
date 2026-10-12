//! Fallout 4's segments: what a `BSSubIndexTriShape` keeps after its
//! triangles. The triangles are dealt out, in order, to segments (what can
//! be cut from a body), and inside a segment to runs of its own. Measured
//! over 3,195 real shapes (`--example segments`): the segments follow one
//! another and hold every triangle between them; a segment's runs lie
//! inside it in order, and need not fill it.

use crate::io::{Reader, Writer};
use crate::NifError;

/// A run of a segment's triangles: where it starts and how many, counted
/// in triangles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Run {
    pub start: usize,
    pub count: usize,
    /// What the file keeps in the last of its four numbers.
    unused: u32,
    /// Where the file said it starts, kept for one with nothing in it:
    /// files say different things of those.
    stated: u32,
}

/// A segment: a run of the shape's triangles, and its own runs within.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub start: usize,
    pub count: usize,
    pub runs: Vec<Run>,
    /// Where the file said it starts, kept for one with nothing in it.
    stated: u32,
}

/// A shape's segments, and what follows them (which is which, something
/// for each, and a file's name), kept as it was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segments {
    pub segments: Vec<Segment>,
    after: Vec<u8>,
}

fn odd<T>(why: &str) -> Result<T, NifError> {
    Err(NifError::Unsupported(format!(
        "the shape's segments are not laid out as others were found to be ({why})"
    )))
}

impl Segments {
    /// Read what a shape of `triangles` triangles keeps after them.
    pub fn read(rest: &[u8], triangles: usize) -> Result<Self, NifError> {
        let mut r = Reader::new(rest);
        if r.u32()? as usize != triangles {
            return odd("they count other triangles than the shape has");
        }
        let count = r.count(16)?;
        let total = r.u32()? as usize;
        let mut segments = Vec::with_capacity(count);
        let (mut next, mut place) = (0usize, 0usize);
        for _ in 0..count {
            let (start, n, parent) = (r.u32()? as usize, r.u32()? as usize, r.u32()?);
            let inner = r.count(16)?;
            // No more triangles than the shape has left to give: counted
            // without letting a number too large come round to a small one.
            if n > triangles - next {
                return odd("a segment holds more triangles than there are");
            }
            if parent != u32::MAX || (n != 0 && start != next * 3) {
                return odd("a segment does not follow the one before");
            }
            let mut runs = Vec::with_capacity(inner);
            let mut reached = next;
            for _ in 0..inner {
                let (a, m, p, unused) = (
                    r.u32()? as usize,
                    r.u32()? as usize,
                    r.u32()? as usize,
                    r.u32()?,
                );
                if p != place {
                    return odd("a run is not its segment's");
                }
                let begin = if m == 0 { reached } else { a / 3 };
                if m != 0
                    && (a % 3 != 0 || begin < reached || begin > next + n || m > next + n - begin)
                {
                    return odd("a run is not inside its segment");
                }
                reached = begin + m;
                runs.push(Run {
                    start: begin,
                    count: m,
                    unused,
                    stated: a as u32,
                });
            }
            segments.push(Segment {
                start: next,
                count: n,
                runs,
                stated: start as u32,
            });
            place += 1 + inner;
            next += n;
        }
        if next != triangles || place != total {
            return odd("they do not hold every triangle");
        }
        Ok(Segments {
            segments,
            after: r.take(r.left())?.to_vec(),
        })
    }

    pub fn write(&self) -> Vec<u8> {
        let mut w = Writer::default();
        let triangles: usize = self.segments.iter().map(|s| s.count).sum();
        let total: usize = self.segments.iter().map(|s| 1 + s.runs.len()).sum();
        w.u32(triangles as u32);
        w.u32(self.segments.len() as u32);
        w.u32(total as u32);
        let mut place = 0u32;
        for s in &self.segments {
            // One with nothing in it starts where its file said.
            w.u32(if s.count == 0 {
                s.stated
            } else {
                (s.start * 3) as u32
            });
            w.u32(s.count as u32);
            w.u32(u32::MAX);
            w.u32(s.runs.len() as u32);
            for run in &s.runs {
                w.u32(if run.count == 0 {
                    run.stated
                } else {
                    (run.start * 3) as u32
                });
                w.u32(run.count as u32);
                w.u32(place);
                w.u32(run.unused);
            }
            place += 1 + s.runs.len() as u32;
        }
        w.bytes(&self.after);
        w.out
    }

    /// The groups triangles are dealt into, in the order a shape rebuilt
    /// from them lays them down: for each segment its runs, then what of
    /// it is in no run.
    pub fn groups(&self) -> usize {
        self.segments.iter().map(|s| s.runs.len() + 1).sum()
    }

    /// Which group each triangle is in, as [`Segments::groups`] counts
    /// them.
    pub fn group_of(&self) -> Vec<usize> {
        let triangles: usize = self.segments.iter().map(|s| s.count).sum();
        let mut of = vec![0usize; triangles];
        let mut group = 0usize;
        for s in &self.segments {
            let loose = group + s.runs.len();
            for slot in of.iter_mut().skip(s.start).take(s.count) {
                *slot = loose;
            }
            for (k, run) in s.runs.iter().enumerate() {
                for slot in of.iter_mut().skip(run.start).take(run.count) {
                    *slot = group + k;
                }
            }
            group = loose + 1;
        }
        of
    }

    /// The segments of a shape whose triangles are dealt out anew: how many
    /// each group now has, the triangles laid down group after group.
    /// `counts` has one number for each of [`Segments::groups`].
    pub fn regrouped(&self, counts: &[usize]) -> Option<Segments> {
        if counts.len() != self.groups() {
            return None;
        }
        let mut out = self.clone();
        let (mut next, mut group) = (0usize, 0usize);
        for s in &mut out.segments {
            s.start = next;
            for run in &mut s.runs {
                run.start = next;
                run.count = counts[group];
                next += run.count;
                group += 1;
            }
            next += counts[group];
            group += 1;
            s.count = next - s.start;
        }
        Some(out)
    }

    /// Whether another shape's segments are these over again: as many, with
    /// as many runs each, and the same said of each after them. Only then
    /// is a group of one the same thing as that group of the other.
    pub fn alike(&self, other: &Segments) -> bool {
        self.after == other.after
            && self.segments.len() == other.segments.len()
            && self
                .segments
                .iter()
                .zip(&other.segments)
                .all(|(a, b)| a.runs.len() == b.runs.len())
    }

    /// A shape's triangles as its groups hold them: one list for each of
    /// [`Segments::groups`].
    pub fn dealt(&self, triangles: &[[u16; 3]]) -> Vec<Vec<[u16; 3]>> {
        let mut parts = vec![Vec::new(); self.groups()];
        for (t, group) in triangles.iter().zip(self.group_of()) {
            parts[group].push(*t);
        }
        parts
    }

    /// Triangles group by group, laid down one group after another, and
    /// the segments that then hold them. Nothing where there are not as
    /// many lists as groups.
    pub fn gathered(&self, parts: Vec<Vec<[u16; 3]>>) -> Option<(Vec<[u16; 3]>, Segments)> {
        let counts: Vec<usize> = parts.iter().map(Vec::len).collect();
        let segments = self.regrouped(&counts)?;
        Some((parts.into_iter().flatten().collect(), segments))
    }
}

#[cfg(test)]
mod tests {
    use super::Segments;

    fn table(triangles: u32, segments: &[(u32, u32, &[(u32, u32)])]) -> Vec<u8> {
        let total: usize = segments.iter().map(|s| 1 + s.2.len()).sum();
        let mut out = Vec::new();
        for w in [triangles, segments.len() as u32, total as u32] {
            out.extend(w.to_le_bytes());
        }
        let mut place = 0u32;
        for (start, count, runs) in segments {
            for w in [*start, *count, u32::MAX, runs.len() as u32] {
                out.extend(w.to_le_bytes());
            }
            for (a, n) in *runs {
                for w in [*a, *n, place, 0] {
                    out.extend(w.to_le_bytes());
                }
            }
            place += 1 + runs.len() as u32;
        }
        out
    }

    #[test]
    fn a_table_reads_and_writes_back() {
        let bytes = table(5, &[(0, 2, &[]), (6, 3, &[(6, 1), (12, 1)])]);
        let read = Segments::read(&bytes, 5).unwrap();
        assert_eq!(read.write(), bytes);
        assert_eq!(read.groups(), 4);
        assert_eq!(read.group_of(), vec![0, 0, 1, 3, 2]);
    }

    #[test]
    fn counts_that_come_round_are_refused() {
        // Two segments whose counts add up, past what a number holds, to
        // the one triangle there is.
        let wrapped = table(1, &[(0, u32::MAX, &[]), (u32::MAX.wrapping_mul(3), 2, &[])]);
        assert!(Segments::read(&wrapped, 1).is_err());
        // A run whose end comes round to before its start.
        let run = table(2, &[(0, 2, &[(3, u32::MAX)])]);
        assert!(Segments::read(&run, 2).is_err());
        // One that says more triangles than the shape has.
        assert!(Segments::read(&table(1, &[(0, 2, &[])]), 1).is_err());
    }

    #[test]
    fn two_tables_are_alike_only_when_segmented_the_same() {
        let four = Segments::read(
            &table(4, &[(0, 1, &[]), (3, 1, &[]), (6, 1, &[]), (9, 1, &[])]),
            4,
        )
        .unwrap();
        let three =
            Segments::read(&table(4, &[(0, 1, &[]), (3, 2, &[(3, 1)]), (9, 1, &[])]), 4).unwrap();
        // As many groups, and not the same groups.
        assert_eq!(four.groups(), three.groups());
        assert!(!four.alike(&three));
        assert!(four.alike(&four.clone()));
    }
}
