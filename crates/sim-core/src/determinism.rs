//! The determinism contract (ADR-005).
//!
//! Every stochastic draw in the simulation traces back to a seed derived here.
//! Three rules make replay work, and all three are easy to violate by accident:
//!
//! 1. **Never use `StdRng`.** Its algorithm is explicitly allowed to change
//!    between `rand` releases, which would silently break replay on a dependency
//!    bump. We use ChaCha8 via `rand_chacha`, whose output is specified.
//!
//! 2. **Never use `std::collections::hash_map::DefaultHasher` for derivation.**
//!    It is randomly seeded *per process*, so the same input hashes differently
//!    between two runs of the same binary. We use SipHash-1-3 with a fixed key.
//!
//! 3. **Seed derivation is causal, not positional.** A seed is a pure function of
//!    (world seed, domain, identity, decision). Reloading a save and repeating a
//!    decision reproduces the result; making a *different* decision may not. The
//!    universe does not reroll because the player loaded a save.
//!
//! Two further rules are not enforceable by this module and must be held by
//! callers:
//!
//! - No decision may depend on `HashMap`/`HashSet` iteration order. Use ordered
//!   collections or sorted iteration in any code path that draws.
//! - Any change to what gets hashed changes every derived seed. That is correct,
//!   but it invalidates saved games, so bump [`RULESET_VERSION`].

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;
use siphasher::sip::SipHasher13;
use std::hash::Hasher;

/// Bumped whenever seed derivation or simulation rules change in a way that
/// makes existing saves non-reproducible. Stored in every save and every
/// harness run manifest.
pub const RULESET_VERSION: u32 = 1;

/// Fixed SipHash key. Arbitrary but frozen: changing it reseeds the universe.
const SIP_KEY: (u64, u64) = (0x5f3a_9c17_b2e4_d081, 0xa17c_44e9_03bd_5f26);

/// The root seed of a save. Everything else derives from it.
///
/// Crosses the API boundary as a *string*, never a number: JavaScript cannot
/// exactly represent every `u64`, so a raw numeric seed would silently round.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WorldSeed(u64);

impl WorldSeed {
    pub fn new(raw: u64) -> Self {
        WorldSeed(raw)
    }

    pub fn raw(self) -> u64 {
        self.0
    }

    /// Canonical string form for storage and for crossing into TypeScript.
    pub fn to_hex(self) -> String {
        format!("{:016x}", self.0)
    }

    pub fn from_hex(s: &str) -> Option<Self> {
        u64::from_str_radix(s.trim(), 16).ok().map(WorldSeed)
    }
}

/// A derived seed. Opaque: the only thing you can do is turn it into an RNG.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Seed(u64);

impl Seed {
    pub fn raw(self) -> u64 {
        self.0
    }

    /// A ChaCha8 generator. Deterministic and portable across platforms and
    /// `rand` versions, unlike `StdRng`.
    pub fn rng(self) -> ChaCha8Rng {
        ChaCha8Rng::seed_from_u64(self.0)
    }
}

/// Domain separation. Two different subsystems deriving from the same identity
/// must not collide, so every derivation starts by declaring what it is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Domain {
    WorldGen,
    ClubGen,
    PlayerGen,
    Match,
    Development,
    TransferOffer,
    Injury,
    YouthIntake,
    /// Estimates the player sees. Seeded from the *observer and observation
    /// count*, never from wall clock or load time, so a reload cannot be used to
    /// re-roll a scout report until it flatters the player.
    Estimate,
}

impl Domain {
    fn tag(self) -> &'static str {
        match self {
            Domain::WorldGen => "world",
            Domain::ClubGen => "club",
            Domain::PlayerGen => "player",
            Domain::Match => "match",
            Domain::Development => "devel",
            Domain::TransferOffer => "offer",
            Domain::Injury => "injur",
            Domain::YouthIntake => "youth",
            Domain::Estimate => "estim",
        }
    }
}

/// Builds a derived seed from an explicitly ordered set of components.
///
/// Field order is enforced by construction rather than by convention: each
/// component contributes its own tag as well as its value, so reordering two
/// calls produces a different seed and is caught by the derivation tests.
///
/// ```
/// # use sim_core::determinism::{WorldSeed, SeedPath, Domain};
/// let world = WorldSeed::new(42);
/// let a = SeedPath::new(world, Domain::PlayerGen).with_u64("club", 3).with_u64("slot", 7).finish();
/// let b = SeedPath::new(world, Domain::PlayerGen).with_u64("club", 3).with_u64("slot", 7).finish();
/// assert_eq!(a, b);
/// ```
pub struct SeedPath {
    hasher: SipHasher13,
}

impl SeedPath {
    pub fn new(world: WorldSeed, domain: Domain) -> Self {
        let mut hasher = SipHasher13::new_with_keys(SIP_KEY.0, SIP_KEY.1);
        hasher.write_u32(RULESET_VERSION);
        hasher.write_u64(world.raw());
        hasher.write(domain.tag().as_bytes());
        hasher.write_u8(0xff); // terminator: prevents tag/value ambiguity
        SeedPath { hasher }
    }

    pub fn with_u64(mut self, tag: &'static str, value: u64) -> Self {
        self.hasher.write(tag.as_bytes());
        self.hasher.write_u8(0xfe);
        self.hasher.write_u64(value);
        self
    }

    pub fn with_u32(self, tag: &'static str, value: u32) -> Self {
        self.with_u64(tag, value as u64)
    }

    pub fn with_usize(self, tag: &'static str, value: usize) -> Self {
        self.with_u64(tag, value as u64)
    }

    pub fn with_str(mut self, tag: &'static str, value: &str) -> Self {
        self.hasher.write(tag.as_bytes());
        self.hasher.write_u8(0xfe);
        self.hasher.write(value.as_bytes());
        self.hasher.write_u8(0xfd);
        self
    }

    pub fn finish(self) -> Seed {
        Seed(self.hasher.finish())
    }

    /// Convenience: derive and immediately open an RNG.
    pub fn rng(self) -> ChaCha8Rng {
        self.finish().rng()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::Rng;

    #[test]
    fn derivation_is_stable_across_processes() {
        // Hard-coded expected values. If these ever change without a deliberate
        // RULESET_VERSION bump, replay is broken and saves are invalid.
        //
        // This test exists specifically to catch the DefaultHasher trap: a
        // randomly-seeded hasher would make these differ between runs of the
        // same binary, and the failure would otherwise surface as "sometimes my
        // save doesn't replay".
        let world = WorldSeed::new(0x0123_4567_89ab_cdef);
        let s = SeedPath::new(world, Domain::PlayerGen)
            .with_u64("club", 3)
            .with_usize("slot", 7)
            .finish();
        assert_eq!(s.raw(), 0x6fed_08a2_baf0_e853, "seed derivation drifted");
    }

    #[test]
    fn domain_separation_holds() {
        let world = WorldSeed::new(99);
        let a = SeedPath::new(world, Domain::Match).with_u64("id", 1).finish();
        let b = SeedPath::new(world, Domain::Injury).with_u64("id", 1).finish();
        assert_ne!(a, b, "different domains must not collide");
    }

    #[test]
    fn component_order_matters() {
        let world = WorldSeed::new(7);
        let a = SeedPath::new(world, Domain::ClubGen)
            .with_u64("a", 1)
            .with_u64("b", 2)
            .finish();
        let b = SeedPath::new(world, Domain::ClubGen)
            .with_u64("b", 2)
            .with_u64("a", 1)
            .finish();
        assert_ne!(a, b, "component order must be part of the derivation");
    }

    #[test]
    fn tag_value_boundaries_are_unambiguous() {
        // Without terminators, ("ab", 1) and ("a", "b1") could hash identically.
        let world = WorldSeed::new(1);
        let a = SeedPath::new(world, Domain::WorldGen).with_str("ab", "c").finish();
        let b = SeedPath::new(world, Domain::WorldGen).with_str("a", "bc").finish();
        assert_ne!(a, b);
    }

    #[test]
    fn same_seed_yields_same_stream() {
        let world = WorldSeed::new(4242);
        let path = || SeedPath::new(world, Domain::Match).with_u64("fixture", 11);
        let mut r1 = path().rng();
        let mut r2 = path().rng();
        let a: Vec<u32> = (0..32).map(|_| r1.gen()).collect();
        let b: Vec<u32> = (0..32).map(|_| r2.gen()).collect();
        assert_eq!(a, b);
    }

    #[test]
    fn different_seeds_diverge() {
        let mut r1 = SeedPath::new(WorldSeed::new(1), Domain::Match).rng();
        let mut r2 = SeedPath::new(WorldSeed::new(2), Domain::Match).rng();
        let a: Vec<u32> = (0..32).map(|_| r1.gen()).collect();
        let b: Vec<u32> = (0..32).map(|_| r2.gen()).collect();
        assert_ne!(a, b);
    }

    #[test]
    fn world_seed_hex_roundtrips() {
        let s = WorldSeed::new(0xdead_beef_cafe_1234);
        assert_eq!(WorldSeed::from_hex(&s.to_hex()), Some(s));
    }
}
