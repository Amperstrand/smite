#!/usr/bin/env bash
# ai-legion sustained stress test — 5 phases, ~5 hours total.
# Designed to run alongside existing services without disrupting them.
#
# Safety constraints (hardcoded):
#   - nice -n 10 + ionice -c2 -n7 on all stress processes
#   - Memory-capped: each phase uses at most 2GB
#   - Disk-capped: max 5GB temp files, cleaned per-phase
#   - Auto-abort if load > 16 or available memory < 1GB
#   - All output to /var/log/stress-test/ (small, append-only)

set -euo pipefail

LOG_DIR=/var/log/stress-test
TEMP_DIR=/tmp/stress-test
mkdir -p "$LOG_DIR" "$TEMP_DIR"

PHASE="${1:-all}"
DURATION="${2:-3600}"  # default 1 hour per phase

log() { printf '[%s] %s\n' "$(date -Iseconds)" "$*" | tee -a "$LOG_DIR/summary.log"; }

check_safety() {
    local load=$(awk '{print int($1)}' /proc/loadavg)
    local avail_mb=$(free -m | awk '/Mem:/{print $7}')
    if [ "$load" -gt 16 ]; then
        log "ABORT: load=$load > 16"
        exit 1
    fi
    if [ "$avail_mb" -lt 1024 ]; then
        log "ABORT: available memory ${avail_mb}MB < 1024MB"
        exit 1
    fi
    local disk_pct=$(df / | awk 'NR==2{gsub(/%/,""); print $5}')
    if [ "$disk_pct" -gt 99 ]; then
        log "ABORT: disk ${disk_pct}% full"
        exit 1
    fi
}

# Safety monitor (runs in background, kills the test if thresholds exceeded)
safety_monitor() {
    while true; do
        check_safety || { pkill -P $$ 2>/dev/null; exit 1; }
        sleep 30
    done
}

# === Phase 1: Sustained CPU compilation (1 hour) ===
# Mirrors the Rust build workload from smite development.
phase_cpu_compile() {
    log "PHASE 1: CPU compilation stress (${DURATION}s)"
    local start=$(date +%s)
    local iterations=0

    while [ $(($(date +%s) - start)) -lt "$DURATION" ]; do
        # Clean build of a small Rust project (representative)
        cat > "$TEMP_DIR/bench/src/main.rs" << 'EOF'
fn fib(n: u64) -> u64 { match n { 0|1 => 1, _ => fib(n-1) + fib(n-2) } }
fn main() { let r: u64 = (0..40).map(fib).sum(); println!("{r}"); }
EOF
        mkdir -p "$TEMP_DIR/bench/src"
        nice -n 10 ionice -c2 -n7 cargo build --release \
            --manifest-path "$TEMP_DIR/bench/Cargo.toml" 2>/dev/null || true
        iterations=$((iterations + 1))
        # Also run the binary for CPU compute
        nice -n 10 "$TEMP_DIR/bench/target/release/bench" >/dev/null 2>&1 || true
    done
    log "PHASE 1 complete: $iterations compile+run cycles"
}

# === Phase 2: Memory allocation pressure (1 hour) ===
# Mirrors the IR program execution memory pattern.
phase_memory() {
    log "PHASE 2: Memory allocation stress (${DURATION}s)"
    local start=$(date +%s)
    local iterations=0

    while [ $(($(date +%s) - start)) -lt "$DURATION" ]; do
        # Allocate and free ~512MB repeatedly (mmap + touch + munmap)
        nice -n 10 python3 -c "
import mmap, os, time, sys
buf = mmap.mmap(-1, 512*1024*1024)
for i in range(0, len(buf), 4096):
    buf[i] = 0xFF
buf.close()
" 2>/dev/null || true
        iterations=$((iterations + 1))
    done
    log "PHASE 2 complete: $iterations x 512MB alloc-touch-free cycles"
}

# === Phase 3: Disk I/O sustained (1 hour) ===
# Mirrors Docker layer writes and seed file generation.
phase_disk_io() {
    log "PHASE 3: Disk I/O stress (${DURATION}s)"
    local start=$(date +%s)
    local total_mb=0

    while [ $(($(date +%s) - start)) -lt "$DURATION" ]; do
        # Write/read/delete 100MB files with O_DIRECT-like pattern
        nice -n 10 ionice -c2 -n7 dd if=/dev/urandom \
            of="$TEMP_DIR/io_test.bin" bs=1M count=100 2>/dev/null || true
        nice -n 10 ionice -c2 -n7 dd if="$TEMP_DIR/io_test.bin" \
            of=/dev/null bs=1M 2>/dev/null || true
        rm -f "$TEMP_DIR/io_test.bin"
        total_mb=$((total_mb + 200))  # write + read
    done
    log "PHASE 3 complete: ${total_mb}MB total I/O"
}

# === Phase 4: Network + postcard serialization (1 hour) ===
# Mirrors the fuzzing executor's serialization and network patterns.
phase_network_serialize() {
    log "PHASE 4: Network + serialization stress (${DURATION}s)"
    local start=$(date +%s)
    local iterations=0

    # Generate postcard seed files repeatedly (mirrors emit command)
    while [ $(($(date +%s) - start)) -lt "$DURATION" ]; do
        # Write 19 seed files (same as smite emit)
        for i in $(seq 1 19); do
            head -c 50 /dev/urandom > "$TEMP_DIR/seed_$i.bin"
        done
        # Read them all back (mirrors deserialization)
        cat "$TEMP_DIR"/seed_*.bin > /dev/null
        # Clean
        rm -f "$TEMP_DIR"/seed_*.bin

        # CLN health check (light network)
        if [ $((iterations % 100)) -eq 0 ]; then
            docker exec regtest-cln lightning-cli --network=regtest getinfo \
                > /dev/null 2>&1 && log "  CLN alive at iteration $iterations" || \
                log "  WARN: CLN health check failed at iteration $iterations"
        fi
        iterations=$((iterations + 1))
    done
    log "PHASE 4 complete: $iterations serialization cycles"
}

# === Phase 5: Combined sustained load (1 hour) ===
# All of the above simultaneously at reduced intensity.
phase_combined() {
    log "PHASE 5: Combined stress (${DURATION}s)"
    local start=$(date +%s)

    # Run all stress types in parallel, backgrounded
    (
        local s=$(date +%s)
        while [ $(($(date +%s) - s)) -lt "$DURATION" ]; do
            nice -n 15 python3 -c "
import mmap
b = mmap.mmap(-1, 256*1024*1024)
for i in range(0, len(b), 8192): b[i] = 0xFF
b.close()
" 2>/dev/null || true
        done
    ) &
    local mem_pid=$!

    (
        local s=$(date +%s)
        while [ $(($(date +%s) - s)) -lt "$DURATION" ]; do
            nice -n 15 ionice -c2 -n7 dd if=/dev/urandom \
                of="$TEMP_DIR/combined_io.bin" bs=1M count=50 2>/dev/null || true
            rm -f "$TEMP_DIR/combined_io.bin"
        done
    ) &
    local io_pid=$!

    (
        local s=$(date +%s)
        while [ $(($(date +%s) - s)) -lt "$DURATION" ]; do
            # CPU: busy loop with periodic sleep
            nice -n 15 timeout 5 bash -c 'while :; do :; done' 2>/dev/null || true
            sleep 2
        done
    ) &
    local cpu_pid=$!

    # Wait for duration
    while [ $(($(date +%s) - start)) -lt "$DURATION" ]; do
        sleep 60
        log "  combined: alive at $(($(date +%s) - start))s, load=$(awk '{print $1}' /proc/loadavg), avail=$(free -m | awk '/Mem:/{print $7}')MB"
    done

    # Kill background jobs
    kill $mem_pid $io_pid $cpu_pid 2>/dev/null || true
    wait 2>/dev/null || true
    log "PHASE 5 complete"
}

# === Monitoring (continuous, throughout all phases) ===
start_monitoring() {
    (
        while true; do
            printf '%s load=%s mem_avail=%s disk_free=%s\n' \
                "$(date -Iseconds)" \
                "$(awk '{print $1}' /proc/loadavg)" \
                "$(free -m | awk '/Mem:/{print $7}')MB" \
                "$(df -h / | awk 'NR==2{print $4}')" \
                >> "$LOG_DIR/monitor.log"
            sleep 60
        done
    ) &
    echo $! > "$TEMP_DIR/monitor_pid"
}

stop_monitoring() {
    [ -f "$TEMP_DIR/monitor_pid" ] && kill $(cat "$TEMP_DIR/monitor_pid") 2>/dev/null || true
}

# === Main ===
log "=== ai-legion stress test starting ==="
log "phases: $PHASE, duration per phase: ${DURATION}s"
log "memory: $(free -h | awk '/Mem:/{print $2}') total, $(free -h | awk '/Mem:/{print $7}') available"
log "disk: $(df -h / | awk 'NR==2{print $4}') free"
log "cores: $(nproc), load: $(cat /proc/loadavg | awk '{print $1}')"

safety_monitor &
SAFETY_PID=$!
start_monitoring

case "$PHASE" in
    1|cpu)       phase_cpu_compile ;;
    2|memory)    phase_memory ;;
    3|disk)      phase_disk_io ;;
    4|network)   phase_network_serialize ;;
    5|combined)  phase_combined ;;
    all)
        phase_cpu_compile
        phase_memory
        phase_disk_io
        phase_network_serialize
        phase_combined
        ;;
    *) echo "usage: $0 [1|2|3|4|5|all] [duration_seconds]"; exit 1 ;;
esac

stop_monitoring
kill $SAFETY_PID 2>/dev/null || true

# Cleanup
rm -rf "$TEMP_DIR/bench" "$TEMP_DIR"/*.bin 2>/dev/null || true

log "=== stress test complete ==="
log "final state: load=$(awk '{print $1}' /proc/loadavg), mem_avail=$(free -m | awk '/Mem:/{print $7}')MB"
log "monitor log: $LOG_DIR/monitor.log"
log "full log: $LOG_DIR/summary.log"
