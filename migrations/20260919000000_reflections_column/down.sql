PRAGMA foreign_keys=OFF;

CREATE TABLE logs_old (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    log_date TEXT NOT NULL UNIQUE,
    energy INTEGER NOT NULL,
    mvos TEXT NOT NULL,
    worked TEXT NOT NULL DEFAULT '[]',
    failed TEXT NOT NULL DEFAULT '[]',
    output TEXT NOT NULL DEFAULT '[]'
);

INSERT INTO logs_old (id, log_date, energy, mvos, worked, failed, output)
SELECT
    id,
    log_date,
    energy,
    mvos,
    COALESCE(json_extract(reflections, '$."What worked"'), '[]'),
    COALESCE(json_extract(reflections, '$."What failed"'), '[]'),
    COALESCE(json_extract(reflections, '$."Output"'), '[]')
FROM logs;

DROP TABLE logs;
ALTER TABLE logs_old RENAME TO logs;

PRAGMA foreign_keys=ON;
