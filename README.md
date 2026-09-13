# Football management sim — M0

Headless simulation core plus the harness that measures it. No interface, no
database, no match engine yet.

## Status

| Milestone | State |
|---|---|
| **M0** — domain types, world generator, harness runner | this code |
| M0.5 — ADR-012 spike: express Copa do Brasil, print fixtures | next |
| M1a — possession sequences, zones, scoring (no tactics) | |
| M1b — styles, roles, axis perturbation tests | |

## Layout

```
crates/sim-core/      pure simulation: no I/O, no clock, no global state
  determinism.rs      the seed contract — read this first
  domain/             attributes, players, clubs, world
  gen/                world generation and naming
  tuning.rs           constants loaded from data
crates/sim-harness/   headless runner, metrics, run manifests
config/tuning.toml    balance constants
```

## Running

```sh
cargo test                                    # 37 tests, determinism included
cargo run -p sim-harness -- --worlds 12       # generate and measure
cargo run -p sim-harness -- --help
```

Sweep a parameter without editing the config:

```sh
cargo run -p sim-harness -- --worlds 32 --set worldgen.club_spread=2.4 --label spread
```

Each run writes `club_metrics.csv`, `fingerprints.csv` and `manifest.json`
under `runs/`. (Named `club_metrics` rather than `clubs` so it never gets
confused with the dataset's `clubs.csv`, which is a completely different file.) The manifest records the ruleset version, git SHA, config hash and seed
set, so a metric that moved six months ago is still explainable.

## The determinism contract

Replay is load-bearing, and three things silently break it:

1. **`StdRng` is not reproducible across `rand` versions.** Its algorithm may
   change on a dependency bump. Use `ChaCha8Rng` via `rand_chacha`.
2. **`DefaultHasher` is randomly seeded per process.** The same input hashes
   differently between two runs of the same binary. Seed derivation uses
   SipHash-1-3 with a fixed key.
3. **Map iteration order leaks into draws.** Any code path that draws must
   iterate an ordered collection. `Attributes` is a fixed array and squads are
   `Vec` for this reason, not for speed.

Seeds are derived causally, not positionally:

```rust
let seed = SeedPath::new(world_seed, Domain::PlayerGen)
    .with_u32("club", club.0)
    .with_usize("slot", slot)
    .finish();
```

Same inputs reproduce. Different inputs may not. **Reloading a save and
repeating a decision gives the same result; making a different decision may give
a different one.** The universe does not reroll because the player loaded a
save — which is why outcome seeds must never be regenerated at load time.

`determinism::tests::derivation_is_stable_across_processes` pins a golden value
specifically to catch rule 2. It has been verified identical across separate
process invocations.

### Known limit

`gaussian()` uses `ln`, `sqrt` and `cos`, whose last bits are
implementation-defined across platforms. **Replay is guaranteed on one platform,
not across them.** Accepted trade for a single-player game; a fixed-point normal
would fix it and is not worth the cost yet.

## Data

No real club or player data is in this repository, or ever will be (ADR-007).
The generator produces fictional Brazilian-shaped worlds; a documented import
format is how real datasets arrive, and they stay outside the repo.

## Toolchain note

Developed against Rust 1.75 for container reasons. `sim-core` deliberately
depends only on `rand`, `rand_chacha` and `siphasher`. On a current toolchain,
two upgrades are worth making immediately:

- swap the hand-rolled flat-TOML parser in `tuning.rs` for `toml` + `serde`
- add `rayon` to the harness for parallel seed sweeps
