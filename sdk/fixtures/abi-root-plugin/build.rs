fn main() {
    let panic_strategy = std::env::var("CARGO_CFG_PANIC").unwrap_or_default();
    assert_eq!(
        panic_strategy, "unwind",
        "the ABI root fixture requires panic=unwind; RUSTFLAGS must not select panic=abort"
    );
}
