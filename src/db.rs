use anyhow::Result;
use sqlx::{sqlite::SqlitePoolOptions, Row, SqlitePool};

#[derive(Clone)]
pub struct Db {
    pool: SqlitePool,
}

#[derive(Debug, Clone)]
pub struct SeenItem {
    pub external_id: String,
    pub title: String,
    pub os_family: String,
    pub url: String,
    pub published_at: String,
    pub first_seen_at: String,
}

#[derive(Debug, Clone)]
pub struct Snippet {
    pub name: String,
    pub content: String,
    pub owner_id: String,
    pub locked: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl Db {
    pub async fn connect(url: &str) -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(8)
            .connect(url)
            .await?;
        Ok(Self { pool })
    }

    pub async fn migrate(&self) -> Result<()> {
        for statement in [
            r#"
            CREATE TABLE IF NOT EXISTS settings (
                guild_id TEXT NOT NULL,
                key TEXT NOT NULL,
                value TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                PRIMARY KEY (guild_id, key)
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS seen_items (
                kind TEXT NOT NULL,
                external_id TEXT NOT NULL,
                title TEXT NOT NULL,
                os_family TEXT NOT NULL,
                url TEXT NOT NULL,
                published_at TEXT NOT NULL,
                first_seen_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                PRIMARY KEY (kind, external_id)
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS snippets (
                guild_id TEXT NOT NULL,
                name TEXT NOT NULL,
                content TEXT NOT NULL,
                owner_id TEXT NOT NULL,
                locked INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                PRIMARY KEY (guild_id, name)
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS aliases (
                guild_id TEXT NOT NULL,
                alias TEXT NOT NULL,
                target TEXT NOT NULL,
                owner_id TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                PRIMARY KEY (guild_id, alias)
            )
            "#,
        ] {
            sqlx::query(statement).execute(&self.pool).await?;
        }

        Ok(())
    }

    pub async fn set_setting(&self, guild_id: u64, key: &str, value: &str) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO settings (guild_id, key, value, updated_at)
            VALUES (?1, ?2, ?3, CURRENT_TIMESTAMP)
            ON CONFLICT(guild_id, key)
            DO UPDATE SET value = excluded.value, updated_at = CURRENT_TIMESTAMP
            "#,
        )
        .bind(guild_id.to_string())
        .bind(key)
        .bind(value)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn setting(&self, guild_id: u64, key: &str) -> Result<Option<String>> {
        let row = sqlx::query("SELECT value FROM settings WHERE guild_id = ?1 AND key = ?2")
            .bind(guild_id.to_string())
            .bind(key)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.map(|row| row.get::<String, _>("value")))
    }

    pub async fn settings_by_key(&self, key: &str) -> Result<Vec<(u64, String)>> {
        let rows = sqlx::query("SELECT guild_id, value FROM settings WHERE key = ?1")
            .bind(key)
            .fetch_all(&self.pool)
            .await?;

        let mut settings = Vec::new();
        for row in rows {
            let guild_id = row.get::<String, _>("guild_id").parse::<u64>()?;
            let value = row.get::<String, _>("value");
            settings.push((guild_id, value));
        }

        Ok(settings)
    }

    pub async fn insert_seen(
        &self,
        kind: &str,
        external_id: &str,
        title: &str,
        os_family: &str,
        url: &str,
        published_at: &str,
    ) -> Result<bool> {
        let result = sqlx::query(
            r#"
            INSERT OR IGNORE INTO seen_items
                (kind, external_id, title, os_family, url, published_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
        )
        .bind(kind)
        .bind(external_id)
        .bind(title)
        .bind(os_family)
        .bind(url)
        .bind(published_at)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() == 1)
    }

    pub async fn has_seen_kind(&self, kind: &str) -> Result<bool> {
        let row = sqlx::query("SELECT 1 FROM seen_items WHERE kind = ?1 LIMIT 1")
            .bind(kind)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.is_some())
    }

    pub async fn recent_seen(&self, kind: &str, limit: i64) -> Result<Vec<SeenItem>> {
        let rows = sqlx::query(
            r#"
            SELECT external_id, title, os_family, url, published_at, first_seen_at
            FROM seen_items
            WHERE kind = ?1
            ORDER BY first_seen_at DESC
            LIMIT ?2
            "#,
        )
        .bind(kind)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| SeenItem {
                external_id: row.get("external_id"),
                title: row.get("title"),
                os_family: row.get("os_family"),
                url: row.get("url"),
                published_at: row.get("published_at"),
                first_seen_at: row.get("first_seen_at"),
            })
            .collect())
    }

    pub async fn seen_for_utc_date(&self, kind: &str, date: &str) -> Result<Vec<SeenItem>> {
        let rows = sqlx::query(
            r#"
            SELECT external_id, title, os_family, url, published_at, first_seen_at
            FROM seen_items
            WHERE kind = ?1 AND substr(first_seen_at, 1, 10) = ?2
            ORDER BY os_family, published_at DESC
            "#,
        )
        .bind(kind)
        .bind(date)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| SeenItem {
                external_id: row.get("external_id"),
                title: row.get("title"),
                os_family: row.get("os_family"),
                url: row.get("url"),
                published_at: row.get("published_at"),
                first_seen_at: row.get("first_seen_at"),
            })
            .collect())
    }

    pub async fn create_snippet(
        &self,
        guild_id: u64,
        name: &str,
        content: &str,
        owner_id: u64,
    ) -> Result<bool> {
        let result = sqlx::query(
            r#"
            INSERT OR IGNORE INTO snippets (guild_id, name, content, owner_id)
            VALUES (?1, ?2, ?3, ?4)
            "#,
        )
        .bind(guild_id.to_string())
        .bind(name)
        .bind(content)
        .bind(owner_id.to_string())
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() == 1)
    }

    pub async fn update_snippet(
        &self,
        guild_id: u64,
        name: &str,
        content: &str,
        user_id: u64,
        is_admin: bool,
    ) -> Result<bool> {
        let current = self.snippet(guild_id, name).await?;
        let Some(current) = current else {
            return Ok(false);
        };

        if current.locked && current.owner_id != user_id.to_string() && !is_admin {
            return Ok(false);
        }

        let result = sqlx::query(
            r#"
            UPDATE snippets
            SET content = ?1, updated_at = CURRENT_TIMESTAMP
            WHERE guild_id = ?2 AND name = ?3
            "#,
        )
        .bind(content)
        .bind(guild_id.to_string())
        .bind(name)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() == 1)
    }

    pub async fn snippet(&self, guild_id: u64, name: &str) -> Result<Option<Snippet>> {
        let row = sqlx::query(
            r#"
            SELECT name, content, owner_id, locked, created_at, updated_at
            FROM snippets
            WHERE guild_id = ?1 AND name = ?2
            "#,
        )
        .bind(guild_id.to_string())
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|row| Snippet {
            name: row.get("name"),
            content: row.get("content"),
            owner_id: row.get("owner_id"),
            locked: row.get::<i64, _>("locked") == 1,
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        }))
    }

    pub async fn resolve_snippet(&self, guild_id: u64, name: &str) -> Result<Option<Snippet>> {
        if let Some(snippet) = self.snippet(guild_id, name).await? {
            return Ok(Some(snippet));
        }

        let alias = sqlx::query("SELECT target FROM aliases WHERE guild_id = ?1 AND alias = ?2")
            .bind(guild_id.to_string())
            .bind(name)
            .fetch_optional(&self.pool)
            .await?;

        match alias {
            Some(row) => {
                self.snippet(guild_id, &row.get::<String, _>("target"))
                    .await
            }
            None => Ok(None),
        }
    }

    pub async fn list_snippets(&self, guild_id: u64) -> Result<Vec<Snippet>> {
        let rows = sqlx::query(
            r#"
            SELECT name, content, owner_id, locked, created_at, updated_at
            FROM snippets
            WHERE guild_id = ?1
            ORDER BY name
            LIMIT 100
            "#,
        )
        .bind(guild_id.to_string())
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| Snippet {
                name: row.get("name"),
                content: row.get("content"),
                owner_id: row.get("owner_id"),
                locked: row.get::<i64, _>("locked") == 1,
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
            })
            .collect())
    }

    pub async fn toggle_lock(
        &self,
        guild_id: u64,
        name: &str,
        user_id: u64,
    ) -> Result<Option<bool>> {
        let Some(snippet) = self.snippet(guild_id, name).await? else {
            return Ok(None);
        };

        if snippet.owner_id != user_id.to_string() {
            return Ok(None);
        }

        let next = !snippet.locked;
        sqlx::query("UPDATE snippets SET locked = ?1, updated_at = CURRENT_TIMESTAMP WHERE guild_id = ?2 AND name = ?3")
            .bind(if next { 1 } else { 0 })
            .bind(guild_id.to_string())
            .bind(name)
            .execute(&self.pool)
            .await?;

        Ok(Some(next))
    }

    pub async fn delete_snippet(
        &self,
        guild_id: u64,
        name: &str,
        user_id: u64,
        is_admin: bool,
    ) -> Result<bool> {
        let Some(snippet) = self.snippet(guild_id, name).await? else {
            return Ok(false);
        };

        if snippet.owner_id != user_id.to_string() && !is_admin {
            return Ok(false);
        }

        sqlx::query("DELETE FROM aliases WHERE guild_id = ?1 AND target = ?2")
            .bind(guild_id.to_string())
            .bind(name)
            .execute(&self.pool)
            .await?;

        let result = sqlx::query("DELETE FROM snippets WHERE guild_id = ?1 AND name = ?2")
            .bind(guild_id.to_string())
            .bind(name)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() == 1)
    }

    pub async fn create_alias(
        &self,
        guild_id: u64,
        target: &str,
        alias: &str,
        owner_id: u64,
    ) -> Result<bool> {
        if self.snippet(guild_id, target).await?.is_none() {
            return Ok(false);
        }
        if self.snippet(guild_id, alias).await?.is_some() {
            return Ok(false);
        }

        let result = sqlx::query(
            r#"
            INSERT OR IGNORE INTO aliases (guild_id, alias, target, owner_id)
            VALUES (?1, ?2, ?3, ?4)
            "#,
        )
        .bind(guild_id.to_string())
        .bind(alias)
        .bind(target)
        .bind(owner_id.to_string())
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() == 1)
    }
}
