/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

/// A single delta in a revlog or bundle.
///
/// The range from `start`-`end` is replaced with the `content`.
#[derive(Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Delta {
    pub start: usize,
    pub end: usize,
    pub content: Vec<u8>, // need to own because of compression
}

fn snip<T>(start: usize, end: usize, slice: &[T]) -> &[T] {
    let (h, _) = slice.split_at(end);
    let (_, t) = h.split_at(start);
    t
}

/// Apply a set of `Delta`s to an input text, returning the result.
pub fn apply(text: &[u8], deltas: &[Delta]) -> Vec<u8> {
    let mut chunks = Vec::with_capacity(deltas.len() * 2);
    let mut off = 0;

    for d in deltas {
        assert!(off <= d.start);
        if off < d.start {
            chunks.push(snip(off, d.start, text));
        }
        if !d.content.is_empty() {
            chunks.push(d.content.as_ref())
        }
        off = d.end;
    }
    if off < text.len() {
        chunks.push(snip(off, text.len(), text));
    }

    let mut ret = Vec::new();
    for s in chunks {
        ret.extend_from_slice(s);
    }
    ret
}

#[cfg(test)]
mod test {
    use mononoke_macros::mononoke;

    use super::apply;
    use super::Delta;

    #[mononoke::test]
    fn test_1() {
        let text = b"aaaa\nbbbb\ncccc\n";
        let delta = Delta {
            start: 5,
            end: 10,
            content: (&b"xxxx\n"[..]).into(),
        };
        let deltas = [delta; 1];

        let res = apply(text, &deltas[..]);
        assert_eq!(&res[..], b"aaaa\nxxxx\ncccc\n");
    }

    #[mononoke::test]
    fn test_2() {
        let text = b"bbbb\ncccc\n";
        let deltas = [
            Delta {
                start: 0,
                end: 5,
                content: (&b"aaaabbbb\n"[..]).into(),
            },
            Delta {
                start: 10,
                end: 10,
                content: (&b"dddd\n"[..]).into(),
            },
        ];

        let res = apply(text, &deltas[..]);
        assert_eq!(&res[..], b"aaaabbbb\ncccc\ndddd\n");
    }

    #[mononoke::test]
    fn test_3a() {
        let text = b"aaaa\nbbbb\ncccc\n";
        let deltas = [Delta {
            start: 0,
            end: 15,
            content: (&b"zzzz\nyyyy\nxxxx\n"[..]).into(),
        }];

        let res = apply(text, &deltas[..]);
        assert_eq!(&res[..], b"zzzz\nyyyy\nxxxx\n");
    }

    #[mononoke::test]
    fn test_3b() {
        let text = b"aaaa\nbbbb\ncccc\n";
        let deltas = [
            Delta {
                start: 0,
                end: 5,
                content: (&b"zzzz\n"[..]).into(),
            },
            Delta {
                start: 5,
                end: 10,
                content: (&b"yyyy\n"[..]).into(),
            },
            Delta {
                start: 10,
                end: 15,
                content: (&b"xxxx\n"[..]).into(),
            },
        ];

        let res = apply(text, &deltas[..]);
        assert_eq!(&res[..], b"zzzz\nyyyy\nxxxx\n");
    }

    #[mononoke::test]
    fn test_4() {
        let text = b"aaaa\nbbbb";
        let deltas = [Delta {
            start: 5,
            end: 9,
            content: (&b"bbbbcccc"[..]).into(),
        }];

        let res = apply(text, &deltas[..]);
        assert_eq!(&res[..], b"aaaa\nbbbbcccc");
    }

    #[mononoke::test]
    fn test_5() {
        let text = b"aaaa\nbbbb\ncccc\n";
        let deltas = [Delta {
            start: 5,
            end: 10,
            content: (&b""[..]).into(),
        }];

        let res = apply(text, &deltas[..]);
        assert_eq!(&res[..], b"aaaa\ncccc\n");
    }
}
