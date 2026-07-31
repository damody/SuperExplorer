#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )
)]
//! Typed search parsing, query binding, result merging, and bounded filesystem fallback.
#![allow(
    clippy::must_use_candidate,
    reason = "query constructors and observations are pure values without resource-loss semantics"
)]

mod address;
mod engine;
mod local_index;
mod parser;

pub use address::{AddressParseError, parse_address};
pub use engine::{
    BackendDiagnostic, DedupeStore, FallbackConfig, SearchBatch, SearchHit, SearchMetrics,
    SearchOutcome, SearchRequest, SearchSource, SearchSourceState, search_filesystem,
};
pub use local_index::{LazyIndex, LazyIndexConfig, default_index_path, matches_entry};
pub use parser::{
    BoundQuery, Comparison, DateValue, Expr, ParseError, PropertyKey, QueryParameter, SizeValue,
    Span, Token, TokenKind, Value, bind_query, parse,
};
