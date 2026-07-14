-- CmdFlow 初始 schema
-- 见 DESIGN.md §4.2 详细设计

-- ==================== 命令库 ====================

CREATE TABLE commands (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL UNIQUE,
    description   TEXT,
    category      TEXT,
    type          TEXT NOT NULL,
    current_ver   INTEGER NOT NULL DEFAULT 1,
    tags          TEXT NOT NULL DEFAULT '[]',
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL
);

CREATE TABLE command_versions (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    command_id    TEXT NOT NULL REFERENCES commands(id) ON DELETE CASCADE,
    version       INTEGER NOT NULL,
    template      TEXT NOT NULL,
    working_dir   TEXT,
    env           TEXT,
    timeout_ms    INTEGER,
    shell         TEXT,
    note          TEXT,
    created_at    INTEGER NOT NULL,
    UNIQUE(command_id, version)
);

CREATE TABLE params (
    id            TEXT PRIMARY KEY,
    command_ver_id INTEGER NOT NULL REFERENCES command_versions(id) ON DELETE CASCADE,
    name          TEXT NOT NULL,
    label         TEXT NOT NULL,
    type          TEXT NOT NULL,
    required      BOOLEAN NOT NULL DEFAULT 0,
    default_value TEXT,
    options       TEXT,
    validation    TEXT,
    sensitive     BOOLEAN NOT NULL DEFAULT 0,
    description   TEXT,
    sort_order    INTEGER NOT NULL DEFAULT 0,
    UNIQUE(command_ver_id, name)
);

CREATE INDEX idx_commands_category ON commands(category);
CREATE INDEX idx_command_versions_cmd ON command_versions(command_id);
CREATE INDEX idx_params_cmd_ver ON params(command_ver_id);

-- ==================== 工作流 ====================

CREATE TABLE workflows (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    description   TEXT,
    enabled       BOOLEAN NOT NULL DEFAULT 1,
    trigger_type  TEXT NOT NULL DEFAULT 'manual',
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL
);

CREATE TABLE nodes (
    id            TEXT PRIMARY KEY,
    workflow_id   TEXT NOT NULL REFERENCES workflows(id) ON DELETE CASCADE,
    type          TEXT NOT NULL,
    command_id    TEXT REFERENCES commands(id),
    config        TEXT NOT NULL,
    position_x    REAL NOT NULL DEFAULT 0,
    position_y    REAL NOT NULL DEFAULT 0,
    sort_order    INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE edges (
    id            TEXT PRIMARY KEY,
    workflow_id   TEXT NOT NULL REFERENCES workflows(id) ON DELETE CASCADE,
    source_node   TEXT NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    target_node   TEXT NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    source_port   TEXT DEFAULT 'out',
    target_port   TEXT DEFAULT 'in',
    condition     TEXT
);

CREATE INDEX idx_nodes_workflow ON nodes(workflow_id);
CREATE INDEX idx_edges_workflow ON edges(workflow_id);

-- ==================== 执行历史 ====================

CREATE TABLE executions (
    id            TEXT PRIMARY KEY,
    workflow_id   TEXT NOT NULL REFERENCES workflows(id),
    trigger       TEXT NOT NULL,
    status        TEXT NOT NULL,
    started_at    INTEGER,
    finished_at   INTEGER,
    duration_ms   INTEGER,
    input_params  TEXT,
    error         TEXT,
    env_snapshot  TEXT
);

CREATE TABLE node_runs (
    id            TEXT PRIMARY KEY,
    execution_id  TEXT NOT NULL REFERENCES executions(id) ON DELETE CASCADE,
    node_id       TEXT NOT NULL,
    status        TEXT NOT NULL,
    started_at    INTEGER,
    finished_at   INTEGER,
    duration_ms   INTEGER,
    exit_code     INTEGER,
    stdout        TEXT,
    stderr        TEXT,
    output_data   TEXT,
    error         TEXT
);

CREATE TABLE run_logs (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    node_run_id   TEXT NOT NULL REFERENCES node_runs(id) ON DELETE CASCADE,
    stream        TEXT NOT NULL,
    content       TEXT NOT NULL,
    ts            INTEGER NOT NULL
);

CREATE INDEX idx_executions_workflow ON executions(workflow_id);
CREATE INDEX idx_node_runs_exec ON node_runs(execution_id);
CREATE INDEX idx_run_logs_node_run ON run_logs(node_run_id);

-- ==================== 调度 ====================

CREATE TABLE schedules (
    id            TEXT PRIMARY KEY,
    workflow_id   TEXT NOT NULL REFERENCES workflows(id) ON DELETE CASCADE,
    cron_expr     TEXT NOT NULL,
    timezone      TEXT DEFAULT 'local',
    enabled       BOOLEAN NOT NULL DEFAULT 1,
    last_run_at   INTEGER,
    next_run_at   INTEGER,
    mode          TEXT NOT NULL DEFAULT 'in_app',
    os_task_id    TEXT,
    created_at    INTEGER NOT NULL
);

-- ==================== 黑名单 ====================

CREATE TABLE blacklist (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    pattern       TEXT NOT NULL,
    description   TEXT,
    enabled       BOOLEAN NOT NULL DEFAULT 1
);

-- ==================== 设置 ====================

CREATE TABLE settings (
    key           TEXT PRIMARY KEY,
    value         TEXT NOT NULL
);
