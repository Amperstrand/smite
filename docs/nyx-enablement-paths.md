# Nyx Execution on ai-legion — Three Paths

The full Nyx snapshot pipeline (AFL++ + Nyx + custom mutator) requires
the `kvm_intel` module loaded with `enable_vmware_backdoor=1`. The
grub parameter is already set (`kvm_intel.enable_vmware_backdoor=1` in
`/etc/default/grub`) — it activates on the next boot.

## Option A — Brief VM pause (no reboot, ~30s lab downtime)

Four qemu VMs hold `/dev/kvm` and prevent module reload:

```
qemu #1  /root/src/omarchy-cashu                      (omarchy-cashu testing)
qemu #2  /root/src/physical-router-test-automation    (router lab)
qemu #3  /root/tollgate-virtual-lab                   (tollgate lab)
qemu #4  /root/src                                    (agent workspace)
```

Procedure:
```bash
# 1. Stop the 4 qemu processes (graceful: monitor commands; hard: kill)
for pid in $(lsof -t /dev/kvm); do kill $pid; done

# 2. Wait for KVM references to drop
while [ "$(cat /sys/module/kvm_intel/refcnt)" -gt 0 ]; do sleep 1; done

# 3. Reload with the parameter
modprobe -r kvm_intel && modprobe kvm_intel enable_vmware_backdoor=1

# 4. Restart the VMs (their init scripts / systemd units)
```

Risk: the 4 VMs need graceful restart paths. Everything else (Docker,
zoo mints, regtest, agents) stays up throughout.

## Option B — Reboot (permanent fix, already staged)

The grub kernel parameter is set:
```
GRUB_CMDLINE_LINUX_DEFAULT="quiet splash kvm_intel.enable_vmware_backdoor=1"
```

On next reboot:
- `kvm_intel` loads with the VMware backdoor enabled automatically
- Secure Boot stays on; lockdown stays; nothing else changes
- Nyx works from boot — no further action ever needed

**Recommended timing**: next scheduled maintenance window (owner
estimates ~1 week from 2026-10-01).

## Option C — Non-Nyx execution (works immediately, no disruption)

Runs IR programs directly against a live CLN regtest node without
AFL++/Nyx. Slower (no snapshot isolation = no instant state restore
between programs), but proves the programs execute correctly against
a real target and validates the entire deterministic pipeline.

This is what we're running today.

## Status

- ** grub parameter set** (activates on next boot = Option B ready)
- **All pre-execution layers proven** (seeds, mutator, Docker image)
- **Running Option C** while waiting for the reboot window
