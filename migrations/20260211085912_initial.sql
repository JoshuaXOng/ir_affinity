CREATE TABLE IF NOT EXISTS processes (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS cpus (
    id INTEGER PRIMARY KEY CHECK(id >= 0)
);

CREATE TABLE IF NOT EXISTS processes_selected_cpus (
    process_id TEXT NOT NULL,
    cpu_id INTEGER NOT NULL,
    PRIMARY KEY (process_id, cpu_id)
);
