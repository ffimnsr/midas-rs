use std::collections::HashMap;

use postgres::Row as PostgresRow;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct State {
    name: String,
    schemas: HashMap<String, Schema>,
}

impl State {
    pub fn new(name: &str) -> Self {
        let mut schemas = HashMap::new();
        schemas.insert("public".into(), Schema::new("public"));

        Self {
            name: name.into(),
            schemas,
        }
    }

    pub fn add_schema(&mut self, name: &str) {
        self.schemas.insert(name.into(), Schema::new(name));
    }

    pub fn get_schema(&mut self, name: &str) -> Option<&mut Schema> {
        self.schemas.get_mut(name)
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Schema {
    name: String,
    tables: Vec<Table>,
    functions: Vec<Function>,
    triggers: Vec<Trigger>,
}

impl Schema {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.into(),
            tables: Vec::new(),
            functions: Vec::new(),
            triggers: Vec::new(),
        }
    }

    pub fn add_table(&mut self, table: Table) {
        self.tables.push(table);
    }

    pub fn add_function(&mut self, function: Function) {
        self.functions.push(function);
    }

    pub fn add_trigger(&mut self, trigger: Trigger) {
        self.triggers.push(trigger);
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Table {
    name: String,
    columns: Vec<Column>,
    rls: bool,
}

impl Table {
    pub fn new(name: String) -> Self {
        Self {
            name,
            columns: Vec::new(),
            rls: false,
        }
    }

    pub fn add_column(&mut self, column: Column) {
        self.columns.push(column);
    }

    pub fn enable_rls(&mut self) {
        self.rls = true;
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Column {
    name: String,
    data_type: String,
    default: Option<String>,
}

impl Column {
    pub fn new(name: String, data_type: String, default: Option<String>) -> Self {
        Self {
            name,
            data_type,
            default,
        }
    }
}

impl From<PostgresRow> for Column {
    fn from(it: PostgresRow) -> Self {
        Self {
            name: it.get("column_name"),
            data_type: it.get("udt_name"),
            default: it.get("column_default"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Function {
    name: String
}

#[derive(Debug, Serialize, Deserialize)]
struct Trigger {
    name: String
}
