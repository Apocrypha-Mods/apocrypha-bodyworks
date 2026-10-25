//! One list of slider data on its own (`.bsd`): the older form, a file for
//! each slider and shape, where an `.osd` holds them all by name.
//!
//! A count, then for each vertex its number and its offset, little-endian:
//! 4 + 16 bytes a vertex and nothing after. That holds for all 3,781 of a
//! real outfit mod's files (Devious Devices, `--example bsd`).

use crate::osd::Diff;
use crate::Error;

/// A file's vertices, in its order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Bsd {
    pub diffs: Vec<Diff>,
    /// For each vertex, the two bytes its number's slot holds past the
    /// number. A vertex's number takes four bytes, but over half the real
    /// files (2,086 of 3,781) have something other than nothing in the
    /// upper two for a few vertices, while the lower two count on from the
    /// vertex before as everywhere else: a number written in two bytes into
    /// a slot of four, the rest left as it was. Kept, so a file writes back
    /// as it was; empty, they are written as nothing.
    pub unused: Vec<u16>,
}

impl Bsd {
    pub fn read(bytes: &[u8]) -> Result<Self, Error> {
        let count = bytes
            .get(..4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
            .ok_or(Error::Truncated)?;
        let body = &bytes[4..];
        let need = count.checked_mul(16).ok_or(Error::Truncated)?;
        if body.len() < need {
            return Err(Error::Truncated);
        }
        if body.len() > need {
            return Err(Error::Invalid(format!(
                "{} bytes after the last vertex",
                body.len() - need
            )));
        }
        let rows = body.as_chunks::<16>().0;
        let diffs = rows
            .iter()
            .map(|d| {
                let f = |i: usize| f32::from_le_bytes([d[i], d[i + 1], d[i + 2], d[i + 3]]);
                Diff {
                    vertex: u16::from_le_bytes([d[0], d[1]]),
                    offset: [f(4), f(8), f(12)],
                }
            })
            .collect();
        let unused: Vec<u16> = rows
            .iter()
            .map(|d| u16::from_le_bytes([d[2], d[3]]))
            .collect();
        Ok(Bsd {
            diffs,
            unused: if unused.iter().all(|u| *u == 0) {
                Vec::new()
            } else {
                unused
            },
        })
    }

    /// A file of `diffs` alone.
    pub fn of(diffs: &[Diff]) -> Self {
        Bsd {
            diffs: diffs.to_vec(),
            unused: Vec::new(),
        }
    }

    pub fn write(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(4 + 16 * self.diffs.len());
        out.extend((self.diffs.len() as u32).to_le_bytes());
        for (i, d) in self.diffs.iter().enumerate() {
            out.extend(d.vertex.to_le_bytes());
            out.extend(self.unused.get(i).copied().unwrap_or(0).to_le_bytes());
            for c in d.offset {
                out.extend(c.to_le_bytes());
            }
        }
        out
    }
}

/// Whether a set's file of slider data is one of these, by its name.
pub fn named(file: &str) -> bool {
    file.len() > 4
        && file
            .get(file.len() - 4..)
            .is_some_and(|e| e.eq_ignore_ascii_case(".bsd"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;

    /// The system's allocator, keeping the largest room asked for on each
    /// thread, so a test can say what reading cost.
    struct Largest;

    thread_local! {
        static LARGEST: Cell<usize> = const { Cell::new(0) };
    }

    unsafe impl GlobalAlloc for Largest {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let _ = LARGEST.try_with(|l| l.set(l.get().max(layout.size())));
            System.alloc(layout)
        }
        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            System.dealloc(ptr, layout)
        }
        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
            let _ = LARGEST.try_with(|l| l.set(l.get().max(size)));
            System.realloc(ptr, layout, size)
        }
    }

    #[global_allocator]
    static ALLOCATOR: Largest = Largest;

    fn diff(vertex: u16, offset: [f32; 3]) -> Diff {
        Diff { vertex, offset }
    }

    /// A file as the layout has it: a count, then a slot and three numbers
    /// for each vertex.
    fn file(rows: &[(u16, u16, [f32; 3])]) -> Vec<u8> {
        let mut out = (rows.len() as u32).to_le_bytes().to_vec();
        for (vertex, unused, offset) in rows {
            out.extend(vertex.to_le_bytes());
            out.extend(unused.to_le_bytes());
            for c in offset {
                out.extend(c.to_le_bytes());
            }
        }
        out
    }

    #[test]
    fn a_file_reads_and_writes_back_as_it_was() {
        let bytes = file(&[(3, 0, [0.5, -1.25, 2.0]), (700, 0, [0.0, 0.0, -0.125])]);
        let got = Bsd::read(&bytes).unwrap();
        assert_eq!(
            got.diffs,
            [diff(3, [0.5, -1.25, 2.0]), diff(700, [0.0, 0.0, -0.125])]
        );
        assert!(got.unused.is_empty());
        assert_eq!(got.write(), bytes);
        let none = file(&[]);
        assert_eq!(none, [0, 0, 0, 0]);
        assert_eq!(Bsd::read(&none).unwrap(), Bsd::default());
        assert_eq!(Bsd::default().write(), none);
    }

    #[test]
    fn a_file_of_diffs_alone_writes_nothing_past_each_number() {
        let diffs = [diff(1, [1.0, 2.0, 3.0]), diff(65535, [-4.0, 0.0, 0.25])];
        let made = Bsd::of(&diffs);
        assert_eq!(made.diffs, diffs);
        assert!(made.unused.is_empty());
        let bytes = made.write();
        assert_eq!(
            bytes,
            file(&[(1, 0, [1.0, 2.0, 3.0]), (65535, 0, [-4.0, 0.0, 0.25])])
        );
        assert_eq!(&bytes[6..8], [0, 0]);
        assert_eq!(&bytes[22..24], [0, 0]);
    }

    #[test]
    fn what_a_slot_holds_past_its_number_is_kept_byte_for_byte() {
        let bytes = file(&[
            (10, 0, [1.0, 0.0, 0.0]),
            (11, 0xbeef, [0.0, 1.0, 0.0]),
            (12, 1, [0.0, 0.0, 1.0]),
        ]);
        let got = Bsd::read(&bytes).unwrap();
        // The number is the lower two bytes alone.
        assert_eq!(
            got.diffs.iter().map(|d| d.vertex).collect::<Vec<_>>(),
            [10, 11, 12]
        );
        assert_eq!(got.unused, [0, 0xbeef, 1]);
        assert_eq!(got.write(), bytes);
        // Fewer kept than there are vertices: the rest are written as nothing.
        let short = Bsd {
            diffs: got.diffs.clone(),
            unused: vec![7],
        };
        assert_eq!(
            short.write(),
            file(&[
                (10, 7, [1.0, 0.0, 0.0]),
                (11, 0, [0.0, 1.0, 0.0]),
                (12, 0, [0.0, 0.0, 1.0]),
            ])
        );
    }

    #[test]
    fn a_file_cut_anywhere_or_carrying_more_is_refused() {
        let bytes = file(&[(3, 9, [0.5, -1.25, 2.0]), (4, 0, [1.0, 1.0, 1.0])]);
        for cut in 0..bytes.len() {
            assert_eq!(
                Bsd::read(&bytes[..cut]),
                Err(Error::Truncated),
                "cut at {cut}"
            );
        }
        assert!(Bsd::read(&bytes).is_ok());
        for more in 1..=16 {
            let mut long = bytes.clone();
            long.extend(std::iter::repeat_n(0, more));
            assert_eq!(
                Bsd::read(&long),
                Err(Error::Invalid(format!(
                    "{more} bytes after the last vertex"
                ))),
            );
        }
    }

    #[test]
    fn a_count_far_past_the_file_is_refused_before_room_is_made_for_it() {
        for count in [u32::MAX, u32::MAX / 16 + 1, 1 << 24] {
            let mut bytes = count.to_le_bytes().to_vec();
            bytes.extend([0; 16]);
            LARGEST.with(|l| l.set(0));
            assert_eq!(Bsd::read(&bytes), Err(Error::Truncated), "{count}");
            let largest = LARGEST.with(Cell::get);
            assert!(largest < 4096, "{largest} bytes asked for, for {count}");
        }
        // The same measure sees what a real read costs.
        let bytes = file(&vec![(1, 0, [0.0; 3]); 1000]);
        LARGEST.with(|l| l.set(0));
        assert_eq!(Bsd::read(&bytes).unwrap().diffs.len(), 1000);
        assert!(LARGEST.with(Cell::get) >= 1000 * std::mem::size_of::<Diff>());
    }

    #[test]
    fn a_file_is_one_of_these_by_its_name_alone() {
        for yes in [
            "Breasts1.bsd",
            "a.bsd",
            "COLLAR.BSD",
            "x.Bsd",
            "collar7B BCup High.bsd",
            "Folder\\Shape#Slider.bsd",
            "ShapeData/X/y.bSd",
        ] {
            assert!(named(yes), "{yes}");
        }
        for no in [
            "",
            ".bsd",
            "bsd",
            "a.osd",
            "Body.osd",
            "a.bsd.osd",
            "abcbsd",
            "a.bsdx",
            "a.bs",
            "a日本",
            "日本.bs",
        ] {
            assert!(!named(no), "{no}");
        }
    }
}
