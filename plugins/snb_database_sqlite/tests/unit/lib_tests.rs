use super::*;
use snb_core::database::{ColumnType, Condition, DatabaseOps, Order};

fn test_db() -> SqliteDatabase {
    let conn = Connection::open_in_memory().expect("failed to open in-memory SQLite database");
    conn.execute_batch("PRAGMA foreign_keys=ON;")
        .expect("failed to set SQLite pragmas");
    SqliteDatabase {
        name: "test".to_string(),
        conn: Mutex::new(conn),
    }
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

    let rows = db.select("items").execute().unwrap();
    assert_eq!(rows.len(), 3);

    let rows = db
        .select("items")
        .where_(Condition::Gt("price".into(), Value::Real(1.0)))
        .execute()
        .unwrap();
    assert_eq!(rows.len(), 2);

    let rows = db
        .select("items")
        .order_by(Order::Desc("price".into()))
        .limit(1)
        .execute()
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("name"), Some(&Value::Text("Cherry".into())));

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

    let rows = db
        .select("products")
        .where_(Condition::And(vec![
            Condition::Eq("category".into(), Value::Text("electronics".into())),
            Condition::Gt("price".into(), Value::Real(500.0)),
        ]))
        .execute()
        .unwrap();
    assert_eq!(rows.len(), 2);

    let rows = db
        .select("products")
        .where_(Condition::Or(vec![
            Condition::Eq("category".into(), Value::Text("clothing".into())),
            Condition::Lt("price".into(), Value::Real(700.0)),
        ]))
        .execute()
        .unwrap();
    assert_eq!(rows.len(), 4);

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

    db.begin_transaction().unwrap();
    db.update("accounts")
        .set("balance", Value::Real(200.0))
        .where_(Condition::Eq("id".into(), Value::Integer(1)))
        .execute()
        .unwrap();
    db.rollback().unwrap();

    let rows = db.select("accounts").execute().unwrap();
    assert_eq!(rows[0].get("balance"), Some(&Value::Real(100.0)));

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

    assert!(db.select("temp").execute().is_err());
}
