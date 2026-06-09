use super::*;
use std::sync::Mutex;

struct MockDriver {
    log: Mutex<Vec<(String, Vec<Value>)>>,
}

impl MockDriver {
    fn new() -> Self {
        Self {
            log: Mutex::new(Vec::new()),
        }
    }

    fn take_log(&self) -> Vec<(String, Vec<Value>)> {
        self.log.lock().unwrap().drain(..).collect()
    }
}

impl DatabaseDriver for MockDriver {
    fn name(&self) -> &str {
        "mock"
    }

    fn column_type_sql(&self, ct: ColumnType) -> &str {
        match ct {
            ColumnType::Integer => "INTEGER",
            ColumnType::Real => "REAL",
            ColumnType::Text => "TEXT",
            ColumnType::Blob => "BLOB",
            ColumnType::Boolean => "BOOLEAN",
        }
    }

    fn placeholder(&self, index: usize) -> String {
        format!("${index}")
    }

    fn exec_raw(&self, sql: &str, params: &[Value]) -> anyhow::Result<QueryResult> {
        self.log
            .lock()
            .unwrap()
            .push((sql.to_string(), params.to_vec()));
        Ok(QueryResult {
            rows_affected: 0,
            last_insert_id: None,
        })
    }

    fn query_raw(&self, sql: &str, params: &[Value]) -> anyhow::Result<Vec<Row>> {
        self.log
            .lock()
            .unwrap()
            .push((sql.to_string(), params.to_vec()));
        Ok(Vec::new())
    }

    fn drop_table(&self, _name: &str) -> anyhow::Result<()> {
        Ok(())
    }
    fn begin_transaction(&self) -> anyhow::Result<()> {
        Ok(())
    }
    fn commit(&self) -> anyhow::Result<()> {
        Ok(())
    }
    fn rollback(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

#[test]
fn value_display() {
    assert_eq!(Value::Null.to_string(), "NULL");
    assert_eq!(Value::Integer(42).to_string(), "42");
    assert_eq!(Value::Real(3.15).to_string(), "3.15");
    assert_eq!(Value::Text("hi".into()).to_string(), "hi");
    assert_eq!(Value::Blob(vec![1, 2, 3]).to_string(), "<blob 3 bytes>");
}

#[test]
fn row_get_existing_column() {
    let row = Row {
        columns: vec!["id".into(), "name".into()],
        values: vec![Value::Integer(1), Value::Text("Alice".into())],
    };
    assert_eq!(row.get("id"), Some(&Value::Integer(1)));
    assert_eq!(row.get("name"), Some(&Value::Text("Alice".into())));
}

#[test]
fn row_get_missing_column() {
    let row = Row {
        columns: vec!["id".into()],
        values: vec![Value::Integer(1)],
    };
    assert_eq!(row.get("missing"), None);
}

#[test]
fn table_builder_generates_create_table_sql() {
    let db = MockDriver::new();
    db.table("users")
        .column("id", ColumnType::Integer, true, true, true, None)
        .column("name", ColumnType::Text, false, true, false, None)
        .column(
            "score",
            ColumnType::Real,
            false,
            false,
            false,
            Some(Value::Real(0.0)),
        )
        .execute()
        .unwrap();

    let log = db.take_log();
    assert_eq!(log.len(), 1);
    let (sql, params) = &log[0];
    assert!(params.is_empty());
    assert!(sql.contains("CREATE TABLE \"users\""));
    assert!(sql.contains("\"id\" INTEGER PRIMARY KEY AUTOINCREMENT"));
    assert!(sql.contains("\"name\" TEXT NOT NULL"));
    assert!(sql.contains("\"score\" REAL DEFAULT 0"));
}

#[test]
fn table_builder_if_not_exists() {
    let db = MockDriver::new();
    db.table("t")
        .column("id", ColumnType::Integer, true, true, true, None)
        .if_not_exists()
        .execute()
        .unwrap();

    let log = db.take_log();
    assert!(log[0].0.contains("CREATE TABLE IF NOT EXISTS"));
}

#[test]
fn insert_builder_generates_correct_sql() {
    let db = MockDriver::new();
    db.insert("users")
        .set("name", Value::Text("Bob".into()))
        .set("age", Value::Integer(25))
        .execute()
        .unwrap();

    let log = db.take_log();
    assert_eq!(log.len(), 1);
    let (sql, params) = &log[0];
    assert!(sql.contains("INSERT INTO \"users\""));
    assert!(sql.contains("\"name\""));
    assert!(sql.contains("\"age\""));
    assert_eq!(params.len(), 2);
    assert_eq!(params[0], Value::Text("Bob".into()));
    assert_eq!(params[1], Value::Integer(25));
}

#[test]
fn select_builder_star() {
    let db = MockDriver::new();
    db.select("items").execute().unwrap();
    let log = db.take_log();
    assert_eq!(log[0].0, "SELECT * FROM \"items\"");
}

#[test]
fn select_builder_with_columns_where_order_limit() {
    let db = MockDriver::new();
    db.select("items")
        .column("name")
        .column("price")
        .where_(Condition::Gt("price".into(), Value::Real(10.0)))
        .order_by(Order::Desc("price".into()))
        .limit(5)
        .offset(2)
        .execute()
        .unwrap();

    let log = db.take_log();
    let (sql, params) = &log[0];
    assert!(sql.contains("SELECT \"name\", \"price\" FROM \"items\""));
    assert!(sql.contains("WHERE price > $1"));
    assert!(sql.contains("ORDER BY \"price\" DESC"));
    assert!(sql.contains("LIMIT 5"));
    assert!(sql.contains("OFFSET 2"));
    assert_eq!(params, &[Value::Real(10.0)]);
}

#[test]
fn update_builder_generates_correct_sql() {
    let db = MockDriver::new();
    db.update("users")
        .set("name", Value::Text("Carol".into()))
        .where_(Condition::Eq("id".into(), Value::Integer(3)))
        .execute()
        .unwrap();

    let log = db.take_log();
    let (sql, params) = &log[0];
    assert!(sql.contains("UPDATE \"users\" SET"));
    assert!(sql.contains("\"name\" = $1"));
    assert!(sql.contains("WHERE id = $2"));
    assert_eq!(params, &[Value::Text("Carol".into()), Value::Integer(3)]);
}

#[test]
fn delete_builder_generates_correct_sql() {
    let db = MockDriver::new();
    db.delete("users")
        .where_(Condition::Eq("id".into(), Value::Integer(7)))
        .execute()
        .unwrap();

    let log = db.take_log();
    let (sql, params) = &log[0];
    assert!(sql.contains("DELETE FROM \"users\""));
    assert!(sql.contains("WHERE id = $1"));
    assert_eq!(params, &[Value::Integer(7)]);
}

#[test]
fn condition_and_or_nested() {
    let db = MockDriver::new();
    db.select("t")
        .where_(Condition::And(vec![
            Condition::Eq("a".into(), Value::Integer(1)),
            Condition::Or(vec![
                Condition::Gt("b".into(), Value::Integer(10)),
                Condition::IsNull("c".into()),
            ]),
        ]))
        .execute()
        .unwrap();

    let log = db.take_log();
    let (sql, params) = &log[0];
    assert!(sql.contains("WHERE (a = $1 AND (b > $2 OR c IS NULL))"));
    assert_eq!(params, &[Value::Integer(1), Value::Integer(10)]);
}

#[test]
fn condition_in() {
    let db = MockDriver::new();
    db.select("t")
        .where_(Condition::In(
            "x".into(),
            vec![Value::Integer(1), Value::Integer(2), Value::Integer(3)],
        ))
        .execute()
        .unwrap();

    let log = db.take_log();
    let (sql, params) = &log[0];
    assert!(sql.contains("x IN ($1, $2, $3)"));
    assert_eq!(
        params,
        &[Value::Integer(1), Value::Integer(2), Value::Integer(3)]
    );
}

#[test]
fn condition_like_not_eq() {
    let db = MockDriver::new();
    db.select("t")
        .where_(Condition::And(vec![
            Condition::Like("name".into(), Value::Text("%foo%".into())),
            Condition::NotEq("status".into(), Value::Text("deleted".into())),
        ]))
        .execute()
        .unwrap();

    let log = db.take_log();
    let (sql, params) = &log[0];
    assert!(sql.contains("name LIKE $1"));
    assert!(sql.contains("status != $2"));
    assert_eq!(
        params,
        &[Value::Text("%foo%".into()), Value::Text("deleted".into())]
    );
}
