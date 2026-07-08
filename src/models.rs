use crate::schema::{logs, tasks, todo_cache};
use diesel::prelude::*;

#[derive(Queryable, Selectable, Identifiable, Debug, Clone)]
#[diesel(table_name = logs)]
pub struct LogDb {
    pub id: i32,
    pub log_date: String,
    pub energy: i32,
    pub mvos: String,
    pub worked: String,
    pub failed: String,
    pub output: String,
}

#[derive(Insertable, Debug)]
#[diesel(table_name = logs)]
pub struct NewLogDb<'a> {
    pub log_date: &'a str,
    pub energy: i32,
    pub mvos: &'a str,
    pub worked: &'a str,
    pub failed: &'a str,
    pub output: &'a str,
}

#[derive(Queryable, Selectable, Identifiable, Associations, Debug, Clone)]
#[diesel(belongs_to(LogDb, foreign_key = log_id))]
#[diesel(table_name = tasks)]
pub struct TaskDb {
    pub id: i32,
    pub log_id: i32,
    pub priority: Option<String>,
    pub completion_date: Option<String>,
    pub creation_date: Option<String>,
    pub project_tag: Option<String>,
    pub context_tag: Option<String>,
    pub key_value_tags: String,
    pub raw_line: String,
    pub is_completed: bool,
}

#[derive(Insertable, Debug)]
#[diesel(table_name = tasks)]
pub struct NewTaskDb<'a> {
    pub log_id: i32,
    pub priority: Option<String>,
    pub completion_date: Option<String>,
    pub creation_date: Option<String>,
    pub project_tag: Option<String>,
    pub context_tag: Option<String>,
    pub key_value_tags: String,
    pub raw_line: &'a str,
    pub is_completed: bool,
}

#[derive(Queryable, Selectable, Insertable, Debug, Clone)]
#[diesel(table_name = todo_cache)]
pub struct TodoCacheDb {
    pub raw_line: String,
}
