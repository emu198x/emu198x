//! Every request/transition versus independently compiled registered functions.
use commodore_agnus_ocs::{DisplayDmaChannel, DisplayDmaInputs, DisplayDmaSequencer};
use std::collections::BTreeMap;

fn numbers(line: &str) -> Vec<i32> {
    line.split(',')
        .map(|value| value.parse().expect("reference integer"))
        .collect()
}

#[test]
fn all_registered_reservations_transitions_and_wraps_match_with_snapshot_replay() {
    let cases = include_str!(
        "../../../test-data/commodore/amiga/display-dma-sequencer/registered-cases.csv"
    );
    let csv = include_str!(
        "../../../test-data/commodore/amiga/display-dma-sequencer/registered-events.csv"
    );
    let mut expected = BTreeMap::new();
    for line in csv.lines().skip(1) {
        let row = numbers(line);
        assert_eq!(row.len(), 15);
        assert!(expected.insert((row[0], row[1], row[2]), row).is_none());
    }
    assert_eq!(expected.len(), 74_577);
    let (mut cases_seen, mut events_seen, mut requests_seen, mut modulo_seen) = (0, 0, 0, 0);
    for line in cases.lines().skip(1) {
        let values = numbers(line);
        let [id, chip, mode, res, length, scenario] = values[..] else {
            panic!("reference case shape differs");
        };
        let width_words = match mode {
            0 => 1,
            3 => 4,
            _ => 2,
        };
        let (fetch_unit, fetch_start, max_planes) = match (width_words, res) {
            (1, 0) => (8, 8, 8),
            (1, 1) => (8, 4, 4),
            (1, _) => (8, 2, 2),
            (2, 0) => (16, 16, 8),
            (2, 1) => (8, 8, 8),
            (2, _) => (8, 4, 4),
            (4, 0) => (32, 32, 8),
            (4, 1) => (16, 16, 8),
            (4, _) => (8, 8, 8),
            _ => panic!("invalid reference width"),
        };
        let mut sequencer = DisplayDmaSequencer::default();
        let mut input = DisplayDmaInputs {
            hpos: 0,
            clock: true,
            enhanced: chip != 0,
            alice: chip == 2,
            dma: true,
            vertical: true,
            hard_limit_disabled: scenario == 9 || chip != 0 && res == 2,
            start: match scenario {
                1 => 60,
                3 => 28,
                4 => 16,
                _ => 56,
            },
            stop: match scenario {
                2 => 56,
                3 => 232,
                7 => 96,
                _ => 208,
            },
            fetch_unit,
            fetch_start,
            max_planes,
            planes: if chip != 2 {
                max_planes.min(6)
            } else {
                max_planes
            },
            width_words,
            fmode: mode as u16,
        };
        for line in 0..2 {
            for h in 0..length {
                input.hpos = h as u16;
                let next = if h + 1 == length { 0 } else { h + 1 };
                input.clock = h & 1 != next & 1;
                match scenario {
                    5 => match h {
                        80 => input.dma = false,
                        88 => input.start = 112,
                        96 => input.dma = true,
                        _ => {}
                    },
                    6 => match h {
                        214 => input.dma = false,
                        222 => input.dma = true,
                        _ => {}
                    },
                    7 if h == 112 => {
                        input.start = 128;
                        input.stop = 176;
                    }
                    8 => match h {
                        80 => input.vertical = false,
                        96 => input.vertical = true,
                        _ => {}
                    },
                    _ => {}
                }
                let before = sequencer.phases();
                let request = sequencer.tick(input);
                let after = sequencer.phases();
                let state_changed = before.0 != after.0
                    || before.2 != after.2
                    || before.3 != after.3
                    || before.4 != after.4
                    || before.5 != after.5;
                let key = (id, line, h);
                if request.is_none() && !state_changed {
                    assert!(
                        !expected.contains_key(&key),
                        "missing native event at {key:?}"
                    );
                    continue;
                }
                let row = expected
                    .remove(&key)
                    .expect("unexpected native request/transition");
                let (plane, add_modulo) = request.map_or((-1, false), |request| {
                    let DisplayDmaChannel::Bitplane(plane) = request.channel else {
                        panic!("not a bitplane");
                    };
                    assert_eq!(request.width_words, width_words);
                    assert_eq!(request.fmode, mode as u16);
                    (i32::from(plane), request.add_modulo)
                });
                let native = [
                    id,
                    line,
                    h,
                    i32::from(input.clock),
                    plane,
                    i32::from(add_modulo),
                    i32::from(before.0),
                    i32::from(before.1),
                    i32::from(before.2),
                    i32::from(after.0),
                    i32::from(after.1),
                    i32::from(after.2),
                    i32::from(after.3),
                    i32::from(after.4),
                    i32::from(after.5),
                ];
                let mut reference = row.clone();
                reference[7] &= 31;
                reference[10] &= 31;
                assert_eq!(
                    native.as_slice(),
                    reference,
                    "case {values:?}, line {line}, h {h}"
                );
                let bytes = postcard::to_allocvec(&sequencer).expect("serialize fetch phases");
                let restored: DisplayDmaSequencer =
                    postcard::from_bytes(&bytes).expect("restore fetch phases");
                assert_eq!(sequencer, restored);
                assert!(restored.validate().is_ok());
                sequencer = restored;
                events_seen += 1;
                requests_seen += usize::from(request.is_some());
                modulo_seen += usize::from(add_modulo);
            }
        }
        cases_seen += 1;
    }
    assert!(expected.is_empty());
    assert_eq!(
        (cases_seen, events_seen, requests_seen, modulo_seen),
        (336, 74_577, 71_835, 3_772)
    );
}
