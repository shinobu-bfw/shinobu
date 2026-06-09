use std::fmt;

// ============================================================================
// Value types
// ============================================================================

/// A parameter or result value for database operations.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Null => write!(f, "NULL"),
            Value::Integer(v) => write!(f, "{v}"),
            Value::Real(v) => write!(f, "{v}"),
            Value::Text(v) => write!(f, "{v}"),
            Value::Blob(v) => write!(f, "<blob {} bytes>", v.len()),
        }
    }
}

/// Column type for DDL (drivers translate to their native SQL types).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnType {
    Integer,
    Real,
    Text,
    Blob,
    Boolean,
}

/// A single row returned from a query.
#[derive(Debug, Clone)]
pub struct Row {
    pub columns: Vec<String>,
    pub values: Vec<Value>,
}

impl Row {
    #[must_use]
    pub fn get(&self, column: &str) -> Option<&Value> {
        self.columns
            .iter()
            .position(|c| c == column)
            .and_then(|i| self.values.get(i))
    }
}

/// Result of an execute call.
#[derive(Debug, Clone)]
pub struct QueryResult {
    pub rows_affected: u64,
    pub last_insert_id: Option<i64>,
}

// ============================================================================
// Condition / Order
// ============================================================================

/// WHERE clause condition for builders.
#[derive(Debug, Clone)]
pub enum Condition {
    Eq(String, Value),
    NotEq(String, Value),
    Gt(String, Value),
    Lt(String, Value),
    Gte(String, Value),
    Lte(String, Value),
    Like(String, Value),
    In(String, Vec<Value>),
    IsNull(String),
    IsNotNull(String),
    And(Vec<Condition>),
    Or(Vec<Condition>),
}

/// ORDER BY direction.
#[derive(Debug, Clone)]
pub enum Order {
    Asc(String),
    Desc(String),
}

// ============================================================================
// DatabaseDriver — object-safe trait that drivers implement
// ============================================================================

/// Low-level database driver interface.
///
/// Drivers implement this trait. Plugins should use [`DatabaseOps`] instead.
/// The trait is object-safe so it can be stored as `Arc<dyn DatabaseDriver>`.
pub trait DatabaseDriver: Send + Sync {
    fn name(&self) -> &str;

    /// Translate a [`ColumnType`] to the driver's native SQL type string.
    fn column_type_sql(&self, ct: ColumnType) -> &str;

    /// Return a placeholder for the given 1-based parameter index.
    fn placeholder(&self, index: usize) -> String;

    /// Execute a raw SQL statement.
    fn exec_raw(&self, sql: &str, params: &[Value]) -> anyhow::Result<QueryResult>;

    /// Execute a raw SQL query and return rows.
    fn query_raw(&self, sql: &str, params: &[Value]) -> anyhow::Result<Vec<Row>>;

    fn drop_table(&self, name: &str) -> anyhow::Result<()>;
    fn begin_transaction(&self) -> anyhow::Result<()>;
    fn commit(&self) -> anyhow::Result<()>;
    fn rollback(&self) -> anyhow::Result<()>;
}

// ============================================================================
// DatabaseOps — extension trait with builder API (what plugins use)
// ============================================================================

/// High-level database operations via builder pattern.
///
/// This is the primary interface for plugins. Import this trait and call
/// builder methods on any `&impl DatabaseDriver` (or `&dyn DatabaseDriver`).
///
/// ```ignore
/// use snb_core::database::{DatabaseOps, ColumnType, Condition, Value};
///
/// db.table("users")
///     .column("id", ColumnType::Integer, true, true, true, None)
///     .if_not_exists()
///     .execute()?;
/// ```
pub trait DatabaseOps: DatabaseDriver {
    fn table(&self, name: &str) -> TableBuilder<'_>;
    fn insert(&self, table: &str) -> InsertBuilder<'_>;
    fn select(&self, table: &str) -> SelectBuilder<'_>;
    fn update(&self, table: &str) -> UpdateBuilder<'_>;
    fn delete(&self, table: &str) -> DeleteBuilder<'_>;
}

impl<T: DatabaseDriver> DatabaseOps for T {
    fn table(&self, name: &str) -> TableBuilder<'_> {
        TableBuilder {
            driver: self as &dyn DatabaseDriver,
            name: name.to_string(),
            columns: Vec::new(),
            if_not_exists: false,
        }
    }

    fn insert(&self, table: &str) -> InsertBuilder<'_> {
        InsertBuilder {
            driver: self as &dyn DatabaseDriver,
            table: table.to_string(),
            pairs: Vec::new(),
        }
    }

    fn select(&self, table: &str) -> SelectBuilder<'_> {
        SelectBuilder {
            driver: self as &dyn DatabaseDriver,
            table: table.to_string(),
            columns: Vec::new(),
            conditions: None,
            orders: Vec::new(),
            limit_val: None,
            offset_val: None,
        }
    }

    fn update(&self, table: &str) -> UpdateBuilder<'_> {
        UpdateBuilder {
            driver: self as &dyn DatabaseDriver,
            table: table.to_string(),
            pairs: Vec::new(),
            conditions: None,
        }
    }

    fn delete(&self, table: &str) -> DeleteBuilder<'_> {
        DeleteBuilder {
            driver: self as &dyn DatabaseDriver,
            table: table.to_string(),
            conditions: None,
        }
    }
}

// ============================================================================
// SQL generation helpers
// ============================================================================

fn build_condition_sql(
    cond: &Condition,
    driver: &dyn DatabaseDriver,
    params: &mut Vec<Value>,
) -> String {
    match cond {
        Condition::Eq(col, val) => {
            params.push(val.clone());
            format!("{} = {}", col, driver.placeholder(params.len()))
        }
        Condition::NotEq(col, val) => {
            params.push(val.clone());
            format!("{} != {}", col, driver.placeholder(params.len()))
        }
        Condition::Gt(col, val) => {
            params.push(val.clone());
            format!("{} > {}", col, driver.placeholder(params.len()))
        }
        Condition::Lt(col, val) => {
            params.push(val.clone());
            format!("{} < {}", col, driver.placeholder(params.len()))
        }
        Condition::Gte(col, val) => {
            params.push(val.clone());
            format!("{} >= {}", col, driver.placeholder(params.len()))
        }
        Condition::Lte(col, val) => {
            params.push(val.clone());
            format!("{} <= {}", col, driver.placeholder(params.len()))
        }
        Condition::Like(col, val) => {
            params.push(val.clone());
            format!("{} LIKE {}", col, driver.placeholder(params.len()))
        }
        Condition::In(col, vals) => {
            let mut placeholders = Vec::new();
            for v in vals {
                params.push(v.clone());
                placeholders.push(driver.placeholder(params.len()));
            }
            format!("{} IN ({})", col, placeholders.join(", "))
        }
        Condition::IsNull(col) => format!("{col} IS NULL"),
        Condition::IsNotNull(col) => format!("{col} IS NOT NULL"),
        Condition::And(conds) => {
            let parts: Vec<String> = conds
                .iter()
                .map(|c| build_condition_sql(c, driver, params))
                .collect();
            format!("({})", parts.join(" AND "))
        }
        Condition::Or(conds) => {
            let parts: Vec<String> = conds
                .iter()
                .map(|c| build_condition_sql(c, driver, params))
                .collect();
            format!("({})", parts.join(" OR "))
        }
    }
}

// ============================================================================
// Builders
// ============================================================================

struct ColumnDef {
    name: String,
    col_type: ColumnType,
    primary_key: bool,
    not_null: bool,
    auto_increment: bool,
    default: Option<Value>,
}

/// Builder for CREATE TABLE.
pub struct TableBuilder<'a> {
    driver: &'a dyn DatabaseDriver,
    name: String,
    columns: Vec<ColumnDef>,
    if_not_exists: bool,
}

impl TableBuilder<'_> {
    #[must_use]
    pub fn column(
        mut self,
        name: &str,
        col_type: ColumnType,
        primary_key: bool,
        not_null: bool,
        auto_increment: bool,
        default: Option<Value>,
    ) -> Self {
        self.columns.push(ColumnDef {
            name: name.to_string(),
            col_type,
            primary_key,
            not_null,
            auto_increment,
            default,
        });
        self
    }

    #[must_use]
    pub fn if_not_exists(mut self) -> Self {
        self.if_not_exists = true;
        self
    }

    pub fn execute(self) -> anyhow::Result<()> {
        let mut col_defs = Vec::new();
        for c in &self.columns {
            let mut parts = vec![
                format!("\"{}\"", c.name),
                self.driver.column_type_sql(c.col_type).to_string(),
            ];
            if c.primary_key {
                parts.push("PRIMARY KEY".to_string());
            }
            if c.auto_increment {
                parts.push("AUTOINCREMENT".to_string());
            }
            if c.not_null && !c.primary_key {
                parts.push("NOT NULL".to_string());
            }
            if let Some(ref default) = c.default {
                parts.push(format!("DEFAULT {default}"));
            }
            col_defs.push(parts.join(" "));
        }
        let if_not = if self.if_not_exists {
            "IF NOT EXISTS "
        } else {
            ""
        };
        let sql = format!(
            "CREATE TABLE {}\"{}\" ({})",
            if_not,
            self.name,
            col_defs.join(", ")
        );
        self.driver.exec_raw(&sql, &[])?;
        Ok(())
    }
}

/// Builder for INSERT.
pub struct InsertBuilder<'a> {
    driver: &'a dyn DatabaseDriver,
    table: String,
    pairs: Vec<(String, Value)>,
}

impl InsertBuilder<'_> {
    #[must_use]
    pub fn set(mut self, column: &str, value: Value) -> Self {
        self.pairs.push((column.to_string(), value));
        self
    }

    pub fn execute(self) -> anyhow::Result<QueryResult> {
        let mut params = Vec::new();
        let mut col_names = Vec::new();
        let mut placeholders = Vec::new();
        for (col, val) in &self.pairs {
            params.push(val.clone());
            col_names.push(format!("\"{col}\""));
            placeholders.push(self.driver.placeholder(params.len()));
        }
        let sql = format!(
            "INSERT INTO \"{}\" ({}) VALUES ({})",
            self.table,
            col_names.join(", "),
            placeholders.join(", ")
        );
        self.driver.exec_raw(&sql, &params)
    }
}

/// Builder for SELECT.
pub struct SelectBuilder<'a> {
    driver: &'a dyn DatabaseDriver,
    table: String,
    columns: Vec<String>,
    conditions: Option<Condition>,
    orders: Vec<Order>,
    limit_val: Option<u64>,
    offset_val: Option<u64>,
}

impl SelectBuilder<'_> {
    #[must_use]
    pub fn column(mut self, name: &str) -> Self {
        self.columns.push(format!("\"{name}\""));
        self
    }

    #[must_use]
    pub fn where_(mut self, cond: Condition) -> Self {
        self.conditions = Some(cond);
        self
    }

    #[must_use]
    pub fn order_by(mut self, order: Order) -> Self {
        self.orders.push(order);
        self
    }

    #[must_use]
    pub fn limit(mut self, n: u64) -> Self {
        self.limit_val = Some(n);
        self
    }

    #[must_use]
    pub fn offset(mut self, n: u64) -> Self {
        self.offset_val = Some(n);
        self
    }

    pub fn execute(self) -> anyhow::Result<Vec<Row>> {
        let cols = if self.columns.is_empty() {
            "*".to_string()
        } else {
            self.columns.join(", ")
        };
        let mut sql = format!("SELECT {cols} FROM \"{}\"", self.table);
        let mut params = Vec::new();

        if let Some(ref cond) = self.conditions {
            sql.push_str(&format!(
                " WHERE {}",
                build_condition_sql(cond, self.driver, &mut params)
            ));
        }

        if !self.orders.is_empty() {
            let order_parts: Vec<String> = self
                .orders
                .iter()
                .map(|o| match o {
                    Order::Asc(col) => format!("\"{col}\" ASC"),
                    Order::Desc(col) => format!("\"{col}\" DESC"),
                })
                .collect();
            sql.push_str(&format!(" ORDER BY {}", order_parts.join(", ")));
        }

        if let Some(limit) = self.limit_val {
            sql.push_str(&format!(" LIMIT {limit}"));
        }
        if let Some(offset) = self.offset_val {
            sql.push_str(&format!(" OFFSET {offset}"));
        }

        self.driver.query_raw(&sql, &params)
    }
}

/// Builder for UPDATE.
pub struct UpdateBuilder<'a> {
    driver: &'a dyn DatabaseDriver,
    table: String,
    pairs: Vec<(String, Value)>,
    conditions: Option<Condition>,
}

impl UpdateBuilder<'_> {
    #[must_use]
    pub fn set(mut self, column: &str, value: Value) -> Self {
        self.pairs.push((column.to_string(), value));
        self
    }

    #[must_use]
    pub fn where_(mut self, cond: Condition) -> Self {
        self.conditions = Some(cond);
        self
    }

    pub fn execute(self) -> anyhow::Result<QueryResult> {
        let mut params = Vec::new();
        let mut set_parts = Vec::new();
        for (col, val) in &self.pairs {
            params.push(val.clone());
            set_parts.push(format!(
                "\"{}\" = {}",
                col,
                self.driver.placeholder(params.len())
            ));
        }
        let mut sql = format!("UPDATE \"{}\" SET {}", self.table, set_parts.join(", "));
        if let Some(ref cond) = self.conditions {
            sql.push_str(&format!(
                " WHERE {}",
                build_condition_sql(cond, self.driver, &mut params)
            ));
        }
        self.driver.exec_raw(&sql, &params)
    }
}

/// Builder for DELETE.
pub struct DeleteBuilder<'a> {
    driver: &'a dyn DatabaseDriver,
    table: String,
    conditions: Option<Condition>,
}

impl DeleteBuilder<'_> {
    #[must_use]
    pub fn where_(mut self, cond: Condition) -> Self {
        self.conditions = Some(cond);
        self
    }

    pub fn execute(self) -> anyhow::Result<QueryResult> {
        let mut params = Vec::new();
        let mut sql = format!("DELETE FROM \"{}\"", self.table);
        if let Some(ref cond) = self.conditions {
            sql.push_str(&format!(
                " WHERE {}",
                build_condition_sql(cond, self.driver, &mut params)
            ));
        }
        self.driver.exec_raw(&sql, &params)
    }
}

#[cfg(test)]
#[path = "../tests/unit/database_tests.rs"]
mod database_tests;
