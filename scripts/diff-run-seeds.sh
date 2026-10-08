#!/usr/bin/env bash
# Differential seed execution: runs every emitted seed through a target's
# ir scenario binary in local (non-Nyx) Docker mode and classifies the
# outcome. Produces a per-seed outcome table for cross-target comparison.
#
# Usage: diff-run.sh <target> <seed-dir> <out-csv>
set -u

target="$1"
seed_dir="$2"
out_csv="$3"
image="smite-${target}-ir"
timeout_s=60

echo "seed,outcome,detail" > "$out_csv"

for seed in "$seed_dir"/*.seed; do
    name=$(basename "$seed")
    # Docker run with the seed mounted as SMITE_INPUT, mirroring the README
    # local-mode reproduction flow for the ir scenario.
    out=$(timeout "$timeout_s" docker run --rm \
        --tmpfs /tmp:rw,exec,size=1g \
        -v "$(readlink -f "$seed")":/input.bin:ro \
        -e SMITE_INPUT=/input.bin \
        "$image" "/${target}-scenario" 2>&1)
    rc=$?

    case "$rc:$out" in
        0:*)
            outcome=ok
            detail=$(echo "$out" | tail -1 | cut -c1-80)
            ;;
        124:*)
            outcome=timeout
            detail="docker timeout ${timeout_s}s"
            ;;
        *)
            # Classify by the executor's own error taxonomy in the output.
            if echo "$out" | grep -q "postcard\|decode"; then
                outcome=decode_error
            elif echo "$out" | grep -qi "unexpected message\|UnexpectedMessage"; then
                outcome=unexpected_message
            elif echo "$out" | grep -qi "timed out\|timeout\|idle"; then
                outcome=recv_timeout
            elif echo "$out" | grep -qi "panic"; then
                outcome=panic
            elif echo "$out" | grep -qi "violation"; then
                outcome=oracle_violation
            else
                outcome=error_rc_${rc}
            fi
            detail=$(echo "$out" | tail -1 | cut -c1-80)
            ;;
    esac

    echo "${name},${outcome},\"${detail//\"/'}\"" >> "$out_csv"
    printf '%-70s %s\n' "$name" "$outcome"
done

echo "---"
awk -F, 'NR>1 {count[$2]++} END {for (o in count) print o, count[o]}' "$out_csv" | sort
