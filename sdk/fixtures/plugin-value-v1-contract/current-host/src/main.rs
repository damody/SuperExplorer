//! Host half of the V1 plugin-value DLL contract.
#![allow(unsafe_code, reason = "the fixture loads only its locally-built DLL")]
use std::{mem::{align_of, offset_of, size_of}, path::Path};
use abi_stable::std_types::ROption;
use explorer_extension_api::{PluginItemOutcomeV1, PluginItemResultV1, PluginValueKindV1, StableSortValueKindV1};
use libloading::{Library, Symbol};

#[repr(C)] struct AllocatorSnapshotV1 { allocations: u64, deallocations: u64 }
type Reset = unsafe extern "C" fn();
type Snapshot = unsafe extern "C" fn() -> AllocatorSnapshotV1;
type Case = unsafe extern "C" fn(u32) -> PluginItemResultV1;

fn expected_sort(case: u32) -> ROption<StableSortValueKindV1> {
    ROption::RSome(match case { 10 => StableSortValueKindV1::BOOL, 11 => StableSortValueKindV1::I64, 12 => StableSortValueKindV1::U64, 13 => StableSortValueKindV1::F64, 14 => StableSortValueKindV1::TIME_UNIX_NANOS, 15 => StableSortValueKindV1::DURATION_NANOS, 16 => StableSortValueKindV1::TEXT, _ => StableSortValueKindV1::BYTES })
}
fn verify_layout() -> Result<(), String> {
    let checks = [
        ("PluginValueKindV1", size_of::<PluginValueKindV1>(), align_of::<PluginValueKindV1>(), 4, 4),
        ("StableSortValueKindV1", size_of::<StableSortValueKindV1>(), align_of::<StableSortValueKindV1>(), 4, 4),
        ("PluginItemOutcomeV1", size_of::<PluginItemOutcomeV1>(), align_of::<PluginItemOutcomeV1>(), 4, 4),
        ("PluginItemResultV1", size_of::<PluginItemResultV1>(), align_of::<PluginItemResultV1>(), 248, 8),
    ];
    for (name, size, align, expected_size, expected_align) in checks { if size != expected_size || align != expected_align { return Err(format!("{name} layout {size}/{align}")); } }
    if offset_of!(PluginItemResultV1, outcome) != 0 || offset_of!(PluginItemResultV1, value) != 8 || offset_of!(PluginItemResultV1, stable_sort) != 128 || offset_of!(PluginItemResultV1, reserved) != 240 { return Err("PluginItemResultV1 offsets changed".to_owned()); }
    if PluginValueKindV1::OPAQUE.into_raw() != 10 || StableSortValueKindV1::BYTES.into_raw() != 8 || PluginItemOutcomeV1::INCOMPATIBLE.into_raw() != 6 { return Err("fixed V1 numeric codes changed".to_owned()); }
    Ok(())
}
fn verify(plugin: &Path) -> Result<(), String> {
    let library = unsafe { Library::new(plugin) }.map_err(|e| e.to_string())?;
    let reset: Symbol<'_, Reset> = unsafe { library.get(b"superexplorer_plugin_value_v1_allocator_reset\0") }.map_err(|e| e.to_string())?;
    let snapshot: Symbol<'_, Snapshot> = unsafe { library.get(b"superexplorer_plugin_value_v1_allocator_snapshot\0") }.map_err(|e| e.to_string())?;
    let case_fn: Symbol<'_, Case> = unsafe { library.get(b"superexplorer_plugin_value_v1_contract_case\0") }.map_err(|e| e.to_string())?;
    unsafe { reset() };
    for case in 0..=17 { let result = unsafe { case_fn(case) }; let copied = result.clone(); if copied.validate_transport(if case >= 10 { expected_sort(case) } else { ROption::RNone }).is_err() { return Err(format!("valid DLL case {case} was rejected")); } if case == 9 { let value = match copied.value { ROption::RSome(ref value) => value, ROption::RNone => return Err("opaque DLL value missing".to_owned()) }; if value.opaque_schema.value != 71 || value.opaque_schema_version != 1 { return Err("opaque schema/version binding changed".to_owned()); } } if case == 12 { let sort = match copied.stable_sort { ROption::RSome(ref sort) => sort, ROption::RNone => return Err("exact integer sort missing".to_owned()) }; if sort.unsigned != (1_u64 << 53) + 1 { return Err("sort integer lost >2^53 precision".to_owned()); } } drop(copied); drop(result); }
    for case in 20..=24 { let result = unsafe { case_fn(case) }; if result.validate_transport(ROption::RNone).is_err() { return Err(format!("outcome DLL case {case} was rejected")); } }
    for case in 30..=35 { let result = unsafe { case_fn(case) }; let sort = if case == 32 { ROption::RSome(StableSortValueKindV1::F64) } else if case == 35 { ROption::RSome(StableSortValueKindV1::I64) } else { ROption::RNone }; if result.validate_transport(sort).is_ok() { return Err(format!("invalid DLL case {case} was accepted")); } }
    let audit = unsafe { snapshot() }; if audit.allocations == 0 || audit.allocations != audit.deallocations { return Err(format!("DLL allocation cleanup mismatch {}/{}", audit.allocations, audit.deallocations)); }
    Ok(())
}
fn main() -> Result<(), String> { let plugin = std::env::args_os().nth(1).ok_or("missing plugin path")?; verify_layout()?; verify(Path::new(&plugin)) }
