//! Standalone diagnostic: compare the live Agnus write path with reference requests.
use commodore_agnus_ocs::{Agnus, DisplayDmaChannel, SpriteDmaVerticalTiming};
use std::{collections::BTreeMap, error::Error, fs, path::Path};

type Event = (u16, u16, u16, u16);

#[cfg(not(test))]
fn main() -> Result<(), Box<dyn Error>> {
    let directory = std::env::args()
        .nth(1)
        .ok_or("expected fixture directory")?;
    verify(Path::new(&directory))
}

pub fn verify(directory: &Path) -> Result<(), Box<dyn Error>> {
    let mut expected: BTreeMap<u16, Vec<Event>> = BTreeMap::new();
    let parse = |line: &str| -> Result<Vec<u16>, Box<dyn Error>> {
        Ok(line.split(',').map(str::parse).collect::<Result<_, _>>()?)
    };
    for line in fs::read_to_string(directory.join("registered-events.csv"))?
        .lines()
        .skip(1)
    {
        let row = parse(line)?;
        expected
            .entry(row[0])
            .or_default()
            .push((row[1], row[2], row[3], row[4]));
    }
    let mut cases = 0;
    let mut mismatches = 0;
    let mut controls = 0;
    let mut control_mismatches = 0;
    let mut requests = 0;
    for line in fs::read_to_string(directory.join("registered-cases.csv"))?
        .lines()
        .skip(1)
    {
        let row = parse(line)?;
        let [
            id,
            chip,
            length,
            phase,
            reg,
            scenario,
            at,
            value,
            start,
            stop,
        ] = row[..]
        else {
            return Err("invalid case row".into());
        };
        let mut agnus = Agnus::new();
        agnus.agnus_id = match chip {
            0 => 0,
            1 => 0x2000,
            _ => 0x2300,
        };
        agnus.max_bitplanes = if chip == 2 { 8 } else { 6 };
        agnus.ddfstrt = start;
        agnus.ddfstop = stop;
        agnus.bplcon0 = 0x1000;
        agnus.dmacon = 0x0300;
        agnus.hpos = length - 1;
        let mut actual = Vec::new();
        let write = |agnus: &mut Agnus| {
            if scenario != 6 {
                if reg == 0 {
                    agnus.write_ddfstrt(value);
                } else {
                    agnus.write_ddfstop(value);
                }
            }
        };
        for line in 0..2 {
            for h in 0..length {
                agnus.tick_cck_with_variant_timing(
                    length,
                    312,
                    SpriteDmaVerticalTiming::fixed(25),
                    true,
                );
                assert_eq!(agnus.hpos, h);
                if let Some(request) = agnus.begin_dma_cck() {
                    agnus.sample_bitplane_dma_address(request);
                }
                let _ = agnus.claim_dma_service();
                let writing = line == 0 && h == at;
                if writing && phase == 0 {
                    write(&mut agnus);
                }
                agnus.generate_display_dma_request(true, false, length);
                if let Some(request) = agnus.dma_pipeline().reservation() {
                    let DisplayDmaChannel::Bitplane(plane) = request.channel else {
                        return Err("unexpected sprite request".into());
                    };
                    actual.push((line, h, u16::from(plane), u16::from(request.add_modulo)));
                }
                if writing && phase == 1 {
                    write(&mut agnus);
                }
            }
        }
        let wanted = expected.remove(&id).unwrap_or_default();
        requests += actual.len();
        cases += 1;
        if scenario == 6 {
            controls += 1;
        }
        if actual != wanted {
            mismatches += 1;
            if scenario == 6 {
                control_mismatches += 1;
            }
            let first = actual
                .iter()
                .zip(&wanted)
                .position(|(a, b)| a != b)
                .unwrap_or(actual.len().min(wanted.len()));
            println!(
                "case={id} chip={chip} phase={phase} reg={reg} write_h={at} value={value} expected={:?} actual={:?}",
                wanted.get(first),
                actual.get(first)
            );
        }
    }
    println!(
        "cases={cases} requests={requests} mismatches={mismatches} controls={controls} control_mismatches={control_mismatches}"
    );
    if cases != 1128 || controls != 24 || !expected.is_empty() || requests == 0 {
        return Err("incomplete native probe coverage".into());
    }
    if mismatches != 0 {
        return Err("native DDF register requests differ from reference".into());
    }
    Ok(())
}
