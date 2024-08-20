/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use dag::Id;
use dag::IdSet;
use minibench::bench;
use minibench::elapsed;

pub fn main() {
    // Ruby code to generate random IdSet:
    // 119.times.map{rand(8)+1}.reduce([0]){|a,b|a+[b+a[-1]]}.each_slice(2).map{|x|"#{x*'..='}"}*', '
    #[rustfmt::skip]
    let span1 = IdSet::from_spans(vec![
        4..=6, 8..=10, 12..=13, 19..=20, 25..=30, 35..=43, 51..=52, 60..=67, 75..=81, 89..=93,
        94..=97, 105..=111, 116..=121, 129..=135, 136..=144, 146..=147, 155..=157, 164..=172,
        180..=188, 193..=201, 204..=212, 220..=221, 227..=234, 239..=241, 248..=251, 253..=257,
        259..=260, 261..=263, 266..=272, 274..=278, 285..=286, 290..=297, 299..=307, 308..=310,
        317..=322, 325..=332, 336..=343, 350..=356, 361..=365, 373..=376, 379..=385, 393..=400,
        405..=410, 416..=421, 422..=424, 431..=432, 433..=434, 440..=444, 452..=460, 465..=466,
        469..=475, 480..=488, 492..=496, 502..=509, 513..=515, 523..=528, 530..=538, 545..=548,
        549..=553, 557..=563
    ].into_iter().map(|r| Id(*r.start())..=Id(*r.end())));

    #[rustfmt::skip]
    let span2 = IdSet::from_spans(vec![
        0..=1, 6..=9, 16..=22, 26..=33, 38..=45, 51..=54, 61..=68, 71..=78, 85..=91, 94..=98,
        102..=105, 106..=111, 112..=114, 117..=121, 124..=128, 130..=132, 138..=140, 142..=143,
        144..=151, 154..=161, 162..=164, 165..=170, 177..=179, 182..=185, 193..=196, 199..=207,
        208..=213, 214..=217, 224..=228, 236..=243, 251..=258, 261..=268, 272..=277, 285..=293,
        296..=302, 308..=312, 318..=320, 326..=327, 328..=329, 333..=336, 340..=341, 342..=343,
        350..=352, 353..=360, 368..=371, 375..=382, 387..=389, 390..=395, 396..=402, 408..=411,
        413..=415, 416..=419, 425..=433, 439..=444, 445..=446, 454..=460, 461..=465, 472..=475,
        480..=486, 487..=491
    ].into_iter().map(|r| Id(*r.start())..=Id(*r.end())));

    const N: usize = 10000;

    bench("intersection", || {
        elapsed(|| {
            for _ in 0..N {
                span1.intersection(&span2);
            }
        })
    });

    bench("union", || {
        elapsed(|| {
            for _ in 0..N {
                span1.union(&span2);
            }
        })
    });

    bench("difference", || {
        elapsed(|| {
            for _ in 0..N {
                span1.difference(&span2);
            }
        })
    });

    bench("push_span (low)", || {
        let mut sets: Vec<_> = (0..N).map(|_| span1.clone()).collect();
        elapsed(move || {
            for set in sets.iter_mut() {
                set.push(Id(0)..=Id(3))
            }
        })
    });

    bench("push_span (middle)", || {
        let mut sets: Vec<_> = (0..N).map(|_| span1.clone()).collect();
        elapsed(move || {
            for set in sets.iter_mut() {
                set.push(Id(287)..=Id(288))
            }
        })
    });

    bench("push_span (middle, large)", || {
        let mut sets: Vec<_> = (0..N).map(|_| span1.clone()).collect();
        elapsed(move || {
            for set in sets.iter_mut() {
                set.push(Id(200)..=Id(300))
            }
        })
    });

    bench("push_span (high)", || {
        let mut sets: Vec<_> = (0..N).map(|_| span1.clone()).collect();
        elapsed(move || {
            for set in sets.iter_mut() {
                set.push(Id(565)..=Id(570))
            }
        })
    });
}
