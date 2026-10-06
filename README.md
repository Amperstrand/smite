# Smite

Smite is a coverage-guided fuzzing framework for Lightning Network implementations, derived from [fuzzamoto](https://github.com/dergoegge/fuzzamoto).

## Supported Targets

- [LND](https://github.com/lightningnetwork/lnd)
- [LDK](https://github.com/lightningdevkit/ldk-node)
- [CLN](https://github.com/ElementsProject/lightning)
- [Eclair](https://github.com/ACINQ/eclair)

## Prerequisites

- x86_64 architecture
- Modern Linux operating system
- Docker
- [AFL++](https://github.com/AFLplusplus/AFLplusplus) built from source with Nyx mode

## Quick Start

Choose a target (`lnd`, `ldk`, `cln`, or `eclair`) and a scenario (`encrypted_bytes`, `noise`, or `init`) and follow the steps below:

```bash
# Choose target and scenario
TARGET=lnd
SCENARIO=encrypted_bytes

# Build the Docker image
docker build -t smite-$TARGET-$SCENARIO -f workloads/$TARGET/Dockerfile --build-arg SCENARIO=$SCENARIO .

# Enable the KVM VMware backdoor (required for Nyx)
./scripts/enable-vmware-backdoor.sh

# Create the Nyx sharedir
./scripts/setup-nyx.sh /tmp/smite-nyx smite-$TARGET-$SCENARIO ~/AFLplusplus

# Create seed corpus
mkdir -p /tmp/smite-seeds
echo 'AAAA' > /tmp/smite-seeds/seed1

# Start fuzzing
~/AFLplusplus/afl-fuzz -X -i /tmp/smite-seeds -o /tmp/smite-out -- /tmp/smite-nyx
```

### IR Scenario

The `ir` scenario uses structured IR programs instead of raw bytes. It requires
a custom mutator library and different AFL++ flags:

```bash
TARGET=ldk
SCENARIO=ir

# Build the Docker image and Nyx sharedir as above
docker build -t smite-$TARGET-$SCENARIO -f workloads/$TARGET/Dockerfile --build-arg SCENARIO=$SCENARIO .
./scripts/setup-nyx.sh /tmp/smite-nyx smite-$TARGET-$SCENARIO ~/AFLplusplus

# Build the custom mutator
cargo build --release -p smite-ir-mutator

# Create seed corpus (an empty file works -- the mutator generates fresh programs)
mkdir -p /tmp/smite-seeds
printf '\x00' > /tmp/smite-seeds/empty

# Start fuzzing with the custom mutator
AFL_CUSTOM_MUTATOR_LIBRARY=target/release/libsmite_ir_mutator.so \
AFL_CUSTOM_MUTATOR_ONLY=1 \
AFL_FRAMESHIFT_DISABLE=1 \
~/AFLplusplus/afl-fuzz -X -i /tmp/smite-seeds -o /tmp/smite-out -- /tmp/smite-nyx
```

`AFL_CUSTOM_MUTATOR_ONLY=1` disables AFL++'s built-in mutators (which would
corrupt the postcard encoding).

## Spec-Derived Seed Pipeline

The `smite-requirements` crate derives IR seed corpora directly from BOLT
specifications: BOLT markdown is parsed into requirements (`extract`),
requirements become violation seeds (`seeds`), seeds become program sketches
(`sketches`), sketches are checked for sketch-to-IR conversion (`programs`),
and convertible sketches are written as postcard-encoded seed files (`emit`).

```bash
# Extract requirements from a BOLT markdown file
cargo run --release -p smite-requirements --bin smite-requirements -- extract /tmp/bolts/02-peer-protocol.md

# Derive violation seeds and program sketches, check conversion
cargo run --release -p smite-requirements --bin smite-requirements -- seeds /tmp/bolts/02-peer-protocol.md
cargo run --release -p smite-requirements --bin smite-requirements -- sketches /tmp/bolts/02-peer-protocol.md
cargo run --release -p smite-requirements --bin smite-requirements -- programs /tmp/bolts/02-peer-protocol.md

# Emit postcard-encoded seed files usable as an ir scenario corpus
cargo run --release -p smite-requirements --bin smite-requirements -- emit /tmp/bolts/02-peer-protocol.md /tmp/smite-seeds

# Verify emitted seeds decode to executable IR programs (no Nyx needed)
cargo run --release -p smite-requirements --bin verify_seeds /tmp/smite-seeds
```

`emit` reports postcard roundtrip counts; `verify_seeds` decodes every
`.seed` file and validates its structure (input bounds, send presence)
without AFL++/Nyx, exiting non-zero if any seed is invalid.

Current status: 95 of 95 sketches convert to executable programs (503 IR
operations), covering stfu, splice_init/ack/locked, the full interactive-tx
family (tx_add_input with shared_input_txid TLV, tx_add_output, tx_complete,
tx_abort, tx_init_rbf, tx_ack_rbf, tx_signatures),
open_channel/channel_ready/commitment_signed, and the dual-funding variants
via pre-encoded messages.

## Running Modes

### Nyx Mode

Uses the [Nyx hypervisor](https://nyx-fuzz.com/) for fast snapshot-based fuzzing.
AFL++ manages the fuzzing loop and coverage feedback.

The `-X` flag enables standalone Nyx mode:

```bash
afl-fuzz -X -i <seeds> -o <output> -- <sharedir>
```

### Local Mode

This mode runs without Nyx and is used to reproduce and debug crashes.

#### Reproducing Crashes

When AFL++ finds a crash:

```bash
# Get the crash input
cp /tmp/smite-out/default/crashes/<crashing-input> ./crash

# Reproduce in local mode (use the matching image and scenario binary)
docker run --rm --tmpfs /tmp:rw,exec,size=1g -v $PWD/crash:/input.bin -e SMITE_INPUT=/input.bin smite-$TARGET-$SCENARIO /$TARGET-scenario
```

The `--tmpfs /tmp:rw,exec,size=1g` option provides a temporary in-memory filesystem for `/tmp`, matching the `tmpfs` environment available inside the Nyx VM. This halves startup time.

### Coverage Report Mode

Generate an HTML coverage report showing which parts of the target were exercised by a fuzzing corpus:

```bash
# Generate coverage report
./scripts/coverage-report.sh $TARGET $SCENARIO /tmp/smite-out/default/queue/

# View the report
firefox ./$TARGET-$SCENARIO-coverage-report/html/index.html
```

## Project Structure

```
smite/              # Core Rust library (runners, scenarios, noise protocol, BOLT messages)
smitebot/           # Automation CLI for fuzzing campaign orchestration 
smite-ir/           # IR types, generators, and mutators for structured fuzzing programs
smite-ir-mutator/   # AFL++ custom mutator cdylib for IR programs
smite-nyx-sys/      # Nyx FFI bindings
smite-requirements/ # BOLT requirement extraction and sketch-to-IR seed conversion (data/ holds checked-in bolt02 artifacts)
smite-scenarios/    # Scenario implementations and target binaries
workloads/
  lnd/              # LND fuzzing workload (Dockerfile, init script)
  ldk/              # LDK fuzzing workload (Dockerfile, init script, ldk-node wrapper)
  cln/              # CLN fuzzing workload (Dockerfile, init script)
  eclair/           # Eclair fuzzing workload (Dockerfile, init script, instrumentation agent)
scripts/
  setup-nyx.sh              # Helper to create Nyx sharedirs
  enable-vmware-backdoor.sh # Enable KVM VMware backdoor for Nyx
  coverage-report.sh        # Generate a coverage report for any scenario
  symbolize-crash.sh        # Symbolize CLN crash report stack traces
```
