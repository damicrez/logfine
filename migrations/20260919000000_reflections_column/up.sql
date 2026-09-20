PRAGMA foreign_keys=OFF;

CREATE TABLE logs_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    log_date TEXT NOT NULL UNIQUE,
    energy INTEGER NOT NULL,
    mvos TEXT NOT NULL,
    reflections TEXT NOT NULL DEFAULT '{}'
);

INSERT INTO logs_new (id, log_date, energy, mvos, reflections)
SELECT
    id,
    log_date,
    energy,
    mvos,
    json_object(
        'What worked', json(worked),
        'What failed', json(failed),
        'Output', json(output)
    )
FROM logs;

DROP TABLE logs;
ALTER TABLE logs_new RENAME TO logs;

CREATE TABLE tasks_new (
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

INSERT INTO tasks_new SELECT * FROM tasks;
DROP TABLE tasks;
ALTER TABLE tasks_new RENAME TO tasks;

PRAGMA foreign_key_check;
PRAGMA foreign_keys=ON;
