//! World generation.
//!
//! Produces a plausible football world from a seed and a spec. No real data is
//! involved (ADR-007), which means the generator has to carry weight the import
//! path would otherwise carry: a credible talent pyramid, a realistic age
//! distribution, and clubs that differ from each other in ways that matter.
//!
//! Everything here draws from seeds derived per entity rather than from a single
//! sequential stream. That costs a little speed and buys a lot: generating club
//! 7 does not depend on having generated club 6, so the order of generation can
//! change without changing the world.

use rand::Rng;
use rand_chacha::ChaCha8Rng;

use super::names;
use crate::determinism::{Domain, SeedPath, WorldSeed, RULESET_VERSION};
use crate::domain::attributes::{clamp_f32, Attribute, Attributes, Rating};
use crate::domain::club::{Club, Division, DivisionId};
use crate::domain::player::{ClubId, Hidden, Player, PlayerId, Position, Temperament};
use crate::domain::world::World;
use crate::data::{BroadPosition, Dataset, PlayerData};
use crate::tuning::{Tuning, TuningError};

#[derive(Debug, Clone, PartialEq)]
pub struct WorldSpec {
    pub divisions: usize,
    pub clubs_per_division: usize,
    pub squad_size: usize,
    pub epoch_year: i32,
    /// Mean coarse ability of the top division's median club.
    pub top_division_strength: f32,
    /// How much weaker each successive division is.
    pub division_step: f32,
    /// Spread of club strength within a division.
    pub club_spread: f32,
    /// Spread of individual attributes around their club's level.
    pub player_spread: f32,
}

impl WorldSpec {
    pub fn from_tuning(t: &Tuning) -> Result<Self, TuningError> {
        Ok(WorldSpec {
            divisions: t.usize("worldgen.divisions")?,
            clubs_per_division: t.usize("worldgen.clubs_per_division")?,
            squad_size: t.usize("worldgen.squad_size")?,
            epoch_year: t.i32("worldgen.epoch_year")?,
            top_division_strength: t.f32("worldgen.top_division_strength")?,
            division_step: t.f32("worldgen.division_step")?,
            club_spread: t.f32("worldgen.club_spread")?,
            player_spread: t.f32("worldgen.player_spread")?,
        })
    }
}

pub fn generate(seed: WorldSeed, spec: &WorldSpec) -> World {
    let mut clubs: Vec<Club> = Vec::new();
    let mut players: Vec<Player> = Vec::new();
    let mut divisions: Vec<Division> = Vec::new();

    for d in 0..spec.divisions {
        let division_id = DivisionId(d as u8);
        let mut club_ids = Vec::with_capacity(spec.clubs_per_division);

        for c in 0..spec.clubs_per_division {
            let club_id = ClubId(clubs.len() as u32);
            let mut club_rng = SeedPath::new(seed, Domain::ClubGen)
                .with_usize("division", d)
                .with_usize("index", c)
                .rng();

            let strength = club_strength(&mut club_rng, spec, d);
            let reputation = reputation_for(strength);
            let name = names::club_name(&mut club_rng);

            let mut squad = Vec::with_capacity(spec.squad_size);
            for slot in 0..spec.squad_size {
                let player_id = PlayerId(players.len() as u32);
                let mut p_rng = SeedPath::new(seed, Domain::PlayerGen)
                    .with_u32("club", club_id.0)
                    .with_usize("slot", slot)
                    .rng();
                let position = position_for_slot(slot, spec.squad_size);
                players.push(generate_player(
                    &mut p_rng, player_id, position, strength, spec,
                ));
                squad.push(player_id);
            }

            clubs.push(Club {
                id: club_id,
                name,
                division: division_id,
                reputation,
                squad,
            });
            club_ids.push(club_id);
        }

        divisions.push(Division {
            id: division_id,
            name: division_name(d),
            clubs: club_ids,
        });
    }

    World {
        seed,
        ruleset_version: RULESET_VERSION,
        day: 0,
        epoch_year: spec.epoch_year,
        dataset_hash: None,
        players,
        clubs,
        divisions,
    }
}


/// Generate a world whose clubs come from a real dataset, with generated
/// players.
///
/// Club identity is imported; squads are not. Real rosters would go stale within
/// a window and are far harder to source than club facts — and the game is about
/// what happens next, not about recreating a specific season's squads.
///
/// Club strength derives from imported `reputation` rather than being drawn, so
/// Flamengo is strong because the dataset says so, not because the RNG felt like
/// it. Within that, players still vary by seed.
pub fn generate_from_dataset(seed: WorldSeed, spec: &WorldSpec, data: &Dataset) -> World {
    let mut clubs: Vec<Club> = Vec::new();
    let mut players: Vec<Player> = Vec::new();
    let mut divisions: Vec<Division> = Vec::new();

    let mut division_ids: Vec<u8> = data.clubs.iter().map(|c| c.division).collect();
    division_ids.sort_unstable();
    division_ids.dedup();

    for d in division_ids {
        let mut club_ids = Vec::new();
        for (index, source) in data.clubs_in_division(d).enumerate() {
            let club_id = ClubId(clubs.len() as u32);

            // Seeded from the club's stable string id, not its index: adding a
            // club to the dataset must not reshuffle everyone else's squad.
            let strength = strength_from_rating(source.squad_strength, spec);

            // Real squad if one was imported; otherwise generate one.
            let imported: Vec<&PlayerData> = data.players_of(&source.id).collect();
            let mut squad = Vec::new();

            if imported.is_empty() {
                for slot in 0..spec.squad_size {
                    let player_id = PlayerId(players.len() as u32);
                    let mut p_rng = SeedPath::new(seed, Domain::PlayerGen)
                        .with_str("club_key", &source.id)
                        .with_usize("slot", slot)
                        .rng();
                    let position = position_for_slot(slot, spec.squad_size);
                    players.push(generate_player(&mut p_rng, player_id, position, strength, spec));
                    squad.push(player_id);
                }
            } else {
                for (slot, p) in imported.iter().enumerate() {
                    let player_id = PlayerId(players.len() as u32);
                    // Seeded from the player's own name, not their squad index:
                    // a re-scrape that reorders or drops players must not change
                    // anyone else's attributes.
                    let mut p_rng = SeedPath::new(seed, Domain::PlayerGen)
                        .with_str("club_key", &source.id)
                        .with_str("player", &p.name)
                        .rng();
                    let position = specialise_position(p.broad_position, &mut p_rng);
                    let mut generated =
                        generate_player(&mut p_rng, player_id, position, strength, spec);
                    generated.name = p.name.clone();
                    if let Some(year) = p.birth_year {
                        generated.birth_year = year;
                    }
                    players.push(generated);
                    squad.push(player_id);
                    let _ = slot;
                }
            }

            clubs.push(Club {
                id: club_id,
                name: source.name.clone(),
                division: DivisionId(d),
                reputation: source.reputation,
                squad,
            });
            club_ids.push(club_id);
            let _ = index;
        }
        divisions.push(Division {
            id: DivisionId(d),
            name: division_name(d as usize),
            clubs: club_ids,
        });
    }

    World {
        seed,
        ruleset_version: RULESET_VERSION,
        day: 0,
        epoch_year: spec.epoch_year,
        dataset_hash: Some(data.content_hash()),
        players,
        clubs,
        divisions,
    }
}

/// Map an imported 1..=20 rating onto the ability scale the generator uses.
///
/// Fed by `squad_strength`, never by `reputation`: stature determines who will
/// sign for a club, not who already plays there. Deliberately compresses, since
/// squad quality in Brazil varies far less than budget does.
fn strength_from_rating(rating: u8, spec: &WorldSpec) -> f32 {
    let normalised = (rating as f32 - 10.0) / 10.0;
    spec.top_division_strength + normalised * spec.club_spread * 1.8
}

/// Turn a broad upstream position into one of the seven families.
///
/// Wikipedia records only GK/DF/MF/FW. The split ratios below are rough squad
/// composition for Brazilian football — more zagueiros than laterais, more
/// pontas than centroavantes. Drawn from the player's own seed so it is stable
/// across re-scrapes and across reloads.
fn specialise_position(broad: BroadPosition, rng: &mut ChaCha8Rng) -> Position {
    // Specific codes map directly — no draw, no guessing. Only the four broad
    // fallbacks need a seeded split, and then only for sources that record
    // nothing better.
    match broad {
        BroadPosition::Goalkeeper => Position::Goleiro,
        BroadPosition::CentreBack => Position::Zagueiro,
        BroadPosition::FullBack => Position::Lateral,
        BroadPosition::DefensiveMid => Position::Volante,
        BroadPosition::AttackingMid => Position::Meia,
        BroadPosition::Winger => Position::Ponta,
        BroadPosition::Striker => Position::Centroavante,
        // A central midfielder is genuinely either; the source does not say.
        BroadPosition::CentralMid => {
            if rng.gen_range(0.0f32..1.0) < 0.5 {
                Position::Volante
            } else {
                Position::Meia
            }
        }
        BroadPosition::Defender => {
            if rng.gen_range(0.0f32..1.0) < 0.58 {
                Position::Zagueiro
            } else {
                Position::Lateral
            }
        }
        BroadPosition::Midfielder => {
            if rng.gen_range(0.0f32..1.0) < 0.52 {
                Position::Volante
            } else {
                Position::Meia
            }
        }
        BroadPosition::Forward => {
            if rng.gen_range(0.0f32..1.0) < 0.58 {
                Position::Ponta
            } else {
                Position::Centroavante
            }
        }
    }
}

fn division_name(index: usize) -> String {
    match index {
        0 => "Série A".to_string(),
        1 => "Série B".to_string(),
        2 => "Série C".to_string(),
        n => format!("Division {}", n + 1),
    }
}

/// Club strength: division base plus a within-division spread.
///
/// The spread is deliberately skewed rather than symmetric — real divisions have
/// a couple of clubs well clear and a long tail, not a neat bell curve.
fn club_strength(rng: &mut ChaCha8Rng, spec: &WorldSpec, division: usize) -> f32 {
    let base = spec.top_division_strength - spec.division_step * division as f32;
    let roll: f32 = rng.gen_range(0.0f32..1.0);
    // Cubing skews mass toward the lower end, leaving a short strong tail.
    let skewed = roll.powf(0.6) - 0.45;
    base + skewed * spec.club_spread * 2.0
}

fn reputation_for(strength: f32) -> u8 {
    clamp_f32(strength)
}

/// Deterministic position allocation across a squad.
///
/// Walks the share table in a fixed order rather than drawing, so every squad
/// gets a sane shape: three keepers in a 26-man squad, not zero or seven.
fn position_for_slot(slot: usize, squad_size: usize) -> Position {
    let mut boundary = 0usize;
    for (i, pos) in Position::ALL.iter().enumerate() {
        let mut count = (pos.squad_share() * squad_size as f32).round() as usize;
        // Guarantee at least one of every position, and at least two keepers.
        count = count.max(if pos.is_keeper() { 2 } else { 1 });
        boundary += count;
        if slot < boundary || i == Position::ALL.len() - 1 {
            return *pos;
        }
    }
    Position::Centroavante
}

fn generate_player(
    rng: &mut ChaCha8Rng,
    id: PlayerId,
    position: Position,
    club_strength: f32,
    spec: &WorldSpec,
) -> Player {
    let name = names::player_name(rng);
    let age = draw_age(rng);
    let birth_year = spec.epoch_year - age;

    // Young players are below their eventual level; peak-age players are at it.
    let maturity = maturity_at(age);
    let personal = club_strength + gaussian(rng) * spec.player_spread;
    let current_level = personal * maturity;

    let mut attributes = Attributes::default();
    for attr in Attribute::ALL {
        let weight = position.generation_weight(attr);
        let value = if weight <= 0.0 {
            // Outfielders get floor-level keeping attributes rather than zero:
            // the values exist, they are simply never checked.
            1.0 + gaussian(rng).abs()
        } else {
            // Attributes central to the position sit near the player's level;
            // incidental ones regress toward the middle of the scale.
            let target = current_level * weight + 8.0 * (1.0 - weight);
            target + gaussian(rng) * 1.6
        };
        attributes.set(attr, clamp_f32(value));
    }

    let hidden = generate_hidden(rng, personal, maturity, age);

    Player {
        id,
        name,
        birth_year,
        position,
        attributes,
        hidden,
    }
}

/// Squad age distribution: a pyramid with a bulge at peak age and a thin tail.
fn draw_age(rng: &mut ChaCha8Rng) -> i32 {
    let roll: f32 = rng.gen_range(0.0f32..1.0);
    match roll {
        r if r < 0.16 => rng.gen_range(16..=19),
        r if r < 0.42 => rng.gen_range(20..=23),
        r if r < 0.72 => rng.gen_range(24..=27),
        r if r < 0.91 => rng.gen_range(28..=31),
        _ => rng.gen_range(32..=38),
    }
}

/// How much of a player's eventual level he has reached at a given age.
///
/// Rises steeply through the window (16–21), flattens at peak, and is *not* a
/// decline curve — decline is per attribute group and belongs to development
/// (M3), not to generation.
fn maturity_at(age: i32) -> f32 {
    match age {
        a if a <= 16 => 0.62,
        a if a <= 18 => 0.72,
        a if a <= 20 => 0.82,
        a if a <= 22 => 0.90,
        a if a <= 24 => 0.96,
        a if a <= 30 => 1.0,
        a if a <= 33 => 0.97,
        _ => 0.92,
    }
}

fn generate_hidden(
    rng: &mut ChaCha8Rng,
    personal_level: f32,
    maturity: f32,
    age: i32,
) -> Hidden {
    // Potential is a soft target, so it is allowed to sit below a veteran's
    // current level — a player who overshot his target is a normal outcome, not
    // a generation bug.
    let headroom = (1.0 - maturity) * personal_level;
    let ambition_bonus = if age <= 21 {
        gaussian(rng).abs() * 2.4
    } else {
        gaussian(rng) * 0.8
    };
    let potential = clamp_f32(personal_level * maturity + headroom + ambition_bonus);

    Hidden {
        potential,
        longevity: draw_trait(rng, 10.0, 3.6),
        temperament: Temperament {
            professionalism: draw_trait(rng, 11.0, 3.8),
            ambition: draw_trait(rng, 12.0, 3.6),
            resilience: draw_trait(rng, 10.5, 3.6),
            greed: draw_trait(rng, 10.0, 4.0),
            loyalty: draw_trait(rng, 9.5, 4.0),
        },
        proneness: draw_trait(rng, 7.5, 3.4),
    }
}

fn draw_trait(rng: &mut ChaCha8Rng, mean: f32, spread: f32) -> Rating {
    clamp_f32(mean + gaussian(rng) * spread)
}

/// Standard normal via Box–Muller.
///
/// Note for the determinism contract: this uses `ln`, `sqrt` and `cos`, which
/// are implementation-defined across platforms in their last bits. Replay is
/// therefore guaranteed on a single platform, not across them. That is an
/// accepted trade for a single-player game; making it cross-platform would mean
/// a fixed-point normal, and the cost is not worth paying yet.
fn gaussian(rng: &mut ChaCha8Rng) -> f32 {
    let u1: f32 = rng.gen_range(f32::EPSILON..1.0);
    let u2: f32 = rng.gen_range(0.0f32..1.0);
    (-2.0 * u1.ln()).sqrt() * (std::f32::consts::TAU * u2).cos()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::attributes::{RATING_MAX, RATING_MIN};

    fn spec() -> WorldSpec {
        WorldSpec {
            divisions: 2,
            clubs_per_division: 12,
            squad_size: 26,
            epoch_year: 2026,
            top_division_strength: 13.0,
            division_step: 2.0,
            club_spread: 1.6,
            player_spread: 1.5,
        }
    }

    #[test]
    fn same_seed_produces_an_identical_world() {
        let a = generate(WorldSeed::new(12345), &spec());
        let b = generate(WorldSeed::new(12345), &spec());
        assert_eq!(a.fingerprint(), b.fingerprint());
        assert_eq!(a, b);
    }

    #[test]
    fn different_seeds_produce_different_worlds() {
        let a = generate(WorldSeed::new(1), &spec());
        let b = generate(WorldSeed::new(2), &spec());
        assert_ne!(a.fingerprint(), b.fingerprint());
    }

    #[test]
    fn structure_matches_the_spec() {
        let s = spec();
        let w = generate(WorldSeed::new(7), &s);
        assert_eq!(w.divisions.len(), s.divisions);
        assert_eq!(w.clubs.len(), s.divisions * s.clubs_per_division);
        assert_eq!(w.players.len(), w.clubs.len() * s.squad_size);
        for club in &w.clubs {
            assert_eq!(club.squad_size(), s.squad_size);
        }
    }

    #[test]
    fn ids_are_dense_and_in_order() {
        let w = generate(WorldSeed::new(8), &spec());
        for (i, p) in w.players.iter().enumerate() {
            assert_eq!(p.id.0 as usize, i);
        }
        for (i, c) in w.clubs.iter().enumerate() {
            assert_eq!(c.id.0 as usize, i);
        }
    }

    #[test]
    fn every_squad_has_at_least_two_keepers() {
        let w = generate(WorldSeed::new(9), &spec());
        for club in &w.clubs {
            let keepers = w
                .squad_of(club.id)
                .filter(|p| p.position.is_keeper())
                .count();
            assert!(keepers >= 2, "{} has {keepers} keepers", club.name);
        }
    }

    #[test]
    fn every_squad_covers_every_position() {
        let w = generate(WorldSeed::new(10), &spec());
        for club in &w.clubs {
            for pos in Position::ALL {
                let n = w.squad_of(club.id).filter(|p| p.position == pos).count();
                assert!(n >= 1, "{} has no {}", club.name, pos.name());
            }
        }
    }

    #[test]
    fn attributes_stay_in_range() {
        let w = generate(WorldSeed::new(11), &spec());
        for p in &w.players {
            for (attr, v) in p.attributes.iter() {
                assert!(
                    (RATING_MIN..=RATING_MAX).contains(&v),
                    "{} {} = {v}",
                    p.name,
                    attr.name()
                );
            }
        }
    }

    #[test]
    fn outfielders_have_floor_level_keeping_attributes() {
        let w = generate(WorldSeed::new(12), &spec());
        for p in w.players.iter().filter(|p| !p.position.is_keeper()) {
            for attr in Attribute::ALL.iter().filter(|a| a.is_goalkeeping()) {
                assert!(
                    p.attributes.get(*attr) <= 5,
                    "{} has {} = {}",
                    p.name,
                    attr.name(),
                    p.attributes.get(*attr)
                );
            }
        }
    }

    #[test]
    fn higher_divisions_are_stronger_on_average() {
        let w = generate(WorldSeed::new(13), &spec());
        let mean_by_division: Vec<f32> = w
            .divisions
            .iter()
            .map(|d| {
                let vals: Vec<f32> = d
                    .clubs
                    .iter()
                    .flat_map(|&c| w.squad_of(c))
                    .map(|p| p.coarse_ability())
                    .collect();
                vals.iter().sum::<f32>() / vals.len() as f32
            })
            .collect();
        assert!(
            mean_by_division[0] > mean_by_division[1] + 0.5,
            "divisions not separated: {mean_by_division:?}"
        );
    }

    #[test]
    fn age_distribution_is_a_pyramid() {
        let w = generate(WorldSeed::new(14), &spec());
        let year = w.current_year();
        let ages: Vec<i32> = w.players.iter().map(|p| p.age(year)).collect();
        let young = ages.iter().filter(|&&a| a <= 21).count();
        let peak = ages.iter().filter(|&&a| (22..=30).contains(&a)).count();
        let old = ages.iter().filter(|&&a| a >= 33).count();
        assert!(peak > young, "peak {peak} should exceed young {young}");
        assert!(young > old, "young {young} should exceed old {old}");
        assert!(ages.iter().all(|&a| (16..=38).contains(&a)));
    }

    #[test]
    fn clubs_within_a_division_actually_differ() {
        let w = generate(WorldSeed::new(15), &spec());
        let strengths: Vec<f32> = w.divisions[0]
            .clubs
            .iter()
            .map(|&c| {
                let vals: Vec<f32> = w.squad_of(c).map(|p| p.coarse_ability()).collect();
                vals.iter().sum::<f32>() / vals.len() as f32
            })
            .collect();
        let max = strengths.iter().cloned().fold(f32::MIN, f32::max);
        let min = strengths.iter().cloned().fold(f32::MAX, f32::min);
        assert!(max - min > 1.0, "clubs too uniform: {min}..{max}");
    }

    #[test]
    fn generation_order_does_not_affect_a_player() {
        // Each player derives from its own seed path, so generating a world with
        // more clubs must not change the players of club 0.
        let mut small = spec();
        small.clubs_per_division = 4;
        let mut large = spec();
        large.clubs_per_division = 20;

        let a = generate(WorldSeed::new(16), &small);
        let b = generate(WorldSeed::new(16), &large);
        for i in 0..small.squad_size {
            assert_eq!(a.players[i].name, b.players[i].name);
            assert_eq!(a.players[i].attributes, b.players[i].attributes);
        }
    }
}
