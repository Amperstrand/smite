#!/usr/bin/env bash
# ai-legion sustained stress test — 5 phases × 1 hour = 5 hours total.
# Uses stress-ng for reliable, controlled load generation.
# Safety: nice/ionice, auto-abort thresholds, capped resource usage.

set -euo pipefail

LOG_DIR=/var/log/stress-test
TEMP_DIR=/tmp/stress-test
mkdir -p "$LOG_DIR" "$TEMP_DIR"

PHASE="${1:-all}"
DUR="${2:-3600}"
NICE="nice -n 10 ionice -c2 -n7"

log() { printf '[%s] %s\n' "$(date -Iseconds)" "$*" | tee -a "$LOG_DIR/summary.log"; }

check_safety() {
    local load=$(awk '{print int($1)}' /proc/loadavg)
    local avail=$(free -m | awk '/Mem:/{print $7}')
    [ "$load" -gt 16 ] && { log "ABORT: load=$load"; kill $(cat "$TEMP_DIR/pids" 2>/dev/null) 2>/dev/null; exit 1; }
    [ "$avail" -lt 1024 ] && { log "ABORT: mem=${avail}MB"; kill $(cat "$TEMP_DIR/pids" 2>/dev/null) 2>/dev/null; exit 1; }
}

# Background safety + metrics monitors
(
    while true; do check_safety; sleep 30; done
) &
echo $! >> "$TEMP_DIR/pids"

(
    while true; do
        printf '%s load=%s mem=%sMB disk=%s temp=%s\n' \
            "$(date -Iseconds)" \
            "$(awk '{print $1}' /proc/loadavg)" \
            "$(free -m | awk '/Mem:/{print $7}')" \
            "$(df -h / | awk 'NR==2{print $4}')" \
            "$(cat /sys/class/thermal/thermal_zone*/temp 2>/dev/null | head -1 | awk '{printf "%.0f°C", $1/1000}')" \
            >> "$LOG_DIR/monitor.log" 2>/dev/null
        sleep 60
    done
) &
echo $! >> "$TEMP_DIR/pids"

phase1_cpu() {
    log "PHASE 1: CPU stress — 8 workers × ${DUR}s"
    $NICE stress-ng --cpu 8 --timeout "${DUR}s" --metrics-brief \
        --log-file "$LOG_DIR/phase1.log" 2>/dev/null || true
    log "PHASE 1 done: $(grep 'cpu\|bogo' "$LOG_DIR/phase1.log" 2>/dev/null | tail -3 | tr '\n' ' ')"
}

phase2_memory() {
    log "PHASE 2: Memory stress — 4GB cap, ${DUR}s"
    $NICE stress-ng --vm 2 --vm-bytes 2G --timeout "${DUR}s" --metrics-brief \
        --log-file "$LOG_DIR/phase2.log" 2>/dev/null || true
    log "PHASE 2 done: $(grep 'vm\|bogo' "$LOG_DIR/phase2.log" 2>/dev/null | tail -3 | tr '\n' ' ')"
}

phase3_disk() {
    log "PHASE 3: Disk I/O stress — ${DUR}s"
    $NICE stress-ng --hdd 4 --hdd-bytes 512M --timeout "${DUR}s" --metrics-brief \
        --log-file "$LOG_DIR/phase3.log" 2>/dev/null || true
    log "PHASE 3 done: $(grep 'hdd\|bogo' "$LOG_DIR/phase3.log" 2>/dev/null | tail -3 | tr '\n' ' ')"
}

phase4_network() {
    log "PHASE 4: Network + serialization — ${DUR}s"
    local start=$(date +%s)
    local iters=0
    while [ $(($(date +%s) - start)) -lt "$DUR" ]; do
        # Postcard-sized file churn (mirrors seed emit/read)
        for i in $(seq 1 19); do
            head -c 50 /dev/urandom > "$TEMP_DIR/s$i.bin"
        done
        sync
        cat "$TEMP_DIR"/s*.bin > /dev/null
        rm -f "$TEMP_DIR"/s*.bin
        # CLN health check every 200 iterations
        if [ $((iters % 200)) -eq 0 ] && [ "$iters" -gt 0 ]; then
            docker exec regtest-cln lightning-cli --network=regtest getinfo \
                >/dev/null 2>&1 && log "  CLN alive (iter $iters)" || log "  WARN: CLN unreachable (iter $iters)"
        fi
        iters=$((iters + 1))
    done
    log "PHASE 4 done: $iters cycles"
}

phase5_combined() {
    log "PHASE 5: Combined — CPU+mem+I/O × ${DUR}s"
    # Moderate CPU + light memory + moderate I/O simultaneously
    $NICE stress-ng \
        --cpu 4 \
        --vm 1 --vm-bytes 1G \
        --hdd 2 --hdd-bytes 256M \
        --timeout "${DUR}s" --metrics-brief \
        --log-file "$LOG_DIR/phase5.log" 2>/dev/null || true
    log "PHASE 5 done: $(grep -E 'cpu|vm|hdd|bogo' "$LOG_DIR/phase5.log" 2>/dev/null | tail -5 | tr '\n' ' ')"
}

log "=== ai-legion 5-hour stress test ==="
log "phases=$PHASE dur=${DUR}s cores=$(nproc) mem=$(free -h | awk '/Mem:/{print $2}') disk=$(df -h / | awk 'NR==2{print $4}') free"

case "$PHASE" in
    1) phase1_cpu ;;
    2) phase2_memory ;;
    3) phase3_disk ;;
    4) phase4_network ;;
    5) phase5_combined ;;
    all)
        phase1_cpu
        phase2_memory
        phase3_disk
        phase4_network
        phase5_combined
        ;;
esac

kill $(cat "$TEMP_DIR/pids" 2>/dev/null) 2>/dev/null || true
rm -rf "$TEMP_DIR"

log "=== COMPLETE ==="
log "final: load=$(awk '{print $1}' /proc/loadavg) mem=$(free -m | awk '/Mem:/{print $7}')MB"
log "logs in $LOG_DIR/"
