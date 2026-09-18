package lnd

/*
#cgo CFLAGS: -fPIC

#define _GNU_SOURCE
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/shm.h>
#include <unistd.h>

static void fatal(const char *msg) {
  fprintf(stderr, "sancov: %s\n", msg);
  exit(1);
}

// Called once by Go's libfuzzer runtime with the bounds of the counter section.
//
// Maps AFL's shared memory over the counter section, so instrumented code
// writes coverage straight into AFL's map. Relies on align.ld giving the
// section its own pages.
void __sanitizer_cov_8bit_counters_init(char *start, char *end) {
  static int initialized = 0;
  size_t size = (size_t)(end - start);

  if (getenv("AFL_DUMP_MAP_SIZE")) {
    printf("%zu\n", size);
    exit(0);
  }

  const char *shm_id_str = getenv("__AFL_SHM_ID");
  if (!shm_id_str) {
    return; // Not fuzzing.
  }
  if (initialized) {
    fatal("counters registered twice");
  }
  initialized = 1;

  size_t page = (size_t)sysconf(_SC_PAGESIZE);
  if (page != 4096) {
    fatal("page size is not 4096");
  }
  if ((uintptr_t)start % page != 0) {
    fatal("counter section is not page-aligned, was LND linked with align.ld?");
  }

  int shm_id = atoi(shm_id_str);
  struct shmid_ds ds;
  if (shmctl(shm_id, IPC_STAT, &ds) == -1) {
    fatal("failed to stat the AFL shared memory segment");
  }
  // shmat maps the whole segment, so a larger one would overwrite whatever
  // follows the counter section.
  if (ds.shm_segsz != size) {
    fatal("AFL map size doesn't match the counter section");
  }

  // Hits so far are lost, but nyx_get_fuzz_input zeroes the map before the
  // snapshot anyway.
  if (shmat(shm_id, start, SHM_REMAP) == (void *)-1) {
    fatal("failed to map the AFL shared memory over the counters");
  }
}

void __sanitizer_cov_pcs_init(const uintptr_t *pcs_beg,
                              const uintptr_t *pcs_end) {
  // PC table not used for AFL coverage.
}

// Empty stubs for comparison tracing hooks. Go's libfuzzer instrumentation
// emits calls to these, so we need to provide them to satisfy the linker.
// Marked weak so they can be overridden by real implementations if desired.
__attribute__((weak)) void __sanitizer_cov_trace_cmp1(uint8_t arg1,
                                                      uint8_t arg2) {}

__attribute__((weak)) void __sanitizer_cov_trace_cmp2(uint16_t arg1,
                                                      uint16_t arg2) {}

__attribute__((weak)) void __sanitizer_cov_trace_cmp4(uint32_t arg1,
                                                      uint32_t arg2) {}

__attribute__((weak)) void __sanitizer_cov_trace_cmp8(uint64_t arg1,
                                                      uint64_t arg2) {}

__attribute__((weak)) void __sanitizer_cov_trace_const_cmp1(uint8_t arg1,
                                                            uint8_t arg2) {}

__attribute__((weak)) void __sanitizer_cov_trace_const_cmp2(uint16_t arg1,
                                                            uint16_t arg2) {}

__attribute__((weak)) void __sanitizer_cov_trace_const_cmp4(uint32_t arg1,
                                                            uint32_t arg2) {}

__attribute__((weak)) void __sanitizer_cov_trace_const_cmp8(uint64_t arg1,
                                                            uint64_t arg2) {}

__attribute__((weak)) void __sanitizer_weak_hook_strcmp(void *caller_pc,
                                                        const char *s1,
                                                        const char *s2,
                                                        int result) {}
*/
import "C"

import (
	"os"
)

// This file provides coverage tracking for Go programs built with -d=libfuzzer.
// The C code above maps the coverage counters onto AFL's shared memory, so no
// per-execution work is needed to report coverage.
//
// The Go code answers the scenario's liveness handshake over pipes:
// - Scenario writes a byte to the trigger fd
// - We echo it on the ack fd
// - Scenario reads the ack; EOF means LND died

func init() {
	// Only answer the handshake if we're in fuzzing mode
	if os.Getenv("__AFL_SHM_ID") == "" {
		return
	}

	// Any scenario that starts LND as a subprocess must set FDs as follows:
	// 3: read end of trigger pipe
	// 4: write end of ack pipe
	triggerFile := os.NewFile(uintptr(3), "liveness_trigger")
	ackFile := os.NewFile(uintptr(4), "liveness_ack")

	go func() {
		defer triggerFile.Close()
		defer ackFile.Close()

		buf := make([]byte, 1)
		for {
			_, err := triggerFile.Read(buf)
			if err != nil {
				return // Pipe closed, exit loop
			}

			ackFile.Write(buf)
		}
	}()
}
