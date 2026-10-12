//! The component library index. CI builds it from the KiCad symbol and footprint libraries into
//! one SQLite file; the app downloads that file and searches it.

mod builder;
mod error;
mod index;
mod kicad;
pub mod kicad_mod;
pub mod kicad_sym;
pub mod preview;
mod search;

pub use builder::{BuildReport, build_index};
pub use error::LibraryError;
pub use index::{
    ATTRIBUTION, Index, ItemKind, META_ATTRIBUTION, META_FOOTPRINT_COUNT, META_KICAD_TAG,
    META_SYMBOL_COUNT, SCHEMA_VERSION, SearchHit,
};
pub use kicad_mod::{FootprintRecord, Mount};
pub use kicad_sym::SymbolRecord;
pub use search::match_expression;
