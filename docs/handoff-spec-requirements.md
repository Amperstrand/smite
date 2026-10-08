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
contributions into the conservation check.

Update: the post-ack balance adjustment landed — an acknowledged
contribution (positive splice-in, negative splice-out, saturating) is
applied to the holder-side `balance_msat`, so sequential splices are
judged against the updated balance (regression-tested: a second splice
that only overdraws the *adjusted* balance is flagged). The oracle is
now symmetric: an ack whose own `funding_contribution_satoshis` splices
out beyond the target's tracked counterparty balance is itself a
violation, judged independently of our contribution. The pipeline also
gained a determinism pin (`emit_seed_dir_is_deterministic`): two emits
from the checked-in BOLT source must be byte-identical.

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

## Lessons learned (this branch's arc)

1. **Serialization-index freezes are invisible until they break.**
   Inserting enum variants mid-declaration silently reinterprets every
   persisted postcard corpus — no decode error, just different programs.
   Append-only + a pin test (`operation_variant_discriminants_are_frozen`)
   turned an invisible hazard into a CI failure.
2. **A new generator can weaponize a pre-existing panic.** The executor's
   `SendSpliceLocked [..32]` slice was harmless until
   `SpliceFlowGenerator` began feeding it 0–256-byte payloads (~12.5% of
   fresh programs crashed, polluting AFL with false findings). Rule: any
   generator emitting into a fixed-width field either produces valid
   lengths or the executor pads — belt and braces, both.
3. **Oracle checks with early returns silently skip later checks.** The
   symmetric splice-conservation check was dead code behind the
   holder-side `>= 0` early return until a regression test caught it.
   Write one violating test per oracle invariant, not one per oracle.
4. **Test fixture encodings deserve the same skepticism as production
   code.** `2^63 + N` is not the two's-complement encoding of `-N`; the
   oracle's saturating multiply exposed it as `u64::MAX`.
5. **When CI can't run, run the CI scripts yourself.** The repo's
   `check-bolt-msg-type-order.sh` had been failing unnoticed for the
   entire splice arc — latent only because Actions never initialized.
6. **Shared worktrees demand file-scoped commits.** Multiple fleet
   agents share this checkout; `git add <files>` (never `-a`), a
   `git status` check before every commit, and leaving foreign
   working-tree changes alone kept 40+ commits from three agents
   cleanly separated.
7. **Make unobservability explicit instead of guessing.** Quiescence
   could not be observed directly (no Recv op consumes the target's
   `stfu`), so the oracle arms on provable engagement instead —
   documented in the op docs — rather than on a fiction.
