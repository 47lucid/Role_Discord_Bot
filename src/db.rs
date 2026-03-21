use rusqlite::{params, Connection, OptionalExtension, Result as SqliteResult};
use std::path::Path;
use std::sync::Mutex;

pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    /// Initialize the database, creating tables if they don't exist.
    pub fn init<P: AsRef<Path>>(db_path: P) -> SqliteResult<Self> {
        let conn = Connection::open(db_path)?;
        let db = Database { conn: Mutex::new(conn) };
        db.create_tables()?;
        Ok(db)
    }

    fn create_tables(&self) -> SqliteResult<()> {
        let conn = self.conn.lock().unwrap();
        // Guild configurations: which roles are safe/avoid for each server
        conn.execute(
            "CREATE TABLE IF NOT EXISTS guild_config (
                guild_id INTEGER PRIMARY KEY,
                safe_roles TEXT NOT NULL DEFAULT '[]',
                avoid_roles TEXT NOT NULL DEFAULT '[]',
                log_channel_id INTEGER,
                filter_admin_roles BOOLEAN DEFAULT 1,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        // Add filter_admin_roles column if it doesn't exist (for existing databases)
        let _ = conn.execute(
            "ALTER TABLE guild_config ADD COLUMN filter_admin_roles BOOLEAN DEFAULT 1",
            [],
        );

        // Add log_channel_id column if it doesn't exist (for existing databases)
        let _ = conn.execute(
            "ALTER TABLE guild_config ADD COLUMN log_channel_id INTEGER",
            [],
        );

        // User role states: roles users had when they left
        conn.execute(
            "CREATE TABLE IF NOT EXISTS user_roles (
                guild_id INTEGER NOT NULL,
                user_id INTEGER NOT NULL,
                role_ids TEXT NOT NULL,
                saved_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                PRIMARY KEY (guild_id, user_id)
            )",
            [],
        )?;

        Ok(())
    }

    /// Save user roles when they leave
    pub fn save_user_roles(
        &self,
        guild_id: u64,
        user_id: u64,
        role_ids: &[u64],
    ) -> SqliteResult<()> {
        let role_json = serde_json::to_string(role_ids)
            .unwrap_or_else(|_| "[]".to_string());

        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO user_roles (guild_id, user_id, role_ids, saved_at)
             VALUES (?, ?, ?, CURRENT_TIMESTAMP)",
            params![guild_id as i64, user_id as i64, &role_json],
        )?;

        Ok(())
    }

    /// Retrieve saved roles for a user
    pub fn get_user_roles(&self, guild_id: u64, user_id: u64) -> SqliteResult<Option<Vec<u64>>> {
        let conn = self.conn.lock().unwrap();
        let result = conn.query_row(
            "SELECT role_ids FROM user_roles WHERE guild_id = ? AND user_id = ?",
            params![guild_id as i64, user_id as i64],
            |row| {
                let role_json: String = row.get(0)?;
                let roles: Vec<u64> = serde_json::from_str(&role_json)
                    .unwrap_or_default();
                Ok(roles)
            },
        ).optional()?;

        Ok(result)
    }

    /// Delete user roles after restoration (cleanup)
    pub fn delete_user_roles(&self, guild_id: u64, user_id: u64) -> SqliteResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM user_roles WHERE guild_id = ? AND user_id = ?",
            params![guild_id as i64, user_id as i64],
        )?;
        Ok(())
    }

    /// Set safe roles for a guild
    pub fn set_safe_roles(&self, guild_id: u64, role_ids: &[u64]) -> SqliteResult<()> {
        let role_json = serde_json::to_string(role_ids)
            .unwrap_or_else(|_| "[]".to_string());

        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO guild_config (guild_id, safe_roles, avoid_roles, log_channel_id, filter_admin_roles, updated_at)
             VALUES (?, ?, COALESCE((SELECT avoid_roles FROM guild_config WHERE guild_id = ?), '[]'), COALESCE((SELECT log_channel_id FROM guild_config WHERE guild_id = ?), NULL), COALESCE((SELECT filter_admin_roles FROM guild_config WHERE guild_id = ?), 1), CURRENT_TIMESTAMP)",
            params![guild_id as i64, &role_json, guild_id as i64, guild_id as i64, guild_id as i64],
        )?;

        Ok(())
    }

    /// Set avoid roles for a guild
    pub fn set_avoid_roles(&self, guild_id: u64, role_ids: &[u64]) -> SqliteResult<()> {
        let role_json = serde_json::to_string(role_ids)
            .unwrap_or_else(|_| "[]".to_string());

        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO guild_config (guild_id, safe_roles, avoid_roles, log_channel_id, filter_admin_roles, updated_at)
             VALUES (?, COALESCE((SELECT safe_roles FROM guild_config WHERE guild_id = ?), '[]'), ?, COALESCE((SELECT log_channel_id FROM guild_config WHERE guild_id = ?), NULL), COALESCE((SELECT filter_admin_roles FROM guild_config WHERE guild_id = ?), 1), CURRENT_TIMESTAMP)",
            params![guild_id as i64, guild_id as i64, &role_json, guild_id as i64, guild_id as i64],
        )?;

        Ok(())
    }

    /// Get safe roles for a guild
    pub fn get_safe_roles(&self, guild_id: u64) -> SqliteResult<Vec<u64>> {
        let conn = self.conn.lock().unwrap();
        let result = conn.query_row(
            "SELECT safe_roles FROM guild_config WHERE guild_id = ?",
            params![guild_id as i64],
            |row| {
                let role_json: String = row.get(0)?;
                let roles: Vec<u64> = serde_json::from_str(&role_json)
                    .unwrap_or_default();
                Ok(roles)
            },
        ).optional()?;

        Ok(result.unwrap_or_default())
    }

    /// Get avoid roles for a guild
    pub fn get_avoid_roles(&self, guild_id: u64) -> SqliteResult<Vec<u64>> {
        let conn = self.conn.lock().unwrap();
        let result = conn.query_row(
            "SELECT avoid_roles FROM guild_config WHERE guild_id = ?",
            params![guild_id as i64],
            |row| {
                let role_json: String = row.get(0)?;
                let roles: Vec<u64> = serde_json::from_str(&role_json)
                    .unwrap_or_default();
                Ok(roles)
            },
        ).optional()?;

        Ok(result.unwrap_or_default())
    }

    /// Filter roles based on safe/avoid configuration
    pub fn filter_roles_for_restoration(
        &self,
        guild_id: u64,
        saved_roles: &[u64],
    ) -> SqliteResult<Vec<u64>> {
        let safe_roles = self.get_safe_roles(guild_id)?;
        let avoid_roles = self.get_avoid_roles(guild_id)?;

        let filtered: Vec<u64> = saved_roles
            .iter()
            .filter(|role_id| {
                let is_safe = safe_roles.is_empty() || safe_roles.contains(role_id);
                let is_not_avoided = !avoid_roles.contains(role_id);
                is_safe && is_not_avoided
            })
            .copied()
            .collect();

        Ok(filtered)
    }

    /// Filter roles based on safe/avoid config, role hierarchy, and optionally admin permissions
    /// This is used during member restoration to ensure we don't assign forbidden roles
    pub fn filter_roles_for_restoration_with_permissions(
        &self,
        guild_id: u64,
        saved_roles: &[u64],
        bot_highest_role_position: i64,
        roles_map: &std::collections::HashMap<u64, (String, i64, bool)>, // (role_id -> (name, position, has_admin))
        filter_admin: bool,  // Whether to filter out admin role
    ) -> SqliteResult<Vec<u64>> {
        let safe_roles = self.get_safe_roles(guild_id)?;
        let avoid_roles = self.get_avoid_roles(guild_id)?;

        let filtered: Vec<u64> = saved_roles
            .iter()
            .filter(|role_id| {
                // Check safe/avoid rules
                let is_safe = safe_roles.is_empty() || safe_roles.contains(role_id);
                let is_not_avoided = !avoid_roles.contains(role_id);
                
                if !(is_safe && is_not_avoided) {
                    return false;
                }

                // Check role hierarchy: only assign roles below bot's highest role
                if let Some((_name, position, _has_admin)) = roles_map.get(role_id) {
                    if *position >= bot_highest_role_position {
                        // Role is at or above bot's position, can't assign it
                        return false;
                    }
                }

                // Check admin permissions if filtering is disabled (filter_admin = false means don't assign admin roles)
                if !filter_admin {
                    if let Some((_name, _position, has_admin)) = roles_map.get(role_id) {
                        if *has_admin {
                            // Role has admin permission, skip it
                            return false;
                        }
                    }
                }

                true
            })
            .copied()
            .collect();

        Ok(filtered)
    }

    /// Set logging channel for a guild
    pub fn set_log_channel(&self, guild_id: u64, channel_id: Option<u64>) -> SqliteResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO guild_config (guild_id, log_channel_id, safe_roles, avoid_roles, filter_admin_roles, updated_at)
             VALUES (?, ?, 
                COALESCE((SELECT safe_roles FROM guild_config WHERE guild_id = ?), '[]'),
                COALESCE((SELECT avoid_roles FROM guild_config WHERE guild_id = ?), '[]'),
                COALESCE((SELECT filter_admin_roles FROM guild_config WHERE guild_id = ?), 1),
             CURRENT_TIMESTAMP)",
            params![guild_id as i64, channel_id.map(|c| c as i64), guild_id as i64, guild_id as i64, guild_id as i64],
        )?;
        Ok(())
    }

    /// Get logging channel for a guild
    pub fn get_log_channel(&self, guild_id: u64) -> SqliteResult<Option<u64>> {
        let conn = self.conn.lock().unwrap();
        let result = conn.query_row(
            "SELECT log_channel_id FROM guild_config WHERE guild_id = ?",
            params![guild_id as i64],
            |row| {
                let channel_id: Option<i64> = row.get(0)?;
                Ok(channel_id.map(|c| c as u64))
            },
        ).optional()?;

        Ok(result.flatten())
    }

    /// Enable/disable admin role filtering for a guild
    pub fn set_filter_admin_roles(&self, guild_id: u64, enabled: bool) -> SqliteResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO guild_config (guild_id, filter_admin_roles, safe_roles, avoid_roles, log_channel_id, updated_at)
             VALUES (?, ?, 
                COALESCE((SELECT safe_roles FROM guild_config WHERE guild_id = ?), '[]'),
                COALESCE((SELECT avoid_roles FROM guild_config WHERE guild_id = ?), '[]'),
                COALESCE((SELECT log_channel_id FROM guild_config WHERE guild_id = ?), NULL),
             CURRENT_TIMESTAMP)",
            params![guild_id as i64, enabled, guild_id as i64, guild_id as i64, guild_id as i64],
        )?;
        Ok(())
    }

    /// Check if admin role filtering is enabled for a guild (defaults to true)
    pub fn get_filter_admin_roles(&self, guild_id: u64) -> SqliteResult<bool> {
        let conn = self.conn.lock().unwrap();
        let result = conn.query_row(
            "SELECT COALESCE(filter_admin_roles, 1) FROM guild_config WHERE guild_id = ?",
            params![guild_id as i64],
            |row| {
                let enabled: i64 = row.get(0)?;
                Ok(enabled != 0)
            },
        ).optional()?;

        Ok(result.unwrap_or(true))
    }
}
