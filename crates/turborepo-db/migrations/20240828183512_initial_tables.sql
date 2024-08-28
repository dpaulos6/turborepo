CREATE TABLE IF NOT EXISTS runs (
    id TEXT PRIMARY KEY, -- primary key should be uuid
    start_time INTEGER NOT NULL,
    end_time INTEGER,
    exit_code INTEGER,
    status TEXT NOT NULL,
    command TEXT NOT NULL,
    package_inference_root TEXT,
    context TEXT NOT NULL,
    git_branch TEXT,
    git_sha TEXT,
    origination_user TEXT NOT NULL,
    client_id TEXT NOT NULL,
    client_name TEXT NOT NULL,
    client_version TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL,
    name TEXT NOT NULL,
    package TEXT NOT NULL,
    hash TEXT NOT NULL,
    start_time INTEGER NOT NULL,
    end_time INTEGER NOT NULL,
    cache_status TEXT NOT NULL,
    exit_code INTEGER,
    logs TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS task_dependencies (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id TEXT NOT NULL,
    dependency_id TEXT NOT NULL
);