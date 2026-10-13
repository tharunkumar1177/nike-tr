//! The SQLite index file: its schema, the writer used by the build, and the reader used by the
//! app.

use std::path::Path;

use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, Type, ValueRef};
use rusqlite::{Connection, OpenFlags, OptionalExtension, Row, params};
use serde::de::DeserializeOwned;
use tron_core::sexpr::{self, SExpr};

use crate::builder::BuildReport;
use crate::error::LibraryError;
use crate::kicad_mod::{FootprintRecord, Mount};
use crate::kicad_sym::{SymbolRecord, flatten};
use crate::search::match_expression;

/// Stored as the database `user_version`. Bump it whenever the schema or the preview JSON
/// changes shape, so the app refuses an index it cannot read.
pub const SCHEMA_VERSION: i64 = 2;

/// The attribution the KiCad library license requires wherever the index is shown.
pub const ATTRIBUTION: &str = "Symbols and footprints from the KiCad libraries \
    (https://gitlab.com/kicad/libraries), licensed CC-BY-SA 4.0 with the KiCad libraries \
    exception.";

pub const META_KICAD_TAG: &str = "kicad_tag";
pub const META_SYMBOL_COUNT: &str = "symbol_count";
pub const META_FOOTPRINT_COUNT: &str = "footprint_count";
pub const META_ATTRIBUTION: &str = "attribution";

const SCHEMA: &str = "
CREATE TABLE meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
) WITHOUT ROWID;

CREATE TABLE symbols (
    id INTEGER PRIMARY KEY,
    lib TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL,
    keywords TEXT NOT NULL,
    datasheet TEXT NOT NULL,
    reference TEXT NOT NULL,
    footprint TEXT NOT NULL,
    footprint_filters TEXT NOT NULL,
    extends TEXT,
    unit_count INTEGER NOT NULL,
    pin_count INTEGER NOT NULL,
    power INTEGER NOT NULL,
    preview TEXT NOT NULL,
    source TEXT NOT NULL,
    UNIQUE (lib, name)
);

CREATE TABLE footprints (
    id INTEGER PRIMARY KEY,
    lib TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL,
    tags TEXT NOT NULL,
    pad_count INTEGER NOT NULL,
    mount TEXT NOT NULL,
    preview TEXT NOT NULL,
    UNIQUE (lib, name)
);

CREATE VIRTUAL TABLE search USING fts5(
    kind UNINDEXED,
    item UNINDEXED,
    name,
    lib,
    keywords,
    description,
    tokenize = 'unicode61 remove_diacritics 2',
    prefix = '1 2 3'
);
";

const INSERT_SYMBOL: &str = "
INSERT INTO symbols (
    lib, name, description, keywords, datasheet, reference, footprint, footprint_filters,
    extends, unit_count, pin_count, power, preview, source
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)";

const INSERT_FOOTPRINT: &str = "
INSERT INTO footprints (lib, name, description, tags, pad_count, mount, preview)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)";

const INSERT_SEARCH: &str = "
INSERT INTO search (kind, item, name, lib, keywords, description)
VALUES (?1, ?2, ?3, ?4, ?5, ?6)";

const INSERT_META: &str = "INSERT INTO meta (key, value) VALUES (?1, ?2)";

const SELECT_META: &str = "SELECT value FROM meta WHERE key = ?1";

/// Exact name matches come first, then BM25 relevance with names weighted above keywords,
/// libraries, and descriptions. The two zero weights are for the unindexed columns.
const SEARCH: &str = "
SELECT kind, item, name, lib, description
FROM search
WHERE search MATCH ?1
ORDER BY (name = ?2 COLLATE NOCASE) DESC, bm25(search, 0, 0, 10, 2, 5, 1), name
LIMIT ?3";

const SELECT_SYMBOL: &str = "
SELECT lib, name, description, keywords, datasheet, reference, footprint, footprint_filters,
    extends, unit_count, pin_count, power, preview, source
FROM symbols
WHERE id = ?1";

const SELECT_DEFINITION: &str = "SELECT lib, name, extends, source FROM symbols WHERE id = ?1";

const SELECT_PARENT: &str = "SELECT extends, source FROM symbols WHERE lib = ?1 AND name = ?2";

const SELECT_FOOTPRINT: &str = "
SELECT lib, name, description, tags, pad_count, mount, preview
FROM footprints
WHERE id = ?1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ItemKind {
    Symbol,
    Footprint,
}

impl ItemKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ItemKind::Symbol => "symbol",
            ItemKind::Footprint => "footprint",
        }
    }
}

impl FromSql for ItemKind {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<ItemKind> {
        match value.as_str()? {
            "symbol" => Ok(ItemKind::Symbol),
            "footprint" => Ok(ItemKind::Footprint),
            _ => Err(FromSqlError::InvalidType),
        }
    }
}

impl FromSql for Mount {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Mount> {
        Mount::from_name(value.as_str()?).ok_or(FromSqlError::InvalidType)
    }
}

/// One search result. `id` selects the full record through [`Index::symbol`] or
/// [`Index::footprint`], depending on `kind`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    pub kind: ItemKind,
    pub id: i64,
    pub name: String,
    pub lib: String,
    pub description: String,
}

/// Writes a new index inside one transaction.
pub(crate) struct Writer {
    conn: Connection,
}

impl Writer {
    pub(crate) fn create(path: &Path) -> Result<Writer, LibraryError> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA synchronous = OFF")?;
        conn.execute_batch(SCHEMA)?;
        conn.execute_batch("BEGIN")?;
        Ok(Writer { conn })
    }

    pub(crate) fn add_symbol(&self, record: &SymbolRecord) -> Result<(), LibraryError> {
        let preview = serde_json::to_string(&record.preview)?;
        let mut insert = self.conn.prepare_cached(INSERT_SYMBOL)?;
        insert.execute(params![
            record.lib,
            record.name,
            record.description,
            record.keywords,
            record.datasheet,
            record.reference,
            record.footprint,
            record.footprint_filters,
            record.extends,
            record.unit_count,
            record.pin_count,
            record.power,
            preview,
            record.source,
        ])?;
        let id = self.conn.last_insert_rowid();
        let mut search = self.conn.prepare_cached(INSERT_SEARCH)?;
        search.execute(params![
            ItemKind::Symbol.as_str(),
            id,
            record.name,
            record.lib,
            record.keywords,
            record.description,
        ])?;
        Ok(())
    }

    pub(crate) fn add_footprint(&self, record: &FootprintRecord) -> Result<(), LibraryError> {
        let preview = serde_json::to_string(&record.preview)?;
        let mut insert = self.conn.prepare_cached(INSERT_FOOTPRINT)?;
        insert.execute(params![
            record.lib,
            record.name,
            record.description,
            record.tags,
            record.pad_count,
            record.mount.as_str(),
            preview,
        ])?;
        let id = self.conn.last_insert_rowid();
        let mut search = self.conn.prepare_cached(INSERT_SEARCH)?;
        search.execute(params![
            ItemKind::Footprint.as_str(),
            id,
            record.name,
            record.lib,
            record.tags,
            record.description,
        ])?;
        Ok(())
    }

    /// Records the build metadata, commits, and compacts the file.
    pub(crate) fn finish(self, kicad_tag: &str, report: &BuildReport) -> Result<(), LibraryError> {
        let symbols = report.symbols.to_string();
        let footprints = report.footprints.to_string();
        let meta = [
            (META_KICAD_TAG, kicad_tag),
            (META_SYMBOL_COUNT, symbols.as_str()),
            (META_FOOTPRINT_COUNT, footprints.as_str()),
            (META_ATTRIBUTION, ATTRIBUTION),
        ];
        {
            let mut insert = self.conn.prepare_cached(INSERT_META)?;
            for (key, value) in meta {
                insert.execute([key, value])?;
            }
        }
        self.conn.execute_batch("COMMIT")?;
        self.conn
            .execute("INSERT INTO search (search) VALUES ('optimize')", ())?;
        self.conn
            .execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION}"))?;
        self.conn.execute_batch("VACUUM")?;
        self.conn
            .close()
            .map_err(|(_, err)| LibraryError::Database(err))
    }
}

/// A read-only handle on a built index.
pub struct Index {
    conn: Connection,
}

impl Index {
    /// Opens the index at `path`, rejecting files that are not an index of [`SCHEMA_VERSION`].
    pub fn open(path: &Path) -> Result<Index, LibraryError> {
        let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let conn = Connection::open_with_flags(path, flags)?;
        let found: i64 = conn.query_row("PRAGMA user_version", (), |row| row.get(0))?;
        if found != SCHEMA_VERSION {
            return Err(LibraryError::SchemaMismatch {
                found,
                expected: SCHEMA_VERSION,
            });
        }
        Ok(Index { conn })
    }

    pub fn meta(&self, key: &str) -> Result<Option<String>, LibraryError> {
        let mut select = self.conn.prepare_cached(SELECT_META)?;
        Ok(select.query_row([key], |row| row.get(0)).optional()?)
    }

    pub fn item_count(&self, kind: ItemKind) -> Result<usize, LibraryError> {
        let sql = match kind {
            ItemKind::Symbol => "SELECT COUNT(*) FROM symbols",
            ItemKind::Footprint => "SELECT COUNT(*) FROM footprints",
        };
        let count: i64 = self.conn.query_row(sql, (), |row| row.get(0))?;
        Ok(usize::try_from(count).unwrap_or(0))
    }

    /// Returns up to `limit` symbols and footprints matching every term of `query` as a word
    /// prefix, best match first. Input with nothing searchable returns no results.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>, LibraryError> {
        let Some(expression) = match_expression(query) else {
            return Ok(Vec::new());
        };
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let mut select = self.conn.prepare_cached(SEARCH)?;
        let hits = select
            .query_map(params![expression, query.trim(), limit], |row| {
                Ok(SearchHit {
                    kind: row.get(0)?,
                    id: row.get(1)?,
                    name: row.get(2)?,
                    lib: row.get(3)?,
                    description: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(hits)
    }

    pub fn symbol(&self, id: i64) -> Result<Option<SymbolRecord>, LibraryError> {
        let mut select = self.conn.prepare_cached(SELECT_SYMBOL)?;
        let record = select
            .query_row([id], |row| {
                Ok(SymbolRecord {
                    lib: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    keywords: row.get(3)?,
                    datasheet: row.get(4)?,
                    reference: row.get(5)?,
                    footprint: row.get(6)?,
                    footprint_filters: row.get(7)?,
                    extends: row.get(8)?,
                    unit_count: row.get(9)?,
                    pin_count: row.get(10)?,
                    power: row.get(11)?,
                    preview: json_column(row, 12)?,
                    source: row.get(13)?,
                })
            })
            .optional()?;
        Ok(record)
    }

    /// The symbol as a schematic embeds it in `(lib_symbols)`: merged with every symbol it
    /// extends and named `<lib>:<name>`. Ancestors are looked up in the symbol's own library.
    pub fn symbol_definition(&self, id: i64) -> Result<Option<SExpr>, LibraryError> {
        let mut select = self.conn.prepare_cached(SELECT_DEFINITION)?;
        let row: Option<(String, String, Stored)> = select
            .query_row([id], |row| Ok((row.get(0)?, row.get(1)?, stored(row, 2)?)))
            .optional()?;
        let Some((lib, name, own)) = row else {
            return Ok(None);
        };
        let invalid = |message: String| LibraryError::Definition {
            lib: lib.clone(),
            name: name.clone(),
            message,
        };

        let mut chain = vec![parse_source(&own.source).map_err(invalid)?];
        let mut visited = vec![name.clone()];
        let mut extends = own.extends;
        let mut select_parent = self.conn.prepare_cached(SELECT_PARENT)?;
        while let Some(parent) = extends {
            if visited.contains(&parent) {
                let message = format!("its extends chain loops back to {parent:?}");
                return Err(invalid(message));
            }
            let found = select_parent
                .query_row(params![lib, parent], |row| stored(row, 0))
                .optional()?;
            let Some(found) = found else {
                let message = format!("it extends {parent:?}, which is not in the index");
                return Err(invalid(message));
            };
            chain.push(parse_source(&found.source).map_err(invalid)?);
            visited.push(parent);
            extends = found.extends;
        }
        flatten(&lib, &chain).map(Some).map_err(invalid)
    }

    pub fn footprint(&self, id: i64) -> Result<Option<FootprintRecord>, LibraryError> {
        let mut select = self.conn.prepare_cached(SELECT_FOOTPRINT)?;
        let record = select
            .query_row([id], |row| {
                Ok(FootprintRecord {
                    lib: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    tags: row.get(3)?,
                    pad_count: row.get(4)?,
                    mount: row.get(5)?,
                    preview: json_column(row, 6)?,
                })
            })
            .optional()?;
        Ok(record)
    }
}

/// A symbol's own definition and the symbol it extends, if any.
struct Stored {
    extends: Option<String>,
    source: String,
}

fn stored(row: &Row<'_>, start: usize) -> rusqlite::Result<Stored> {
    Ok(Stored {
        extends: row.get(start)?,
        source: row.get(start + 1)?,
    })
}

fn parse_source(source: &str) -> Result<SExpr, String> {
    sexpr::parse(source).map_err(|err| format!("stored definition: {err}"))
}

fn json_column<T: DeserializeOwned>(row: &Row<'_>, index: usize) -> rusqlite::Result<T> {
    let text: String = row.get(index)?;
    serde_json::from_str(&text)
        .map_err(|err| rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(err)))
}
