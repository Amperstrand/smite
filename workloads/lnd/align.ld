/* Gives Go's libfuzzer counter section its own pages, so sancov.go can map
 * AFL's shared memory over them without touching neighboring data. */
SECTIONS
{
  .go.fuzzcntrs ALIGN(4096) : { *(.go.fuzzcntrs) . = ALIGN(4096); }
}
INSERT AFTER .bss;
