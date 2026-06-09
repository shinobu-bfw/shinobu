//! SQLite database driver plugin for the Shinobu bot framework.
//!
//! Implements [`DatabaseDriver`](snb_core::database::DatabaseDriver) backed by
//! [`rusqlite`] and registers itself so other plugins can use the
//! [`DatabaseOps`](snb_core::database::DatabaseOps) builder API.

use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::types::Value as RusqliteValue;
use rusqlite::{Connection, params_from_iter};
use snb_core::database::{ColumnType, DatabaseDriver, QueryResult, Row, Value};
use snb_macros::{database, plugin};

// -- SQLite database driver ---------------------------------------------------

struct SqliteDatabase {
    name: String,
    conn: Mutex<Connection>,
}

impl SqliteDatabase {
    fn new(name: &str, path: PathBuf) -> Self {
        std::fs::create_dir_all(path.parent().unwrap()).ok();
        let conn = Connection::open(&path).expect("failed to open SQLite database");
        conn.execute_batch("PRAGMA foreign_keys=ON;")
            .expect("failed to set SQLite pragmas");
        Self {
            name: name.to_string(),
            conn: Mutex::new(conn),
        }
    }
}

fn to_rusqlite_value(v: &Value) -> RusqliteValue {
    match v {
        Value::Null => RusqliteValue::Null,
        Value::Integer(i) => RusqliteValue::Integer(*i),
        Value::Real(f) => RusqliteValue::Real(*f),
        Value::Text(s) => RusqliteValue::Text(s.clone()),
        Value::Blob(b) => RusqliteValue::Blob(b.clone()),
    }
}

fn from_rusqlite_value(v: &rusqlite::types::Value) -> Value {
    match v {
        rusqlite::types::Value::Null => Value::Null,
        rusqlite::types::Value::Integer(i) => Value::Integer(*i),
        rusqlite::types::Value::Real(f) => Value::Real(*f),
        rusqlite::types::Value::Text(s) => Value::Text(s.clone()),
        rusqlite::types::Value::Blob(b) => Value::Blob(b.clone()),
    }
}

impl DatabaseDriver for SqliteDatabase {
    fn name(&self) -> &str {
        &self.name
    }

    fn column_type_sql(&self, ct: ColumnType) -> &str {
        match ct {
            ColumnType::Integer => "INTEGER",
            ColumnType::Real => "REAL",
            ColumnType::Text => "TEXT",
            ColumnType::Blob => "BLOB",
            ColumnType::Boolean => "INTEGER",
        }
    }

    fn placeholder(&self, index: usize) -> String {
        format!("?{index}")
    }

    fn exec_raw(&self, sql: &str, params: &[Value]) -> anyhow::Result<QueryResult> {
        let conn = self.conn.lock().unwrap();
        let rusqlite_params: Vec<RusqliteValue> = params.iter().map(to_rusqlite_value).collect();
        let mut stmt = conn.prepare(sql)?;
        let rows_affected = stmt.execute(params_from_iter(rusqlite_params.iter()))?;
        Ok(QueryResult {
            rows_affected: rows_affected as u64,
            last_insert_id: Some(conn.last_insert_rowid()),
        })
    }

    fn query_raw(&self, sql: &str, params: &[Value]) -> anyhow::Result<Vec<Row>> {
        let conn = self.conn.lock().unwrap();
        let rusqlite_params: Vec<RusqliteValue> = params.iter().map(to_rusqlite_value).collect();
        let mut stmt = conn.prepare(sql)?;
        let columns: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
        let mut rows = Vec::new();
        let mut query_rows = stmt.query(params_from_iter(rusqlite_params.iter()))?;
        while let Some(sqlite_row) = query_rows.next()? {
            let mut values = Vec::new();
            for i in 0..columns.len() {
                let val: rusqlite::types::Value = sqlite_row.get(i)?;
                values.push(from_rusqlite_value(&val));
            }
            rows.push(Row {
                columns: columns.clone(),
                values,
            });
        }
        Ok(rows)
    }

    fn drop_table(&self, name: &str) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(&format!("DROP TABLE IF EXISTS \"{name}\""))?;
        Ok(())
    }

    fn begin_transaction(&self) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch("BEGIN TRANSACTION")?;
        Ok(())
    }

    fn commit(&self) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch("COMMIT")?;
        Ok(())
    }

    fn rollback(&self) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch("ROLLBACK")?;
        Ok(())
    }
}

// -- Driver registration ------------------------------------------------------

/// Builds the SQLite driver. Runs during `register_all` (after `set_bot`), so it
/// may read the plugin's data directory from the context.
#[database]
fn sqlite_driver() -> SqliteDatabase {
    let plugin = snb_core::context::PluginHelper::new("sqlite");
    let db_path = plugin.data_dir().join("data.db");
    SqliteDatabase::new("sqlite", db_path)
}

// -- Plugin -------------------------------------------------------------------

#[plugin(name = "sqlite", version = "0.1.0", kind = DatabaseDriver)]
struct SqlitePlugin;

// -- Unit tests ---------------------------------------------------------------

#[cfg(test)]
#[path = "../tests/unit/lib_tests.rs"]
mod lib_tests;
