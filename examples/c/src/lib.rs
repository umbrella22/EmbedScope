//! Host-only static library wrapper. std supplies the final panic/runtime
//! integration; the reusable escope-c-api dependency remains no_std.
pub use escope_c_api::*;

#[no_mangle]
pub extern "C" fn escope_c_example_anchor() -> u32 {
    escope_context_size()
}
