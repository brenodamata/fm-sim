# Implementation plan: complete M0, then prove the football

Status: proposed implementation sequence, ready to execute.
Repository baseline: 4947d470da0907600bb44d77c44ede51d95ab929 (main).
Scope of this commit: planning only. No implementation or gate is marked complete by this document.

## 1. Outcome and source of truth

Continue the existing Rust project. Complete and verify M0, execute the isolated M0.5 competition-format spike, then implement M1a and calibrate the football before introducing tactical controls. Continue through M5 only in the dependency order below.

The game specification governs intended behavior. The build brief governs implementation and ordering. Amendments 1 and 2, followed by the accepted review corrections, supersede conflicting passages. The source documents were supplied in the project conversation; they are not currently versioned in this repository. Bring their exact approved text into docs/spec/ in the first implementation PR, with an index recording precedence. Do not reconstruct missing source text or treat this plan as a replacement specification.

Accepted corrections incorporated here:

- Minimal player inflow and retirement belong at M3, before twenty-season validation. Academy facilities, intake presentations, and youth-coach opinions remain later work.
- Fatigue and recovery belong at M2. Injuries remain tier 3.
- Development windows differ by attribute group. Goalkeeping follows the technical curve initially.
- Style balance uses win-equivalent share, including half a win for a draw.
- Metrics have milestone applicability and explicit observation units. There is no universal minimum of 100 observations.
- Calibration uses pooled estimates; dispersion is diagnostic until its baseline is validated.
- Drift uses absolute change, practical slope limits, and intermediate-excursion checks. A nonsignificant slope is not evidence of stability.
- Four gate states are pass, fail, insufficient, and n/a. Investigation is a diagnostic flag, not a fifth state.
- Aggregate score distributions establish calibration, not the mechanism that produced the scores.
- No league simulation, twenty-season health metric, or knowledge-filter gate can be declared passed merely because M0 generates a world.

Some numerical definitions remain implementation prerequisites for later gates. Section 6 makes them explicit so unfinished definitions cannot silently become green checks.

## 2. What exists at the baseline

These are source-inspection findings, not a claim that the Rust suite has passed in the review environment.

| Area | Existing implementation | Action |
|---|---|---|
| Workspace | sim-core and sim-harness, Cargo.lock committed | Preserve |
| Domain | 25 attributes, seven positions, hidden traits, players, clubs, divisions, integer day offsets | Extend incrementally |
| Randomness | ChaCha8, fixed-key SipHash-1-3, domain-separated SeedPath | Preserve and strengthen regression coverage |
| Generation | Procedural worlds and dataset-based squads | Repair imported-age behavior and validate inputs |
| Imports | Header-mapped CSV parsing, optional rosters, separate reputation and squad strength | Preserve the contract; retain missing identity fields in the world |
| Harness | Sequential world generation, parameter overrides, CSV metrics, JSON manifests | Add reproducible run bundles, populations, gate evaluation, parallelism |
| Inspection | HTML inspector and interactive development browser | Keep as explicitly unfiltered diagnostic tools |
| Python ingestion | Scraper plus 20 parser regression functions | Keep and run using its actual test entry point |
| Later systems | No competition format model, match engine, time advancement, development, fatigue, or finance | Build in milestone order |

Validation performed during review: all 20 Python parser tests passed using python scripts/tests/test_parser.py. Cargo was not installed in that environment, so Rust execution remains unverified. No GitHub Actions runs existed at review time. The README's test count is not verification evidence.

### Confirmed implementation issues

1. Imported ages are applied after generation. In generate_from_dataset, generate_player draws a random age and uses it for maturity and hidden potential; only afterward is birth_year overwritten. Resolve identity and age before generating age-dependent values.
2. Run outputs overwrite earlier runs. The directory uses only label and tuning hash, so changing seeds or datasets reuses the path. Existing output must never be silently replaced.
3. Manifests omit effective tuning values. A hash and list of keys cannot reconstruct a parameter override.
4. World fingerprints omit temperament and division structure. They cannot detect all modeled-state changes.
5. Imported division IDs can violate direct-index lookup. The importer accepts arbitrary u8 division IDs, while World::division indexes a dense vector by that ID. Define and enforce a mapping before competition code relies on it.
6. Player nationality exists in PlayerData but is absent from Player. Preserve it before registration or foreign-player rules require it.

Additional validation gaps: tuning overrides accept unknown keys; float inputs are not checked for finiteness or sensible bounds; zero worlds can produce an empty successful run. Address these at the input boundary.

## 3. Architecture

### 3.1 Keep the simulation synchronous and separate from adapters

| Component | Owns | Must not own |
|---|---|---|
| sim-core | Domain state, generation, causal seed derivation, deterministic transitions, typed events | Filesystem access, wall clock, network, async runtime, presentation |
| sim-harness input adapter | CLI, runtime config loading, CSV loading, validation errors, experiment selection | Football or economic rules |
| sim-harness experiment runner | Seed scheduling, independent replications, collecting observations | Hidden modifications to simulation state |
| sim-harness metric evaluator | Metric definitions, aggregation, applicability, uncertainty, gate status | Engine behavior or automatic retuning |
| sim-harness output adapter | CSV now, JSON manifests/reports, later Parquet, atomic run publication | Randomness that affects outcomes |
| Diagnostic renderers | Human inspection of true generated values and later event traces | Player-facing knowledge guarantees |
| Later application service, M10 | Observation-based DTO construction and commands | Unfiltered truth endpoints |
| Later persistence/API/UI adapters | SQLite, localhost API, SSE, user interface | Duplicated simulation logic |

Keep two crates for M0. Split the harness into modules such as cli, input, population, runner, metrics, report, and emit as those responsibilities are implemented. Do not introduce a crate for each module.

The core keeps its existing RNG/hasher dependency budget. Use serde and a maintained TOML/JSON implementation in the harness, translating into validated typed core inputs. Incrementally replace the hand-written tuning parser without pulling transport dependencies into sim-core. The existing pure CSV parser can remain until a concrete format requirement justifies moving it; no broad parser rewrite is required for this milestone.

Proposed core growth:

- determinism: canonical seed and state encoding, domain tags, versioning.
- domain: identities, dates, players, clubs, competitions as introduced.
- gen: resolve identity, generate attributes and hidden values, construct a world.
- competition: isolated format definitions and validators from M0.5.
- match_engine: resumable state and contested events from M1a.
- calendar and selection: scheduled advancement, fatigue, Squad AI from M2.
- development and knowledge evidence: lifecycle updates and observation counters from M3.
- finance and transfer: obligations, deals, Economic AI from M4.

### 3.2 Identity, time, and randomness contracts

Internal IDs must resolve safely after imports, transfers, and retirement. At M0, normalize imported division IDs to dense internal IDs while retaining external identity. Preserve existing player/club IDs where possible. At M3, retirement must not shift every later PlayerId: keep retired records or introduce stable lookup indirection. Never compact vectors without an explicit identity migration.

A competition season owns its own identity and date window. There is no global season reset. An advance_season convenience function must specify a competition season or target date rather than reinterpret every region's year simultaneously.

Continue entity-scoped RNG derivation. Resolve imported age before generation, and avoid consuming an unnecessary random-age draw for a player whose age is known. Use explicit subdomains or tagged subpaths where necessary to keep unrelated generation operations independent.

Before introducing commands, define canonical event identity, decision identity, and relevant causal-state encoding. Two commands with identical semantic content must serialize identically. Unrelated UI actions must not change outcome seeds. A repeated step must not restart a random stream at the same position: match state carries a stable step identity, or an equivalent replayable cursor.

Encode hashed primitives with documented widths and byte order; length-prefix variable strings and collections; encode optional values distinctly from valid zero values. Never hash Debug output. Record ruleset and encoding versions. Intentional changes to generated outcomes require a version change and an explanation, not a golden-value refresh alone.

Clarify the existing documentation: RandomState-backed hash maps have randomized keys; DefaultHasher::new is not itself accurately described as randomly seeded per process. The reason to avoid default hash algorithms for replay is that their implementation is not a stable contract. Explicit SipHash remains the chosen implementation.

### 3.3 Knowledge containment

The engine computes truth and may produce observation evidence. A manager-owned knowledge store records evidence counts and estimates; application DTO construction later applies the only player-facing filter.

The current inspectors deliberately expose truth. Keep that explicit in their title and documentation. They are developer tools, not a shortcut to the M10 user interface. Do not spend this phase expanding their visual design.

## 4. M0 implementation sequence

Each item below is intended to be a small, reviewable PR. Combine adjacent items only when doing so simplifies a dependency; do not combine the match engine with foundation fixes.

### PR 1: establish a verified baseline and version the requirements

Work:

- Import the exact approved source documents and precedence index into docs/spec/.
- Select an available supported Rust toolchain, validate it against the workspace, and pin it in rust-toolchain.toml.
- Add CI running cargo test --locked --workspace and python scripts/tests/test_parser.py.
- Establish formatting checks; separate broad formatting changes from behavioral patches.
- Add a minimal ignore policy for target/, runs/, Python caches, external datasets, and platform metadata. Remove accidentally tracked platform metadata.
- Audit fixtures against the repository's no-real-player-data rule; replace real identity examples with fictional equivalents without changing parser structure.
- Update README commands and status to describe the actual verified baseline.

Acceptance:

- A clean checkout executes the Rust tests and all 20 Python parser tests.
- CI fails when either suite fails.
- Existing failures are recorded and resolved explicitly; do not weaken assertions simply to obtain a green baseline.

### PR 2: repair generation and import invariants

Work:

- Resolve imported identity first, including age, position, nationality, and stable external identity.
- Refactor generation to accept resolved age. Unknown ages follow the documented procedural distribution.
- Preserve nationality in Player; do not silently present invented identity data as imported.
- Normalize imported division IDs and retain their external mapping.
- Reject invalid WorldSpec values at construction: nonfinite spreads, unsupported dimensions, ID overflow, and squad sizes unable to meet the declared composition guarantees.
- Validate unknown tuning keys and malformed overrides.
- Distinguish a missing optional players.csv from an unreadable file; permission or decoding errors must not silently cause procedural fallback.

Targeted tests:

- Known age reaches the maturity/potential calculation, rather than merely changing displayed birth year.
- Same imported identity, inputs, and seed reproduce; unknown age reproduces too.
- Import order and insertion behavior match the documented stable-identity contract.
- Sparse external division IDs resolve correctly.
- Nationality survives import-to-world construction.
- Invalid tuning and unreadable optional inputs produce actionable errors.

Do not require every random teenager to be weaker than every veteran. Test the age-dependent generation contract directly and assess distributions separately.

Acceptance: relevant regression tests pass, full Rust suite passes, Python suite passes if parser behavior changes, and any replay change has an explicit version bump.

### PR 3: make replay evidence complete

Work:

- Define canonical encoding for seed derivation, dataset identity, and complete World fingerprints.
- Include temperament, divisions, collection structure, optional values, and every modeled field that influences future outcomes.
- Distinguish a full-state fingerprint from a generation-input hash; presentation-only exclusions belong to the latter and must be documented.
- Add frozen expected values with a documented process for intentional updates.
- Add an integration test that launches the harness in two separate processes and compares fingerprints for identical inputs.
- Record the same-platform floating-point limitation, including generation's transcendental operations.

Acceptance:

- Mutating each relevant field changes its state fingerprint.
- Identical invocations in separate processes produce identical fingerprints.
- A derivation change cannot be hidden by merely re-running a same-process equality test.
- Golden values are confirmed across separate invocations before being committed.

### PR 4: preserve reconstructable experiment output

Work:

- Read config/tuning.toml at runtime, with an explicit --config option and documented default.
- Serialize manifests using a JSON library, including effective typed tuning values.
- Record full commit SHA, dirty-tree indicator, ruleset/encoding version, lockfile hash, build target/toolchain, dataset identity, explicit seed list, population definition/version, and experiment settings.
- Separate deterministic experiment identity from invocation identity. Experiment identity includes all effective inputs; invocation identity prevents repeated runs from overwriting evidence.
- Write into a temporary directory and publish the completed bundle only on success. Refuse accidental replacement of an existing bundle.
- Keep CSV as the immediate metric format. Add a schema version and clear units; introduce Parquet before large season/event outputs make CSV impractical.
- Keep imported real datasets outside the repository. Record a content hash and how to supply the matching source; a hash alone does not recover the dataset.

Acceptance:

- Identical experiments produce identical simulation data, excluding documented invocation metadata.
- Changing only seeds or datasets never overwrites another run.
- A run's effective settings can be reconstructed from its bundle.
- Interrupted writes do not appear as completed experiments.
- JSON remains valid for arbitrary permitted labels and configuration strings.

### PR 5: implement population, metric, and gate contracts

Work:

- Add named Population A and B presets: 200 and 40 worlds respectively, one 20-club league for the tier-1 populations.
- Version the mapping from A0001–A0200 and B0001–B0040 to explicit numeric seeds; record both names and resolved values.
- Keep independent exploration seeds and custom world-generation settings available.
- Add a metric registry with ID, version, earliest milestone, observation unit, sampling unit, aggregation, units, minimum evidence, tolerance, and uncertainty method.
- Report pass/fail/insufficient/n/a with observed sample counts and a reason. Applicable missing metrics fail through insufficient status.
- Add per-metric diagnostic flags. Until calibrated, dispersion flags are advisory and cannot be described as proven instability.
- Add CLI mode selection, with gate mode exiting nonzero for fail or insufficient. Exploration can emit measurements without claiming acceptance.
- Define M0 generation checks separately from M1a match and M5 health checks.

M0 can generate Population A/B starting worlds and measure their initial state. It cannot simulate their season horizons yet. Report later metrics as n/a, not passed, and make this explicit in the run summary.

Acceptance:

- Synthetic metric observations exercise every status and aggregation path.
- A regression test proves that empty denominators cannot produce pass.
- World-level metrics count independent worlds, not players or repeated seasons.
- The expected metric registry is checked so accidentally omitting a metric cannot make a gate pass.

### PR 6: parallelize independent seeds and close M0

Work:

- Add rayon to the harness; parallelize entire independent world jobs.
- Sort emitted observations by stable keys after collection.
- Preserve sequential behavior within each world where event ordering is causal.
- Add a controlled worker-count option and benchmark generation cost.
- Expose player-weighted initial age distribution and player count, plus per-club squad metrics. Use the specified 16–21, 22–30, and 31+ age bins.
- Document commands to reproduce the M0 acceptance run.

Acceptance:

- One-worker and multi-worker runs produce identical ordered simulation outputs.
- Generation populations execute with valid IDs, bounded attributes, required position coverage, and recorded inputs.
- New gate reports distinguish completed M0 checks from future n/a checks.
- CI verifies replay and input invariants; it does not claim to validate twenty seasons.
- Record runtime and memory baselines. Set future CI budgets from measurements rather than guessing them now.

## 5. Following milestones

### M0.5: isolated competition-format spike

Purpose: prove the format model before integrating competition progression.

Inputs must include a named ruleset edition and official source references. Verify the intended Copa do Brasil edition before encoding its participants, entry rounds, seeding, leg structure, and tie rules; do not assume one year's format applies forever. Use fictional club identities in committed examples.

Represent phases, participants, entry sources, draw constraints, calendar windows, tie rules, and qualification outputs as validated data. Test the Copa do Brasil staggered-entry case and a small synthetic cross-competition drop-down example. The latter tests expressiveness, not a full continental simulation.

Validation should reject impossible participant counts, dangling sources, invalid phase dependencies, and duplicate qualification. Future-round fixtures should reference unresolved winner/qualification slots rather than pretending participants are known before prior rounds finish.

Deliverable: a command prints deterministic fixture slots for supplied fictional entrants, plus format tests. No integration with league economics, registration enforcement, or a match engine.

Exit: the hardest required structure is expressible and invalid definitions fail clearly. Stop the spike there.

### M1a: football without tactical inputs

Build a resumable MatchState, typed commands/events, and synchronous step function. State includes clock, score, possession, zone, participants, stable step identity, and completion status. Events carry time, named actors, pitch third, and an optional width channel from day one.

Implement possession transitions, contested checks, chance creation, shooting, and goalkeeping. Goals arise from those transitions. Do not sample a scoreline first and manufacture explanatory events afterward.

Keep event rendering and metric aggregation outside the engine. Define how stoppage time maps into the goal-minute buckets. Keep the initial football baseline free of style and role controls.

Population A needs 380 matches per world before M2 exists. Use a harness-only double round-robin pairing driver over fixed squads to collect M1a observations. This is a calibration experiment, not an early implementation of season economics or the overlapping competition calendar.

Measure all amended score buckets, home/draw/away shares, rating-gap response, goal timing, zone transitions, and causal event consistency. Define chance quality independently of the shooter's finishing for the conversion test. Use paired interventions that change finishing with other inputs fixed; correlation alone is not sufficient evidence of a causal lever.

Exit: all applicable Population A calibration gates pass; goal/event accounting and resumed-vs-uninterrupted replay pass; a human reviews representative event traces. No M1b until this gate passes.

### M1b: small tactical surface

Add two or three styles and a small role set. Implement role weights and participation through the axis model; location supplies the conditions for pressing and transitions. Do not introduce a style-vs-style bonus table.

Perturb one axis at a time over paired seeds and controlled squads. Predeclare the relevant observable: territory, progression, turnovers, chance creation, or scoring. A lever need not change win rate alone to demonstrate an effect.

Exit: axes have measurable, interpretable effects without breaking the calibrated football baseline. Full six-style win-equivalent bounds become binding at M6, not here.

### M2: overlapping competitions, fatigue, and Squad AI

Integrate one league and one cup, ordered fixture/day advancement, fatigue accumulation, recovery, and lineup selection. Define deterministic ordering for simultaneous events. Keep injuries out.

Implement the accepted 42-day congestion experiment with 12 fixtures and alternating 3/4-day gaps. Pin exact fixture dates, opponents, home/away assignments, squad size, starting fatigue, and replacement rules.

Compare rested-ability strongest XI with the specified fatigue-aware rotation policy, using paired world seeds and identical starting states. Define a fallback if the minimum-rest constraint makes a legal XI impossible. For the experiment, score fixture results consistently as 3/1/0; separately report league points and cup progression in the actual season model.

Exit: rotation improves the specified experiment by at least 0.15 points per match and exceeds twice the estimated standard error, with world-clustered uncertainty. Fixture order and intervention replay remain deterministic.

### M3: self-sustaining players, development, knowledge

Add group-specific growth/decline, finite coaching capacity, continuous minimal inflow, and age-driven retirement. Initial indicative windows are physical 16–24, technical 16–29, mental 16–34; goalkeeping follows technical. Potential remains a soft attractor.

Inflow uses a fixed talent curve and reputation-based allocation. Avoid a hard corrective mechanism that deletes or inserts players solely to force population metrics green. Tune rates and retirement behavior, then measure what emerges.

Retired player IDs remain valid in history; retired players leave active squad/count denominators. Define counts for active players, free agents, and later loans without double-counting ownership and playing location.

Add observation evidence and manager-owned counters with deterministic estimate noise. Keep full user-facing DTO enforcement at M10. At tier 1, measure the single-squad development model; do not claim a reserve/U20/loan ladder has been validated.

Exit: twenty-year lifecycle-only experiments satisfy approved age/count stability definitions, and speed-weighted peak age precedes reading-weighted peak age by at least four years. Label these as lifecycle validation, not an M5 economy pass.

### M4: contracts, transfers, cash waterfall, Economic AI

Use integer monetary units, dated obligations, and explicit outstanding balances. A missed payment remains unpaid; negative cash must not function as unlimited borrowing.

Model wages, valuation, transfer acceptance, installment obligations, and lender refusal. Economic AI acts under the same budget and information constraints defined for its decisions. It must not read unavailable player knowledge through a convenience truth accessor.

Implement the minimal operational consequences needed for waterfall steps 4–8, including forced-sale intervention. Full board personality/reputation remains later work. This resolves the build brief's overlap between a tier-1 cash waterfall and tier-3 board/debt texture.

Use adverse fixtures to verify each waterfall transition, payment priority, arrears, and termination. Population prevalence is then a balance measurement; ordinary worlds need not experience every failure individually.

Exit: obligations reconcile, insolvency has actual consequences, transfers preserve identity/ownership and cash accounting, and the integrated world can run for twenty seasons.

### M5: integrated tier-1 tuning gate

Run the full frozen Population B: 40 independent worlds, 20 seasons each. Evaluate all applicable world-health metrics with approved units and aggregation; loans remain n/a until M8 and the full style balance experiment remains M6 work.

Track annual trajectories, not only final snapshots. Export debt/revenue, player-count, age, wage inequality, sales, squad sizes, title concentration, and interventions. Retain failed-run manifests and traces.

Tune on exploration seeds and use separate confirmation seeds to check that fitting the frozen gate has not merely overfit those worlds. Never change a tolerance only because the current implementation misses it; record the reason and rerun previous experiments when an approved definition changes.

Exit: all applicable gates pass with sufficient evidence. Human inspection still judges whether decisions are legible and interesting. Headless gate completion is not a claim that the eventual interface or full career experience has passed playtesting.

Only after M5: M6 full tactics/Match AI; M7 real calendar and registration; M8 continental competitions, European market, and loans; M9 tier-2 tuning; M10 persistence, filtered API, frontend; M11 live interaction; M12 remaining career/academy/injury depth. Schedule reserve/U20 competition support explicitly before the M9 ladder check.

## 6. Statistical contracts to settle before their gates

These are bounded design decisions, not reasons to delay M0.

| Decision | Resolve before | Required definition |
|---|---|---|
| Minimum evidence | First gate implementation | Metric-specific independent unit and minimum count; world metrics use the 40-world population |
| Favourite rating gap | M1a | Rating scale, lineup aggregation, home advantage handling, and adequate samples per band |
| Goal timing | M1a | Stoppage-time allocation and whether a final-bucket tie counts as highest |
| Dispersion | Before making it blocking | Repeated fixed-world simulations separate sampling variance from between-world composition differences |
| Rotation uncertainty | M2 | Paired comparison, replication count, world clustering, fixed selection tie-breaking |
| Lifecycle stability | M3 | Practical slope equivalence margin, confidence interval method, and maximum intermediate excursion |
| Peak age | M3 | Profile weights, career score, tie/plateau handling, and retirement censoring |
| World-health aggregation | M5 | Per-world vs pooled rules, denominators, annual vs cumulative sanctions, and zero-revenue treatment |
| Style balance | M6 | Frozen formation/reference squad, symmetric schedules, own-style mirrors, and default roles |
| Loan liquidity | M8 | Eligible seeking prospects, placement horizon, observation unit, and minimum attempts |

Population A's 76,000-match home-win SE is approximately 0.18 percentage points under the simple independent-match approximation; a ±1-point band is approximately ±5.5 SE. Shared teams and world differences mean that approximation is not a complete dispersion model.

Preserve the amended six-style experiment: 36 ordered pairings, 2,000 matches per pairing, balanced home/away, identical reference squads, and default roles. Evaluate win-equivalent share with aggregate best/worst bounds of 58%/42% and individual pairing bounds of 30–70%. This measures balance on that reference squad; additional squad profiles are exploration needed to understand suitability interactions.

Do not interpret absence of statistical significance as proof of no drift. For stability, the confidence interval must fit inside an agreed practically acceptable slope range. Add intermediate-excursion limits because a symmetric collapse and recovery can have zero slope.

An investigate flag is advisory while the dispersion model is unvalidated. If later promoted to a blocking rule, map the evaluated violation to fail and version the metric definition.

## 7. Verification and delivery discipline

- Behavioral changes to seeds, generation constants, or parsers run the relevant regression suite before success is reported.
- Gate corrections test the failure mode they prevent: missing observations, wrong denominator, overwritten output, incomplete fingerprints, or worker-count drift.
- Use component fixtures for accounting and state-machine invariants; use population experiments for distributional claims.
- Record full experiments separately from fast CI subsets. A smoke subset cannot claim the full M1a or M5 gate passed. If the agreed every-commit full-regression policy is impractical after measuring runtime, change that policy explicitly rather than silently shrinking its population.
- Keep each PR's validation evidence, command, commit, manifest location, limitations, and replay-version changes reviewable.
- Re-run separate-process replay when seed derivation or state encoding changes.
- Keep scope out of M0: no database, localhost API, player-facing UI, full real-world competition integration, academy, or economic simulation.

## 8. Immediate next action

Execute PR 1, then PR 2. The first decision point is a green, reproducible baseline and corrected age-dependent generation. Proceed through PRs 3–6 to close M0, then begin M0.5.

The existing code is the foundation. The next work makes its measurements trustworthy enough to support the simulation that follows.

