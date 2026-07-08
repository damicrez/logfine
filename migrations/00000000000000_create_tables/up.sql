CREATE TABLE logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    log_date TEXT NOT NULL UNIQUE,
    energy INTEGER NOT NULL,
    mvos TEXT NOT NULL,
    worked TEXT NOT NULL,
    failed TEXT NOT NULL,
    output TEXT NOT NULL
);

CREATE TABLE tasks (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    log_id INTEGER NOT NULL REFERENCES logs(id) ON DELETE CASCADE,
    priority TEXT,
    completion_date TEXT,
    creation_date TEXT,
    project_tag TEXT,
    context_tag TEXT,
    key_value_tags TEXT NOT NULL,
    raw_line TEXT NOT NULL,
    is_completed BOOLEAN NOT NULL DEFAULT FALSE
);

CREATE TABLE todo_cache (
    raw_line TEXT PRIMARY KEY NOT NULL
);
