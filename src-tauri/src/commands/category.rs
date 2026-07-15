//! 分类树 IPC (Phase 7)

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::storage::models::{Category, CategoryNode, Command, CommandType};

// `Option<Option<T>>::flatten` 不可用，写个 helper
fn flatten_opt<T>(o: Option<Option<T>>) -> Option<T> {
    match o {
        Some(inner) => inner,
        None => None,
    }
}

const MAX_DEPTH: i32 = 3; // 根 + 2 级子分类

#[tauri::command]
pub async fn list_category_tree(state: State<'_, AppState>) -> AppResult<Vec<CategoryNode>> {
    let conn = state.db.get()?;

    // 一次拿所有分类
    let mut cat_stmt = conn.prepare(
        "SELECT id, parent_id, name, icon, sort_order, created_at
         FROM categories ORDER BY sort_order, name",
    )?;
    let categories: Vec<Category> = cat_stmt
        .query_map([], |r| {
            Ok(Category {
                id: r.get(0)?,
                parent_id: r.get(1)?,
                name: r.get(2)?,
                icon: r.get(3)?,
                sort_order: r.get(4)?,
                created_at: r.get(5)?,
            })
        })?
        .filter_map(Result::ok)
        .collect();

    // 拿所有命令（按 category_id 分组）
    let mut cmd_stmt = conn.prepare(
        "SELECT id, name, description, category_id, type, current_ver, tags, created_at, updated_at
         FROM commands ORDER BY name",
    )?;
    let mut commands_by_cat: std::collections::HashMap<String, Vec<Command>> =
        std::collections::HashMap::new();
    let mut root_commands: Vec<Command> = Vec::new();
    let rows = cmd_stmt.query_map([], |r| {
        let tags_json: String = r.get(6)?;
        let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
        Ok(Command {
            id: r.get(0)?,
            name: r.get(1)?,
            description: r.get(2)?,
            category: r.get(3)?,
            command_type: CommandType::parse(&r.get::<_, String>(4)?)
                .ok_or_else(|| rusqlite::Error::InvalidQuery)?,
            current_ver: r.get(5)?,
            tags,
            created_at: r.get(7)?,
            updated_at: r.get(8)?,
        })
    })?;
    for row in rows.flatten() {
        if let Some(cat_id) = &row.category {
            commands_by_cat.entry(cat_id.clone()).or_default().push(row);
        } else {
            root_commands.push(row);
        }
    }

    // 递归构造树
    fn build(
        parent: Option<&str>,
        cats: &[Category],
        cmds: &mut std::collections::HashMap<String, Vec<Command>>,
        root_cmds: &mut Vec<Command>,
    ) -> Vec<CategoryNode> {
        let mut nodes: Vec<CategoryNode> = Vec::new();
        let children: Vec<&Category> = cats
            .iter()
            .filter(|c| c.parent_id.as_deref() == parent)
            .collect();
        for c in children {
            let sub = build(Some(&c.id), cats, cmds, root_cmds);
            let my_cmds = cmds.remove(&c.id).unwrap_or_default();
            nodes.push(CategoryNode {
                id: c.id.clone(),
                parent_id: c.parent_id.clone(),
                name: c.name.clone(),
                icon: c.icon.clone(),
                sort_order: c.sort_order,
                subcategories: sub,
                commands: my_cmds,
            });
        }
        nodes
    }

    let mut tree = build(None, &categories, &mut commands_by_cat, &mut root_commands);

    // 根级命令 (category_id = NULL) 包装成虚拟根
    if !root_commands.is_empty() {
        tree.insert(
            0,
            CategoryNode {
                id: "__root__".to_string(),
                parent_id: None,
                name: "未分类".to_string(),
                icon: Some("📦".to_string()),
                sort_order: -1,
                subcategories: Vec::new(),
                commands: std::mem::take(&mut root_commands),
            },
        );
    }

    Ok(tree)
}

#[derive(Deserialize)]
pub struct CreateCategoryInput {
    pub name: String,
    pub parent_id: Option<String>,
    pub icon: Option<String>,
}

#[tauri::command]
pub async fn create_category(
    state: State<'_, AppState>,
    input: CreateCategoryInput,
) -> AppResult<String> {
    if input.name.trim().is_empty() {
        return Err(AppError::invalid("分类名不能为空"));
    }

    // 检查深度
    if let Some(pid) = &input.parent_id {
        let conn = state.db.get()?;
        let depth = compute_depth(&conn, pid)? + 1;
        if depth >= MAX_DEPTH {
            return Err(AppError::invalid(format!(
                "分类最多 {MAX_DEPTH} 级,无法在当前层级下新建子分类"
            )));
        }
    }

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();
    let conn = state.db.get()?;
    conn.execute(
        "INSERT INTO categories (id, parent_id, name, icon, sort_order, created_at) VALUES (?1, ?2, ?3, ?4, 0, ?5)",
        rusqlite::params![id, input.parent_id, input.name.trim(), input.icon, now],
    )?;
    Ok(id)
}

#[derive(Deserialize)]
pub struct RenameCategoryInput {
    pub id: String,
    pub name: String,
}

#[tauri::command]
pub async fn rename_category(
    state: State<'_, AppState>,
    input: RenameCategoryInput,
) -> AppResult<()> {
    if input.name.trim().is_empty() {
        return Err(AppError::invalid("分类名不能为空"));
    }
    let conn = state.db.get()?;
    conn.execute(
        "UPDATE categories SET name = ?1 WHERE id = ?2",
        rusqlite::params![input.name.trim(), input.id],
    )?;
    Ok(())
}

#[derive(Deserialize)]
pub struct MoveCategoryInput {
    pub id: String,
    pub new_parent_id: Option<String>,
}

#[tauri::command]
pub async fn move_category(state: State<'_, AppState>, input: MoveCategoryInput) -> AppResult<()> {
    // 不能把分类移到自己的后代
    if let Some(pid) = &input.new_parent_id {
        let conn = state.db.get()?;
        if is_descendant_of(&conn, pid, &input.id)? {
            return Err(AppError::invalid("不能把分类移到自己的子分类下"));
        }
        let depth = compute_depth(&conn, pid)? + 1;
        let sub_depth = subtree_max_depth(&conn, &input.id)?;
        if depth + sub_depth >= MAX_DEPTH {
            return Err(AppError::invalid(format!(
                "移动后会超过最大深度 {MAX_DEPTH}"
            )));
        }
    }

    let conn = state.db.get()?;
    conn.execute(
        "UPDATE categories SET parent_id = ?1 WHERE id = ?2",
        rusqlite::params![input.new_parent_id, input.id],
    )?;
    Ok(())
}

#[tauri::command]
pub async fn delete_category(state: State<'_, AppState>, id: String) -> AppResult<()> {
    if id == "__root__" {
        return Err(AppError::invalid("根目录不可删除"));
    }
    // FK ON DELETE CASCADE 会删子分类
    // commands.category_id 是 ON DELETE SET NULL,命令自动归入根
    let conn = state.db.get()?;
    conn.execute("DELETE FROM categories WHERE id = ?1", [&id])?;
    Ok(())
}

#[derive(Deserialize)]
pub struct MoveCommandInput {
    pub command_id: String,
    pub category_id: Option<String>,
}

#[tauri::command]
pub async fn move_command(state: State<'_, AppState>, input: MoveCommandInput) -> AppResult<()> {
    let conn = state.db.get()?;
    conn.execute(
        "UPDATE commands SET category_id = ?1 WHERE id = ?2",
        rusqlite::params![input.category_id, input.command_id],
    )?;
    Ok(())
}

#[derive(Deserialize)]
pub struct ReorderCategoriesInput {
    /// 完整的有序 ID 列表 (从最顶层开始, 按 depth-first 顺序)
    pub ordered_ids: Vec<String>,
}

#[tauri::command]
pub async fn reorder_categories(
    state: State<'_, AppState>,
    input: ReorderCategoriesInput,
) -> AppResult<()> {
    let conn = state.db.get()?;
    let tx = conn.unchecked_transaction()?;
    for (i, id) in input.ordered_ids.iter().enumerate() {
        if id == "__root__" {
            continue;
        }
        tx.execute(
            "UPDATE categories SET sort_order = ?1 WHERE id = ?2",
            rusqlite::params![i as i32, id],
        )?;
    }
    tx.commit()?;
    Ok(())
}

// ==================== 内部辅助 ====================

fn compute_depth(conn: &rusqlite::Connection, id: &str) -> AppResult<i32> {
    let mut cur = id.to_string();
    let mut depth = 0i32;
    loop {
        let parent: Option<String> = match conn.query_row(
            "SELECT parent_id FROM categories WHERE id = ?1",
            [&cur],
            |r| r.get(0),
        ) {
            Ok(p) => p,
            Err(rusqlite::Error::QueryReturnedNoRows) => break,
            Err(e) => return Err(AppError::Database(e)),
        };
        match parent {
            Some(p) => {
                cur = p;
                depth += 1;
                if depth > 10 {
                    return Ok(depth);
                }
            }
            None => break,
        }
    }
    Ok(depth)
}

fn is_descendant_of(
    conn: &rusqlite::Connection,
    candidate: &str,
    ancestor: &str,
) -> AppResult<bool> {
    let mut cur = candidate.to_string();
    let mut depth = 0i32;
    loop {
        let parent: Option<String> = match conn.query_row(
            "SELECT parent_id FROM categories WHERE id = ?1",
            [&cur],
            |r| r.get(0),
        ) {
            Ok(p) => p,
            Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(false),
            Err(e) => return Err(AppError::Database(e)),
        };
        match parent {
            Some(p) => {
                if p == ancestor {
                    return Ok(true);
                }
                cur = p;
                depth += 1;
                if depth > 10 {
                    return Ok(false);
                }
            }
            None => return Ok(false),
        }
    }
}

fn subtree_max_depth(conn: &rusqlite::Connection, id: &str) -> AppResult<i32> {
    let mut stmt = conn.prepare("SELECT id FROM categories WHERE parent_id = ?1")?;
    let children: Vec<String> = stmt
        .query_map([id], |r| r.get::<_, String>(0))?
        .filter_map(Result::ok)
        .collect();
    drop(stmt);

    let mut max = 0i32;
    for child in children {
        let sub = subtree_max_depth(conn, &child)?;
        max = max.max(sub + 1);
    }
    Ok(max)
}
