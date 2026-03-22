// src/main.rs
mod commands;
mod db;
mod http_server;

use async_trait::async_trait;
use db::Database;
use serenity::{
    all::{
        ChannelId, Context, CreateEmbed, CreateEmbedAuthor, CreateMessage, EventHandler,
        GatewayIntents, GuildId, Interaction, Member, Ready, RoleId, User,
    },
    Client,
};
use std::env;
use std::io::Write;
use std::sync::Arc;

struct Handler {
    db: Arc<Database>,
}

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, _ready: Ready) {
        println!("✅ Bot is ready! Registering commands...");
        match commands::register_commands(&ctx).await {
            Ok(_) => println!("✅ Commands registered successfully"),
            Err(e) => eprintln!("❌ Error registering commands: {}", e),
        }
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        match interaction {
            Interaction::Command(command) => {
                if command.data.name == "setup" {
                    if let Err(e) =
                        commands::handle_setup_command(&ctx, &command, self.db.clone()).await
                    {
                        eprintln!("Error handling setup command: {}", e);
                    }
                }
            }
            Interaction::Component(component) => {
                let custom_id = component.data.custom_id.as_str();
                let result = if custom_id.starts_with("safe_roles_select_") {
                    commands::handle_safe_roles_select(&ctx, &component, self.db.clone()).await
                } else if custom_id.starts_with("avoid_roles_select_") {
                    commands::handle_avoid_roles_select(&ctx, &component, self.db.clone()).await
                } else if custom_id.starts_with("log_channel_select_") {
                    commands::handle_log_channel_select(&ctx, &component, self.db.clone()).await
                } else if custom_id.starts_with("admin_filter_toggle_") {
                    commands::handle_admin_filter_toggle(&ctx, &component, self.db.clone()).await
                } else {
                    Ok(())
                };

                if let Err(e) = result {
                    eprintln!("Error handling component interaction: {}", e);
                }
            }
            _ => {}
        }
    }

    // Save roles when someone leaves
    async fn guild_member_removal(
        &self,
        ctx: Context,
        guild_id: GuildId,
        user: User,
        member_data_if_available: Option<Member>,
    ) {
        let guild_id_num = guild_id.get();
        let user_tag = user.tag();

        let role_ids: Vec<u64> = if let Some(member) = member_data_if_available {
            // If we have member data, use it directly
            let roles: Vec<u64> = member.roles.iter().map(|r| r.get()).collect();
            eprintln!(
                "[{}] Member data provided with {} roles for {} on removal",
                guild_id_num,
                roles.len(),
                user_tag
            );
            roles
        } else {
            // Fallback: try to get from cache
            // Note: We cannot fetch from API when member has left the guild (will return "Unknown Member")
            let cached_result = ctx.cache.guild(guild_id).and_then(|guild| {
                guild
                    .members
                    .get(&user.id)
                    .map(|m| m.roles.iter().map(|r| r.get()).collect::<Vec<u64>>())
            });

            if let Some(roles) = cached_result.clone() {
                eprintln!(
                    "[{}] Got {} roles from cache for {} on removal",
                    guild_id_num,
                    roles.len(),
                    user_tag
                );
                roles
            } else {
                eprintln!(
                    "[{}] No member data and not in cache for {} on removal",
                    guild_id_num, user_tag
                );
                vec![]
            }
        };

        if !role_ids.is_empty() {
            eprintln!(
                "[{}] Saving {} roles for {}",
                guild_id_num,
                role_ids.len(),
                user_tag
            );
            if let Err(e) = self
                .db
                .save_user_roles(guild_id_num, user.id.get(), &role_ids)
            {
                eprintln!(
                    "[{}] Failed to save roles for {}: {}",
                    guild_id_num, user_tag, e
                );
            }
        } else {
            eprintln!("[{}] No roles to save for {}", guild_id_num, user_tag);
        }
    }

    // Restore roles when someone rejoins
    async fn guild_member_addition(&self, ctx: Context, member: Member) {
        let guild_id = member.guild_id.get();
        let user_tag = member.user.tag();

        match self.db.get_user_roles(guild_id, member.user.id.get()) {
            Ok(Some(saved_roles)) => {
                eprintln!(
                    "[{}] Found {} saved roles for {}",
                    guild_id,
                    saved_roles.len(),
                    user_tag
                );

                // Build role hierarchy and permission data (must be done before await)
                let (bot_highest_role_position, roles_map) = {
                    if let Some(guild) = ctx.cache.guild(member.guild_id) {
                        let mut roles_map: std::collections::HashMap<u64, (String, i64, bool)> =
                            std::collections::HashMap::new();
                        let mut bot_highest_role_position: i64 = i64::MIN;
                        let bot_id = ctx.cache.current_user().id;

                        // Find bot's member to get their role hierarchy position
                        if let Some(bot_member) = guild.members.get(&bot_id) {
                            for role_id in &bot_member.roles {
                                if let Some(role) = guild.roles.get(role_id) {
                                    bot_highest_role_position =
                                        bot_highest_role_position.max(role.position as i64);
                                }
                            }
                            if bot_highest_role_position != i64::MIN {
                                eprintln!(
                                    "[{}] Bot highest role position: {}",
                                    guild_id, bot_highest_role_position
                                );
                            }
                        }

                        // Fallback if bot not found or has no roles - set to MAX so all normal roles can be assigned
                        if bot_highest_role_position == i64::MIN {
                            eprintln!("[{}] Bot not found in cache or has no roles, allowing all assignable roles", guild_id);
                            bot_highest_role_position = i64::MAX;
                        }

                        // Build role info map
                        for role in guild.roles.values() {
                            let role_id = role.id.get();
                            let has_admin = role
                                .permissions
                                .contains(serenity::all::Permissions::ADMINISTRATOR);
                            roles_map.insert(
                                role_id,
                                (role.name.clone(), role.position as i64, has_admin),
                            );
                        }

                        (bot_highest_role_position, roles_map)
                    } else {
                        eprintln!("[{}] Guild not found in cache", guild_id);
                        (i64::MAX, std::collections::HashMap::new())
                    }
                };

                // Now use the extracted data for filtering
                let filter_admin = self.db.get_filter_admin_roles(guild_id).unwrap_or(true);
                eprintln!(
                    "[{}] Admin role filter enabled: {} (true=allow admin, false=block admin)",
                    guild_id, filter_admin
                );

                // Debug: log the roles that will be checked
                for role_id in &saved_roles {
                    if let Some((name, position, has_admin)) = roles_map.get(role_id) {
                        eprintln!(
                            "[{}]   Role to check: {} (id: {}, pos: {}, admin: {})",
                            guild_id, name, role_id, position, has_admin
                        );
                    }
                }

                match self.db.filter_roles_for_restoration_with_permissions(
                    guild_id,
                    &saved_roles,
                    bot_highest_role_position,
                    &roles_map,
                    filter_admin,
                ) {
                    Ok(filtered_roles) => {
                        eprintln!(
                            "[{}] Filtered roles: {} -> {} (removed {} roles)",
                            guild_id,
                            saved_roles.len(),
                            filtered_roles.len(),
                            saved_roles.len() - filtered_roles.len()
                        );
                        for role_id in &filtered_roles {
                            if let Some((name, _, _)) = roles_map.get(role_id) {
                                eprintln!("[{}]   Will assign: {} ({})", guild_id, name, role_id);
                            }
                        }

                        if !filtered_roles.is_empty() {
                            let roles_to_give: Vec<RoleId> =
                                filtered_roles.iter().map(|&id| RoleId::new(id)).collect();

                            if !roles_to_give.is_empty() {
                                match member.add_roles(&ctx.http, &roles_to_give).await {
                                    Ok(_) => {
                                        // Log the restoration if a log channel is configured
                                        if let Ok(Some(log_channel_id)) =
                                            self.db.get_log_channel(guild_id)
                                        {
                                            let role_mentions: Vec<String> = roles_to_give
                                                .iter()
                                                .map(|r| format!("<@&{}>", r.get()))
                                                .collect();
                                            let roles_text = role_mentions.join(", ");

                                            let author = CreateEmbedAuthor::new(&member.user.name)
                                                .icon_url(member.user.avatar_url().unwrap_or_else(
                                                    || member.user.default_avatar_url(),
                                                ));

                                            let embed = CreateEmbed::new()
                                                .author(author)
                                                .title("Roles Restored")
                                                .description(roles_text)
                                                .colour(0x2ecc71);

                                            if let Err(e) = ChannelId::new(log_channel_id)
                                                .send_message(
                                                    &ctx.http,
                                                    CreateMessage::new().embed(embed),
                                                )
                                                .await
                                            {
                                                eprintln!("Failed to send log message: {}", e);
                                            }
                                        }

                                        if let Err(e) = self
                                            .db
                                            .delete_user_roles(guild_id, member.user.id.get())
                                        {
                                            eprintln!("Failed to delete user roles: {}", e);
                                        }
                                    }
                                    Err(e) => {
                                        eprintln!(
                                            "Failed to restore roles for {}: {}",
                                            member.user.tag(),
                                            e
                                        );
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("[{}] Advanced role filtering failed: {}, falling back to basic filtering", guild_id, e);
                        // Fallback to basic filtering if permission check fails
                        if let Ok(filtered_roles) =
                            self.db.filter_roles_for_restoration(guild_id, &saved_roles)
                        {
                            if !filtered_roles.is_empty() {
                                let roles_to_give: Vec<RoleId> =
                                    filtered_roles.iter().map(|&id| RoleId::new(id)).collect();

                                if !roles_to_give.is_empty() {
                                    match member.add_roles(&ctx.http, &roles_to_give).await {
                                        Ok(_) => {
                                            // Log the restoration if a log channel is configured
                                            if let Ok(Some(log_channel_id)) =
                                                self.db.get_log_channel(guild_id)
                                            {
                                                let role_mentions: Vec<String> = roles_to_give
                                                    .iter()
                                                    .map(|r| format!("<@&{}>", r.get()))
                                                    .collect();
                                                let roles_text = role_mentions.join(", ");

                                                let author =
                                                    CreateEmbedAuthor::new(&member.user.name)
                                                        .icon_url(
                                                            member
                                                                .user
                                                                .avatar_url()
                                                                .unwrap_or_else(|| {
                                                                    member.user.default_avatar_url()
                                                                }),
                                                        );

                                                let embed = CreateEmbed::new()
                                                    .author(author)
                                                    .title("Roles Restored")
                                                    .description(roles_text)
                                                    .colour(0x2ecc71);

                                                if let Err(e) = ChannelId::new(log_channel_id)
                                                    .send_message(
                                                        &ctx.http,
                                                        CreateMessage::new().embed(embed),
                                                    )
                                                    .await
                                                {
                                                    eprintln!("Failed to send log message: {}", e);
                                                }
                                            }

                                            if let Err(e) = self
                                                .db
                                                .delete_user_roles(guild_id, member.user.id.get())
                                            {
                                                eprintln!("Failed to delete user roles: {}", e);
                                            }
                                        }
                                        Err(e) => {
                                            eprintln!(
                                                "Failed to restore roles for {}: {}",
                                                member.user.tag(),
                                                e
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Ok(None) => {
                eprintln!("[{}] No saved roles found for {}", guild_id, user_tag);
            }
            Err(e) => {
                eprintln!(
                    "[{}] Failed to retrieve saved roles for {}: {}",
                    guild_id, user_tag, e
                );
            }
        }
    }
}

#[tokio::main]
async fn main() {
    // Ensure output is flushed immediately
    std::io::stdout().flush().ok();

    let token = env::var("DISCORD_TOKEN").expect("Please set DISCORD_TOKEN environment variable");

    // Initialize database
    let db_path = env::var("DB_PATH").unwrap_or_else(|_| "discord_roles.db".to_string());
    let db = match Database::init(&db_path) {
        Ok(db) => Arc::new(db),
        Err(e) => {
            eprintln!("❌ Failed to initialize database: {}", e);
            std::io::stderr().flush().ok();
            std::process::exit(1);
        }
    };

    // Start HTTP health check server first (Render needs to detect this port)
    let http_state = http_server::ServerState::new();
    let http_state_clone = http_state.clone();
    tokio::spawn(async move {
        http_server::start_http_server(http_state_clone).await;
    });

    // Give HTTP server a moment to bind
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    println!("🤖 Starting Discord bot...");
    std::io::stdout().flush().ok();

    let handler = Handler { db };
    let intents = GatewayIntents::GUILD_MEMBERS
        | GatewayIntents::GUILDS
        | GatewayIntents::GUILD_MESSAGE_REACTIONS;

    eprintln!("[DEBUG] Building Discord client with intents...");
    std::io::stderr().flush().ok();

    let mut client = match Client::builder(&token, intents)
        .event_handler(handler)
        .await
    {
        Ok(client) => {
            println!("✅ Discord client created successfully!");
            std::io::stdout().flush().ok();
            client
        }
        Err(e) => {
            eprintln!("❌ Failed to create Discord client: {}", e);
            std::io::stderr().flush().ok();
            std::process::exit(1);
        }
    };

    println!("✅ Attempting to connect to Discord Gateway...");
    std::io::stdout().flush().ok();

    match client.start().await {
        Ok(_) => {
            println!("✅ Discord bot connected successfully");
            std::io::stdout().flush().ok();
        }
        Err(e) => {
            eprintln!("❌ Discord bot connection failed: {}", e);
            eprintln!("Error type: {}", e);
            std::io::stderr().flush().ok();
            std::process::exit(1);
        }
    }
}
