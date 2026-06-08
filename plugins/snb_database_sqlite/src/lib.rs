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

    #[cfg(test)]
    fn new_memory(name: &str) -> Self {
        let conn = Connection::open_in_memory().expect("failed to open in-memory SQLite database");
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
mod tests {
    use super::*;
    use snb_core::database::{ColumnType, Condition, DatabaseOps, Order};

    fn test_db() -> SqliteDatabase {
        SqliteDatabase::new_memory("test")
    }

    #[test]
    fn test_create_table_and_insert() {
        let db = test_db();

        db.table("users")
            .column("id", ColumnType::Integer, true, true, true, None)
            .column("name", ColumnType::Text, false, true, false, None)
            .column("age", ColumnType::Integer, false, false, false, None)
            .if_not_exists()
            .execute()
            .unwrap();

        let result = db
            .insert("users")
            .set("name", Value::Text("Alice".into()))
            .set("age", Value::Integer(30))
            .execute()
            .unwrap();
        assert_eq!(result.rows_affected, 1);
        assert_eq!(result.last_insert_id, Some(1));

        db.insert("users")
            .set("name", Value::Text("Bob".into()))
            .set("age", Value::Integer(25))
            .execute()
            .unwrap();
    }

    #[test]
    fn test_select_with_conditions() {
        let db = test_db();

        db.table("items")
            .column("id", ColumnType::Integer, true, true, true, None)
            .column("name", ColumnType::Text, false, true, false, None)
            .column("price", ColumnType::Real, false, false, false, None)
            .execute()
            .unwrap();

        db.insert("items")
            .set("name", Value::Text("Apple".into()))
            .set("price", Value::Real(1.5))
            .execute()
            .unwrap();
        db.insert("items")
            .set("name", Value::Text("Banana".into()))
            .set("price", Value::Real(0.8))
            .execute()
            .unwrap();
        db.insert("items")
            .set("name", Value::Text("Cherry".into()))
            .set("price", Value::Real(3.0))
            .execute()
            .unwrap();

        // Select all
        let rows = db.select("items").execute().unwrap();
        assert_eq!(rows.len(), 3);

        // Select with WHERE
        let rows = db
            .select("items")
            .where_(Condition::Gt("price".into(), Value::Real(1.0)))
            .execute()
            .unwrap();
        assert_eq!(rows.len(), 2);

        // Select with ORDER BY + LIMIT
        let rows = db
            .select("items")
            .order_by(Order::Desc("price".into()))
            .limit(1)
            .execute()
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].get("name"), Some(&Value::Text("Cherry".into())));

        // Select specific columns
        let rows = db
            .select("items")
            .column("name")
            .where_(Condition::Eq("name".into(), Value::Text("Apple".into())))
            .execute()
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].get("name"), Some(&Value::Text("Apple".into())));
    }

    #[test]
    fn test_update_and_delete() {
        let db = test_db();

        db.table("tasks")
            .column("id", ColumnType::Integer, true, true, true, None)
            .column("title", ColumnType::Text, false, true, false, None)
            .column("done", ColumnType::Boolean, false, false, false, None)
            .execute()
            .unwrap();

        db.insert("tasks")
            .set("title", Value::Text("Buy milk".into()))
            .set("done", Value::Integer(0))
            .execute()
            .unwrap();

        // Update
        let result = db
            .update("tasks")
            .set("done", Value::Integer(1))
            .where_(Condition::Eq("id".into(), Value::Integer(1)))
            .execute()
            .unwrap();
        assert_eq!(result.rows_affected, 1);

        let rows = db
            .select("tasks")
            .where_(Condition::Eq("id".into(), Value::Integer(1)))
            .execute()
            .unwrap();
        assert_eq!(rows[0].get("done"), Some(&Value::Integer(1)));

        // Delete
        let result = db
            .delete("tasks")
            .where_(Condition::Eq("id".into(), Value::Integer(1)))
            .execute()
            .unwrap();
        assert_eq!(result.rows_affected, 1);

        let rows = db.select("tasks").execute().unwrap();
        assert_eq!(rows.len(), 0);
    }

    #[test]
    fn test_complex_conditions() {
        let db = test_db();

        db.table("products")
            .column("id", ColumnType::Integer, true, true, true, None)
            .column("name", ColumnType::Text, false, true, false, None)
            .column("category", ColumnType::Text, false, false, false, None)
            .column("price", ColumnType::Real, false, false, false, None)
            .execute()
            .unwrap();

        for (name, cat, price) in [
            ("Laptop", "electronics", 999.99),
            ("Phone", "electronics", 699.99),
            ("Shirt", "clothing", 29.99),
            ("Pants", "clothing", 49.99),
            ("Tablet", "electronics", 499.99),
        ] {
            db.insert("products")
                .set("name", Value::Text(name.into()))
                .set("category", Value::Text(cat.into()))
                .set("price", Value::Real(price))
                .execute()
                .unwrap();
        }

        // AND condition
        let rows = db
            .select("products")
            .where_(Condition::And(vec![
                Condition::Eq("category".into(), Value::Text("electronics".into())),
                Condition::Gt("price".into(), Value::Real(500.0)),
            ]))
            .execute()
            .unwrap();
        assert_eq!(rows.len(), 2);

        // OR condition
        let rows = db
            .select("products")
            .where_(Condition::Or(vec![
                Condition::Eq("category".into(), Value::Text("clothing".into())),
                Condition::Lt("price".into(), Value::Real(700.0)),
            ]))
            .execute()
            .unwrap();
        assert_eq!(rows.len(), 4); // Shirt, Pants, Phone(699.99), Tablet(499.99)

        // IN condition
        let rows = db
            .select("products")
            .where_(Condition::In(
                "name".into(),
                vec![Value::Text("Laptop".into()), Value::Text("Shirt".into())],
            ))
            .execute()
            .unwrap();
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn test_transaction() {
        let db = test_db();

        db.table("accounts")
            .column("id", ColumnType::Integer, true, true, true, None)
            .column("balance", ColumnType::Real, false, false, false, None)
            .execute()
            .unwrap();

        db.insert("accounts")
            .set("balance", Value::Real(100.0))
            .execute()
            .unwrap();

        // Rollback
        db.begin_transaction().unwrap();
        db.update("accounts")
            .set("balance", Value::Real(200.0))
            .where_(Condition::Eq("id".into(), Value::Integer(1)))
            .execute()
            .unwrap();
        db.rollback().unwrap();

        let rows = db.select("accounts").execute().unwrap();
        assert_eq!(rows[0].get("balance"), Some(&Value::Real(100.0)));

        // Commit
        db.begin_transaction().unwrap();
        db.update("accounts")
            .set("balance", Value::Real(200.0))
            .where_(Condition::Eq("id".into(), Value::Integer(1)))
            .execute()
            .unwrap();
        db.commit().unwrap();

        let rows = db.select("accounts").execute().unwrap();
        assert_eq!(rows[0].get("balance"), Some(&Value::Real(200.0)));
    }

    #[test]
    fn test_drop_table() {
        let db = test_db();

        db.table("temp")
            .column("id", ColumnType::Integer, true, true, true, None)
            .execute()
            .unwrap();

        db.insert("temp")
            .set("id", Value::Integer(1))
            .execute()
            .unwrap();
        assert_eq!(db.select("temp").execute().unwrap().len(), 1);

        db.drop_table("temp").unwrap();

        // After drop, select should fail (table doesn't exist)
        assert!(db.select("temp").execute().is_err());
    }
}
