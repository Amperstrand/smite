# Handoff: spec-requirements branch

State as of the PR #1 head: seed pipeline complete and CI-pinned, IR
surface covering splice / interactive-tx / commitment families, 12
generators including stateful live-channel flows, two stateful oracles,
mutator soak, 1184 tests green, all gates (`clippy -D warnings`, `fmt`,
`build --features nyx`) clean.

## Next stages, in value order

### 1. Differential seed run (blocked here, ready to run)
Docker egress to Debian mirrors is broken in the dev container, so the
`smite-<target>-ir` images could not be built locally. On any host with
working docker egress:

```bash
docker build -t smite-ldk-ir -f workloads/ldk/Dockerfile --build-arg SCENARIO=ir .
cargo run --release -p smite-requirements --bin smite-requirements -- \
  emit smite-requirements/data/bolt02-peer-protocol.md /tmp/smite-seeds
scripts/diff-run-seeds.sh ldk /tmp/smite-seeds /tmp/ldk.csv   # repeat per target
```

Cross-target divergence rows in the CSVs are finding candidates; uniform
rejections quantify how shallow static seeds are vs the stateful
generators. This is also the first executor run against real nodes —
treat any panic as a bug.

### 2. First campaign (KVM/Nyx hardware)
`smitebot seeds <md> <dir>` then `smitebot start campaign.toml` with
`seed_dir` pointing at the emitted corpus. Success metric: corpus
coverage vs the empty-seed baseline (scripts/coverage-report.sh).

### 3. Splice balance-conservation oracle (design)
The executor's `channel_states` hold the negotiated
`funding_satoshis`/`push_msat`; our sent splice contributions are
observable client-side. A splice-out exceeding our balance MUST be
rejected by the target — detecting the rejection needs response-shape
observation (abort vs silence vs ack), which is the missing piece.

### 4. VLS integration (needs owner input)
No VLS code exists in this repo or its remotes. If "VLS" means Validating
Lightning Signer, the promising angle is a signer-proxy workload whose
approve/reject/signature responses are fuzz-driven at commitment and
splice moments. Needs a pointer to the existing VLS work first.

## Invariants to preserve

- **Frozen discriminants**: new `Operation` variants are appended at the
  enum tail only. `operation_variant_discriminants_are_frozen` fails CI
  otherwise. Bump `PRE_SESSION_VARIANT_COUNT` never; add new sentinels if
  the tail itself needs freezing.
- **Executor robustness contract**: wire-level malformation from mutated
  inputs must never panic — zero-pad or fall back (see the SendTxAddInput
  / SendTxSignatures blocks for the pattern).
- **Corpus count is CI-pinned** (95): converter changes that alter it
  must bump the number in `.github/workflows/rust.yml` and the README
  deliberately.
- The mutator soak test must stay green: mutators preserve program
  validity by construction.
