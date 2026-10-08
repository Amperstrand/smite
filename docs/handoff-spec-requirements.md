# Handoff: spec-requirements branch

State as of the PR #1 head: seed pipeline complete and CI-pinned, IR
surface covering splice / interactive-tx / commitment families, 12
generators including stateful live-channel flows, two stateful oracles,
mutator soak, 1184 tests green, all gates (`clippy -D warnings`, `fmt`,
`build --features nyx`) clean.

## Gate summary (fleet sweep, 131e5bf)

All local gates green at `131e5bf` (pushed): 1196 tests, 0 failures
(`smite` 786 incl. three splice roundtrips, `smite-scenarios` 82 incl.
four response-shape tests); `clippy --all-targets --all-features
-D warnings` clean; `cargo fmt --all --check` clean; `build --features
nyx` clean; `check-bolt-msg-type-order.sh` all seven blocks OK;
seed pipeline gate 95/95 postcard roundtrips, `verify_seeds` pass,
473 requirements / 0 drift. Splice response-shape observation landed
(see stage 3). No further stages are executable from this container:
stage 1 needs working docker egress, stage 2 needs KVM/Nyx hardware,
stage 3's remaining extensions are design decisions (see below),
stage 4 needs a VLS pointer. Standing by.

## Gate status (post stand-down re-check)

- **PR #1**: OPEN, mergeable, no reviews yet. `ffa3860..` pushed.
- **Actions has never run on this repo** (zero workflow runs, zero check
  runs on the head SHA) despite `actions/permissions.enabled = true` and
  the workflow targeting `pull_request` → `master`. The new pipeline gate
  step is locally verified but has never executed on a GitHub runner.
  Owner action: visit the repo's Actions tab (GitHub often defers
  workflow initialization until first visit / explicit enable on forks);
  if runs then appear, confirm the `Spec-derived seed pipeline gate` step
  passes.
- **Docker egress re-probed, still broken** in the dev container
  (apt hangs inside containers; host HTTPS fine). The differential run
  stays deferred to hardware per stage 1 below.
- **`check-bolt-msg-type-order.sh` had been failing unnoticed** since
  the splice dispatch landed (`TX_ABORT` declared out of canonical
  order, and the splice messages were missing from the roundtrip tests
  and `message_type_values`). Fixed: `TxAbort` reordered into canonical
  position across all seven checked blocks, three splice roundtrip
  tests and `message_type_values` entries added. Latent only because
  Actions has never run — see the first gate item.

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

Status: the ack path landed as `SpliceAckOracle` (in-band judgment in
`RecvSpliceAck`), and response-shape observation now covers the rest:
`recv_tracked` classifies every answer to a splice we initiated —
`tx_abort` (Aborted), BOLT `error` (Errored), out-of-band `splice_ack`
(still judged by `SpliceAckOracle` even when the program expected a
different message), and end-of-program silence (Silent, logged in the
run summary; not judged — only acceptance is unambiguous). Rejection
shapes drain the pending contribution instead of leaking it. Still
open: summing `tx_add_input`/`tx_add_output` interactive-tx
contributions into the conservation check, and adjusting
`channel_states` balances after a completed splice so sequential
splices are not judged against a stale balance.

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
