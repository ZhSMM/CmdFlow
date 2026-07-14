-- 0002: 放宽 executions.workflow_id 的外键约束
--
-- 背景：0001_init 里 executions.workflow_id 定义了 REFERENCES workflows(id)，
-- 但 Phase 1 的命令直跑 (`executor::execute`) 把 spec.command_id 直接塞到
-- workflow_id 字段（注释里也写了「Phase 1 复用 workflow_id 字段存 command_id」）。
-- 直跑命令时 command_id 不在 workflows 表里，于是 INSERT 触发
-- "FOREIGN KEY constraint failed"。
--
-- 修复：去掉 executions.workflow_id 的外键约束，让它既能存 workflow 的 id
-- 也能存 command 的 id（trigger 字段标识是哪种执行：manual/schedule/...）。
--
-- SQLite 不能 ALTER TABLE 改外键，只能 "新建-拷贝-删除-重命名"。
-- 关键坑：executions 被 node_runs / run_logs 引用 ON DELETE CASCADE，
-- 直接 DROP TABLE 会带没这两张表的数据。所以先把它们备份到 TEMP TABLE，
-- 删完再插回去。TEMP TABLE 不受事务回滚影响，所以开头加 IF EXISTS 防御。

-- 防御性清理（上回跑失败留下的临时表）
DROP TABLE IF EXISTS _tmp_node_runs;
DROP TABLE IF EXISTS _tmp_run_logs;

-- 1. 备份被级联删除的子表
CREATE TEMP TABLE _tmp_node_runs AS SELECT * FROM node_runs;
CREATE TEMP TABLE _tmp_run_logs  AS SELECT * FROM run_logs;

-- 2. 重建 executions（去掉 workflow_id 的外键）
CREATE TABLE executions_new (
    id            TEXT PRIMARY KEY,
    workflow_id   TEXT NOT NULL,  -- 既能存 workflow id 也能存 command id，无外键
    trigger       TEXT NOT NULL,
    status        TEXT NOT NULL,
    started_at    INTEGER,
    finished_at   INTEGER,
    duration_ms   INTEGER,
    input_params  TEXT,
    error         TEXT,
    env_snapshot  TEXT
);

INSERT INTO executions_new
    (id, workflow_id, trigger, status, started_at, finished_at, duration_ms, input_params, error, env_snapshot)
SELECT id, workflow_id, trigger, status, started_at, finished_at, duration_ms, input_params, error, env_snapshot
FROM executions;

DROP TABLE executions;
ALTER TABLE executions_new RENAME TO executions;
CREATE INDEX idx_executions_workflow ON executions(workflow_id);

-- 3. 恢复被级联删除的子表
INSERT INTO node_runs SELECT * FROM _tmp_node_runs;
INSERT INTO run_logs  SELECT * FROM _tmp_run_logs;

-- 4. 清理临时表
DROP TABLE _tmp_node_runs;
DROP TABLE _tmp_run_logs;
