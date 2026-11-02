# escope-c-api

Allocation-free, `no_std` C ABI for the EmbedScope Agent. The header is
[`include/escope.h`](include/escope.h). A running C integration is available in
[`examples/c`](../../examples/c).

The caller allocates `escope_context_size()` bytes at
`escope_context_alignment()`, initializes the context once, and serializes all
calls, including log publishing. The library copies the callback table; callback
code and `user` data remain valid for the context's lifetime. Callback reentry
returns `ES_BUSY`; callbacks cannot unwind or retain request/reply buffers.

The transport loop follows these operations:

1. `escope_feed` consumes an input prefix through at most one complete frame.
   Always advance by `consumed`, including after `ES_PROTOCOL`.
2. `escope_take_tx` copies the pending response as a whole frame. Insufficient
   capacity preserves the response and reports its required size through
   `written`. The transport retains this copy until its own partial writes finish.
3. `escope_poll` moves one queued log event to the TX slot when it is empty.
4. `escope_reset` clears partial input, pending output, subscriptions and logs,
   invokes the backend reset callback, and advances the session. The next
   connection begins with a new Hello.

A full TX slot returns `ES_BUSY` from feed with `consumed=0`; the caller drains
TX before retrying the same input suffix. This bounded slot preserves control
responses under log traffic. Bad-frame errors end at a delimiter and the next
feed resumes with a clean frame collector.

`capabilities` and `info` callbacks are required. `handle` encodes a typed CBOR
reply for an operation and returns a protocol `ES_STATUS_*`; the ABI's
`ES_OK/ES_CAPACITY/ES_BUSY/...` return codes are a separate namespace. Callback
reply lengths beyond their supplied capacity become an Internal response.
Metadata string capacities and pointer lifetimes are specified in the header;
oversized identity fields produce a Capacity response instead of truncation.
Structured log prefixes are bounded and explicitly marked when truncated.

The rlib supplies no global panic handler and compiles for bare-metal RISC-V.
The C harness's static-library wrapper uses `std` for a host runtime; firmware
wrappers supply their target-specific panic handler, compiler and linker setup.

```sh
cargo check -p escope-c-api --target riscv32imafc-unknown-none-elf
./scripts/check-c-api.sh
```
