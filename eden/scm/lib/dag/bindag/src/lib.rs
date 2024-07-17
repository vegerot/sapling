/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

// See D16294467 for the "bindag" format specification.

use std::ops::Deref;
use std::ops::Range;

use vlqencoding::VLQDecode;

mod gca;
pub mod octopus;
mod range;
mod test_context;

pub use gca::gca;
pub use range::range;
pub use test_context::GeneralTestContext;
pub use test_context::OctopusTestContext;
pub use test_context::TestContext;

pub static MOZILLA: &[u8] = include_bytes!("mozilla-central.bindag");
pub static GIT: &[u8] = include_bytes!("git.bindag");

/// "smallvec" optimization
#[derive(Clone, Copy)]
pub struct ParentRevs([usize; 2]);

impl ParentRevs {
    const NONE: usize = usize::max_value();
    const EMPTY: Self = Self([Self::NONE; 2]);
}

impl From<Vec<usize>> for ParentRevs {
    fn from(revs: Vec<usize>) -> Self {
        assert!(revs.len() <= 2);
        match revs.len() {
            0 => Self([Self::NONE, Self::NONE]),
            1 => Self([revs[0], Self::NONE]),
            2 => Self([revs[0], revs[1]]),
            n => panic!("unsupported len: {}", n),
        }
    }
}

impl AsRef<[usize]> for ParentRevs {
    fn as_ref(&self) -> &[usize] {
        if self.0[0] == Self::NONE {
            &self.0[0..0]
        } else if self.0[1] == Self::NONE {
            &self.0[0..1]
        } else {
            &self.0[..]
        }
    }
}

impl Deref for ParentRevs {
    type Target = [usize];

    fn deref(&self) -> &[usize] {
        self.as_ref()
    }
}

pub fn parse_bindag(bindag: &[u8]) -> Vec<ParentRevs> {
    let mut parents = Vec::new();
    let mut cur = std::io::Cursor::new(bindag);
    let mut read_next = move || -> Result<usize, _> { cur.read_vlq() };

    while let Ok(i) = read_next() {
        let next_id = parents.len();
        match i {
            0 => {
                // no parents
                parents.push(vec![].into());
            }
            1 => {
                // 1 specified parent
                let p1 = next_id - read_next().unwrap() - 1;
                parents.push(vec![p1].into());
            }
            2 => {
                // 2 specified parents
                let p1 = next_id - read_next().unwrap() - 1;
                let p2 = next_id - read_next().unwrap() - 1;
                parents.push(vec![p1, p2].into());
            }
            3 => {
                // 2 parents, p2 specified
                let p1 = next_id - 1;
                let p2 = next_id - read_next().unwrap() - 1;
                parents.push(vec![p1, p2].into());
            }
            4 => {
                // 2 parents, p1 specified
                let p1 = next_id - read_next().unwrap() - 1;
                let p2 = next_id - 1;
                parents.push(vec![p1, p2].into());
            }
            _ => {
                // n commits
                for _ in 0..(i - 4) {
                    let p1 = parents.len() - 1;
                    parents.push(vec![p1].into());
                }
            }
        }
    }

    parents
}

/// Slice a graph. Remove unrefered edges.
pub fn slice_parents(parents: Vec<ParentRevs>, range: Range<usize>) -> Vec<ParentRevs> {
    let start: usize = range.start;
    let end: usize = range.end;
    if start == 0 && end >= parents.len() {
        return parents;
    }

    let mut result = Vec::with_capacity(end - start);
    for i in &parents[range] {
        let new_parents: Vec<usize> = i
            .as_ref()
            .iter()
            .filter_map(|&p| {
                if p < start || p >= end {
                    None
                } else {
                    Some(p - start)
                }
            })
            .collect();
        result.push(new_parents.into())
    }
    result
}

/// Compact form of segments (no allocation).
#[derive(Copy, Clone)]
pub struct CompactSegment {
    pub low: usize,
    pub high: usize,
    pub parents: ParentRevs,
}

/// Parse bindag and return "CompactSegment".
pub fn parse_bindag_segments(bindag: &[u8]) -> Vec<CompactSegment> {
    let mut segments = Vec::new();
    let mut cur = std::io::Cursor::new(bindag);
    let mut read_next = move || -> Result<usize, _> { cur.read_vlq() };

    let mut next_id = 0;

    while let Ok(i) = read_next() {
        let low = next_id;
        let mut high = low;
        let mut parents = ParentRevs::EMPTY;
        match i {
            0 => {
                // no parents
            }
            1 => {
                // 1 specified parent
                let p1 = next_id - read_next().unwrap() - 1;
                parents.0[0] = p1;
            }
            2 => {
                // 2 specified parents
                let p1 = next_id - read_next().unwrap() - 1;
                let p2 = next_id - read_next().unwrap() - 1;
                parents.0 = [p1, p2];
            }
            3 => {
                // 2 parents, p2 specified
                let p1 = next_id - 1;
                let p2 = next_id - read_next().unwrap() - 1;
                parents.0 = [p1, p2];
            }
            4 => {
                // 2 parents, p1 specified
                let p1 = next_id - read_next().unwrap() - 1;
                let p2 = next_id - 1;
                parents.0 = [p1, p2];
            }
            _ => {
                // n commits (5: 1 commit)
                high += i - 5;
                parents.0[0] = low - 1;
            }
        }
        next_id = high + 1;
        segments.push(CompactSegment { low, high, parents });
    }

    segments
}
