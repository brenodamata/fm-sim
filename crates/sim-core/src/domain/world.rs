//! The world: everything durable, held in memory by the engine.
//!
//! Storage owns the durable copy (ADR-004) and the engine holds nothing between
//! calls. This type is what a snapshot deserialises into and what a delta is
//! computed against.

use super::club::{Club, Division, DivisionId};
use super::player::{ClubId, Player, PlayerId};
use crate::determinism::{WorldSeed, RULESET_VERSION};

/// Dates are integer day offsets from the save epoch, never timestamps.
/// No timezones, no DST, and calendar arithmetic stays trivial and
/// deterministic — which matters because Brazilian and European seasons
/// overlap and neither aligns to a calendar year.
pub type Day = i32;

#[derive(Debug, Clone, PartialEq)]
pub struct World {
    pub seed: WorldSeed,
    pub ruleset_version: u32,
    /// Day offset from epoch.
    pub day: Day,
    /// Year the save starts in, for age arithmetic.
    pub epoch_year: i32,
    /// Hash of the dataset this world was built from, when one was used.
    ///
    /// Seed alone does not identify a world once reputation and squad strength
    /// are editable before kickoff. `None` means a fully generated world.
    pub dataset_hash: Option<u64>,
    /// Indexed by `PlayerId.0`. Dense, stable order.
    pub players: Vec<Player>,
    /// Indexed by `ClubId.0`. Dense, stable order.
    pub clubs: Vec<Club>,
    pub divisions: Vec<Division>,
}

impl World {
    pub fn player(&self, id: PlayerId) -> &Player {
        &self.players[id.0 as usize]
    }

    pub fn player_mut(&mut self, id: PlayerId) -> &mut Player {
        &mut self.players[id.0 as usize]
    }

    pub fn club(&self, id: ClubId) -> &Club {
        &self.clubs[id.0 as usize]
    }

    pub fn club_mut(&mut self, id: ClubId) -> &mut Club {
        &mut self.clubs[id.0 as usize]
    }

    pub fn division(&self, id: DivisionId) -> &Division {
        &self.divisions[id.0 as usize]
    }

    pub fn current_year(&self) -> i32 {
        self.epoch_year + self.day / 365
    }

    pub fn squad_of(&self, id: ClubId) -> impl Iterator<Item = &Player> {
        self.club(id).squad.iter().map(move |&p| self.player(p))
    }

    /// A structural fingerprint of the whole world.
    ///
    /// Used by tests and by the harness manifest to assert that a given seed
    /// still produces the same world. If this changes without a
    /// [`RULESET_VERSION`] bump, replay is broken.
    pub fn fingerprint(&self) -> u64 {
        use siphasher::sip::SipHasher13;
        use std::hash::Hasher;

        let mut h = SipHasher13::new_with_keys(0x1111_2222_3333_4444, 0x5555_6666_7777_8888);
        h.write_u32(self.ruleset_version);
        h.write_u64(self.seed.raw());
        h.write_u64(self.dataset_hash.unwrap_or(0));
        h.write_i32(self.day);
        h.write_i32(self.epoch_year);
        // Dense vectors, iterated in index order — no map iteration anywhere.
        for p in &self.players {
            h.write_u32(p.id.0);
            h.write(p.name.as_bytes());
            h.write_i32(p.birth_year);
            h.write_u8(p.position as u8);
            for (_, v) in p.attributes.iter() {
                h.write_u8(v);
            }
            h.write_u8(p.hidden.potential);
            h.write_u8(p.hidden.longevity);
            h.write_u8(p.hidden.proneness);
        }
        for c in &self.clubs {
            h.write_u32(c.id.0);
            h.write(c.name.as_bytes());
            h.write_u8(c.division.0);
            h.write_u8(c.reputation);
            for p in &c.squad {
                h.write_u32(p.0);
            }
        }
        h.finish()
    }

    pub fn is_current_ruleset(&self) -> bool {
        self.ruleset_version == RULESET_VERSION
    }
}
