-- 0004: 多级分类树 + 收藏

-- 多级分类树（最多 3 级，UI 控制）
CREATE TABLE categories (
    id          TEXT PRIMARY KEY,
    parent_id   TEXT REFERENCES categories(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    icon        TEXT,                          -- emoji 或 lucide 名
    sort_order  INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL,
    UNIQUE(parent_id, name)                     -- 同级不重名
);

CREATE INDEX idx_categories_parent ON categories(parent_id);

-- 命令可以归类（NULL = 根目录）
ALTER TABLE commands ADD COLUMN category_id TEXT REFERENCES categories(id) ON DELETE SET NULL;
CREATE INDEX idx_commands_category_id ON commands(category_id);

-- 收藏（用户置顶的常用命令）
CREATE TABLE favorites (
    command_id  TEXT PRIMARY KEY REFERENCES commands(id) ON DELETE CASCADE,
    sort_order  INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL
);

-- 插件
CREATE TABLE plugins (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    version     TEXT NOT NULL,
    author      TEXT,
    description TEXT,
    format      TEXT NOT NULL,                  -- 'js' | 'wasm'
    entry       TEXT NOT NULL,                  -- 相对 plugins_dir 的入口路径
    manifest    TEXT NOT NULL,                  -- manifest.json 全文
    enabled     BOOLEAN NOT NULL DEFAULT 1,
    installed_at INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);
