# C integration harness

Run from the repository root:

```sh
./scripts/check-c-api.sh
```

The script builds a host-only static library and compiles `harness.c` with the
system C compiler. The harness allocates aligned context storage, sends framed
Hello/Info requests, validates CBOR responses and checks buffer, reentry,
malformed-frame, log and reset behavior.

`escope-c-api` itself is an allocation-free `no_std` rlib. The small Rust wrapper
here uses `std` to provide a host runtime for the executable; a firmware static
library wrapper must supply the target's panic handler and linker integration.
The installed C header is `crates/escope-c-api/include/escope.h`.
