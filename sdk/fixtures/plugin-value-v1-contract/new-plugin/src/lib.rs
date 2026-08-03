//! DLL half of the V1 plugin-value ownership and validation contract.

#![allow(unsafe_code, reason = "the fixture counts allocations in its own DLL")]

use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicU64, Ordering},
};

use abi_stable::std_types::{ROption, RString, RVec};
use explorer_extension_api::{
    EXTENSION_ID_NAMESPACE_V1, PluginItemOutcomeV1, PluginItemResultV1, PluginValueKindV1,
    PluginValueV1, StableIdV1, StableSortValueKindV1, StableSortValueV1, MAX_PLUGIN_VALUE_BYTES_V1,
};

struct CountingAllocator;
static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static DEALLOCATIONS: AtomicU64 = AtomicU64::new(0);

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() { ALLOCATIONS.fetch_add(1, Ordering::Relaxed); }
        pointer
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) };
        DEALLOCATIONS.fetch_add(1, Ordering::Relaxed);
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

#[repr(C)]
pub struct AllocatorSnapshotV1 { pub allocations: u64, pub deallocations: u64 }

#[unsafe(no_mangle)]
pub extern "C" fn superexplorer_plugin_value_v1_allocator_reset() {
    ALLOCATIONS.store(0, Ordering::Relaxed);
    DEALLOCATIONS.store(0, Ordering::Relaxed);
}
#[unsafe(no_mangle)]
pub extern "C" fn superexplorer_plugin_value_v1_allocator_snapshot() -> AllocatorSnapshotV1 {
    AllocatorSnapshotV1 { allocations: ALLOCATIONS.load(Ordering::Relaxed), deallocations: DEALLOCATIONS.load(Ordering::Relaxed) }
}

fn text_value() -> PluginValueV1 { PluginValueV1::text(RString::from("copy-me")).expect("fixture text") }
fn value(case: u32) -> PluginValueV1 {
    match case {
        0 => PluginValueV1::boolean(true),
        1 => PluginValueV1::integer(-7),
        2 => PluginValueV1::float(1.5).expect("fixture float"),
        3 => PluginValueV1::bytes(RVec::from(vec![1, 2])).expect("fixture bytes"),
        4 => PluginValueV1::time_unix_nanos(-3),
        5 => PluginValueV1::duration_nanos(7).expect("fixture duration"),
        6 => text_value(),
        7 => PluginValueV1::localized_text(RString::from("localized")).expect("fixture localized"),
        8 => PluginValueV1::structured_canonical_json(RVec::from(vec![b'[', b'1', b']'])).expect("fixture json"),
        9 => PluginValueV1::opaque(StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 71), 1, RVec::from(vec![9])).expect("fixture opaque"),
        _ => text_value(),
    }
}
fn sort(case: u32) -> StableSortValueV1 {
    match case {
        10 => StableSortValueV1::boolean(true),
        11 => StableSortValueV1::integer(-1),
        12 => StableSortValueV1::unsigned((1_u64 << 53) + 1),
        13 => StableSortValueV1::float(2.5).expect("fixture sort float"),
        14 => StableSortValueV1::time_unix_nanos(-1),
        15 => StableSortValueV1::duration_nanos(2),
        16 => StableSortValueV1::text(RString::from("sort")).expect("fixture sort text"),
        _ => StableSortValueV1::bytes(RVec::from(vec![3])).expect("fixture sort bytes"),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn superexplorer_plugin_value_v1_contract_case(case: u32) -> PluginItemResultV1 {
    if (20..=24).contains(&case) {
        let outcome = match case { 20 => PluginItemOutcomeV1::UNSUPPORTED, 21 => PluginItemOutcomeV1::UNAVAILABLE, 22 => PluginItemOutcomeV1::CANCELLED, 23 => PluginItemOutcomeV1::PLUGIN_ERROR, _ => PluginItemOutcomeV1::INCOMPATIBLE };
        return PluginItemResultV1::absent(outcome);
    }
    if case == 30 {
        let mut invalid = text_value(); invalid.reserved = 1;
        return PluginItemResultV1::value(invalid, ROption::RNone);
    }
    if case == 31 {
        let mut invalid = text_value(); invalid.kind = PluginValueKindV1::from_raw(99);
        return PluginItemResultV1::value(invalid, ROption::RNone);
    }
    if case == 32 {
        let invalid_sort = StableSortValueV1 { kind: StableSortValueKindV1::F64, reserved: 0, signed: 0, unsigned: 0, float: -0.0, text: RString::new(), bytes: RVec::new(), reserved_tail: 0 };
        return PluginItemResultV1::value(text_value(), ROption::RSome(invalid_sort));
    }
    if case == 33 { return PluginItemResultV1::value(PluginValueV1 { kind: PluginValueKindV1::BYTES, reserved: 0, integer: 0, float: 0.0, text: RString::new(), payload: RVec::from(vec![0; MAX_PLUGIN_VALUE_BYTES_V1 + 1]), opaque_schema: StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 0), opaque_schema_version: 0, reserved_tail: 0 }, ROption::RNone); }
    if case == 34 { return PluginItemResultV1::value(PluginValueV1 { kind: PluginValueKindV1::OPAQUE, reserved: 0, integer: 0, float: 0.0, text: RString::new(), payload: RVec::from(vec![1]), opaque_schema: StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 71), opaque_schema_version: 0, reserved_tail: 0 }, ROption::RNone); }
    if case == 35 { let invalid_sort = StableSortValueV1 { kind: StableSortValueKindV1::I64, reserved: 1, signed: 1, unsigned: 0, float: 0.0, text: RString::new(), bytes: RVec::new(), reserved_tail: 0 }; return PluginItemResultV1::value(text_value(), ROption::RSome(invalid_sort)); }
    if (10..=17).contains(&case) { return PluginItemResultV1::value(text_value(), ROption::RSome(sort(case))); }
    PluginItemResultV1::value(value(case), ROption::RNone)
}
