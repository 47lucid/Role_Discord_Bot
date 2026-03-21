use serenity::{
    all::{
        Context, CreateCommand,
        CommandInteraction, CreateInteractionResponse, CreateInteractionResponseMessage,
        ComponentInteraction, CreateSelectMenuOption, CreateSelectMenu, CreateEmbed, EditInteractionResponse, ChannelType, Permissions, EditMessage, CreateActionRow, CreateButton, ButtonStyle,
    },
};
use std::sync::Arc;
use crate::db::Database;
use tokio::time::{sleep, Duration};

pub async fn register_commands(ctx: &Context) -> serenity::Result<()> {
    let commands = vec![
        CreateCommand::new("setup")
            .description("Configure safe and avoid roles for automatic restoration and set the log channel")
            .default_member_permissions(serenity::all::Permissions::MANAGE_ROLES),
    ];

    serenity::all::Command::set_global_commands(ctx, commands).await?;
    Ok(())
}

/// Get channels the bot can actually message in (text channels with proper permissions)
async fn get_bot_accessible_text_channels(ctx: &Context, guild_id: serenity::all::GuildId) -> Vec<CreateSelectMenuOption> {
    get_bot_accessible_text_channels_with_default(ctx, guild_id, None).await
}

/// Get bot-accessible text channels with optional default selection
async fn get_bot_accessible_text_channels_with_default(ctx: &Context, guild_id: serenity::all::GuildId, default_channel_id: Option<u64>) -> Vec<CreateSelectMenuOption> {
    // Try to get bot user from cache
    let bot_id = ctx.cache.current_user().id;
    
    // Try to fetch all guild channels
    let channels = match guild_id.channels(&ctx.http).await {
        Ok(chs) => chs,
        Err(_) => return vec![],
    };

    // Filter channels: only text channels the bot can see and message in
    channels.values()
        .filter_map(|ch| {
            // Only text channels
            if ch.kind != ChannelType::Text {
                return None;
            }
            
            // Check bot permissions in this channel
            match ch.permissions_for_user(&ctx.cache, bot_id) {
                Ok(perms) => {
                    // Bot needs VIEW_CHANNEL and SEND_MESSAGES
                    if perms.contains(Permissions::VIEW_CHANNEL) && perms.contains(Permissions::SEND_MESSAGES) {
                        let mut opt = CreateSelectMenuOption::new(ch.name.clone(), ch.id.to_string());
                        // Set as default if this matches the provided default_channel_id
                        if let Some(default_id) = default_channel_id {
                            if ch.id.get() == default_id {
                                opt = opt.default_selection(true);
                            }
                        }
                        Some(opt)
                    } else {
                        None
                    }
                }
                Err(_) => None,
            }
        })
        .collect()
}

/// Schedule automatic disabling of dropdown components after 5 minutes of no interaction
fn schedule_component_timeout(ctx: serenity::all::Context, channel_id: serenity::all::ChannelId, message_id: serenity::all::MessageId) {
    tokio::spawn(async move {
        // Wait 5 minutes with no interaction
        sleep(Duration::from_secs(300)).await;
        
        // Try to edit the message and disable all components (but keep them visible)
        // Ignore errors - message may have been deleted or is no longer accessible
        let _ = channel_id.edit_message(&ctx.http, message_id, EditMessage::new().components(vec![])).await;
    });
}

pub async fn handle_setup_command(
    ctx: &Context,
    command: &CommandInteraction,
    db: Arc<Database>,
) -> serenity::Result<()> {
    let guild_id = match command.guild_id {
        Some(id) => id,
        None => {
            let embed = CreateEmbed::new()
                .title("Command Error")
                .description("This command must be used in a server")
                .colour(0xe74c3c); // Red
            
            command
                .create_response(ctx, CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::new()
                        .embed(embed)
                ))
                .await?;
            return Ok(());
        }
    };
    
    // Check if user is server owner or has admin permission
    let guild = guild_id.to_partial_guild(&ctx.http).await?;
    let is_owner = command.user.id == guild.owner_id;
    let has_admin = command.member.as_ref()
        .and_then(|m| m.permissions)
        .map(|p| p.contains(serenity::all::Permissions::ADMINISTRATOR))
        .unwrap_or(false);
    
    if !is_owner && !has_admin {
        let embed = CreateEmbed::new()
            .title("Permission Denied")
            .description("Only server administrators and the server owner can use this command.")
            .colour(0xe74c3c);
        
        command
            .create_response(ctx, CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .embed(embed)
                    .ephemeral(true)
            ))
            .await?;
        return Ok(());
    }
    
    // Get all roles in the server
    let role_options: Vec<CreateSelectMenuOption> = guild
        .roles
        .values()
        .filter(|r| !r.managed && r.name != "@everyone") // Exclude managed and everyone roles
        .map(|r| {
            let mut option = CreateSelectMenuOption::new(r.name.clone(), r.id.to_string());
            
            // Add description if role has admin permissions (won't be auto-restored)
            if r.permissions.contains(serenity::all::Permissions::ADMINISTRATOR) {
                option = option.description("⚠️ Admin role - won't be auto-restored for security");
            }
            option
        })
        .collect();

    if role_options.is_empty() {
        let embed = CreateEmbed::new()
            .title("No Roles Available")
            .description("Create some roles in this server first before configuring them for automatic restoration.")
            .colour(0xe74c3c); // Red
        
        command
            .create_response(ctx, CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .embed(embed)
            ))
            .await?;
        return Ok(());
    }

    // Get current configuration
    let safe_roles = db.get_safe_roles(guild_id.get())
        .unwrap_or_default();
    let avoid_roles = db.get_avoid_roles(guild_id.get())
        .unwrap_or_default();
    let log_channel = db.get_log_channel(guild_id.get())
        .unwrap_or_default();
    let filter_admin_roles = db.get_filter_admin_roles(guild_id.get())
        .unwrap_or(true);

    // Fetch role mentions for safe roles
    let mut safe_role_mentions = Vec::new();
    for &id in &safe_roles {
        safe_role_mentions.push(format!("<@&{}>", id));
    }
    
    // Fetch role mentions for avoid roles
    let mut avoid_role_mentions = Vec::new();
    for &id in &avoid_roles {
        avoid_role_mentions.push(format!("<@&{}>", id));
    }
    
    let safe_roles_display = if safe_role_mentions.is_empty() {
        "None configured".to_string()
    } else {
        safe_role_mentions.join(" ")
    };
    
    let avoid_roles_display = if avoid_role_mentions.is_empty() {
        "None configured".to_string()
    } else {
        avoid_role_mentions.join(" ")
    };

    let log_channel_display = if let Some(ch_id) = log_channel {
        format!("<#{}>", ch_id)
    } else {
        "Not configured".to_string()
    };

    // Dynamically set max values based on available roles (Discord max is 25)
    let max_values = std::cmp::min(role_options.len() as u8, 25);

    // Get user ID who executed the command
    let user_id = command.user.id;
    let user_id_str = user_id.to_string();

    // Create select menus with user ID in custom_id to restrict to command user only
    let safe_select = CreateSelectMenu::new(
        format!("safe_roles_select_{}", user_id_str),
        serenity::all::CreateSelectMenuKind::String {
            options: role_options.clone()
        },
    )
    .placeholder("Select roles to ALLOW automatic restoration")
    .max_values(max_values)
    .min_values(0);

    let avoid_select = CreateSelectMenu::new(
        format!("avoid_roles_select_{}", user_id_str),
        serenity::all::CreateSelectMenuKind::String {
            options: role_options.clone()
        },
    )
    .placeholder("Select roles to PREVENT automatic restoration")
    .max_values(max_values)
    .min_values(0);

    let channel_options = get_bot_accessible_text_channels_with_default(ctx, guild_id, log_channel).await;
    
    let log_channel_select = CreateSelectMenu::new(
        format!("log_channel_select_{}", user_id_str),
        serenity::all::CreateSelectMenuKind::String {
            options: channel_options,
        },
    )
    .placeholder("Select or clear log channel")
    .min_values(0)
    .max_values(1);

    let safe_row = serenity::all::CreateActionRow::SelectMenu(safe_select);
    let avoid_row = serenity::all::CreateActionRow::SelectMenu(avoid_select);
    let log_row = serenity::all::CreateActionRow::SelectMenu(log_channel_select);

    // Admin filter toggle button
    let admin_filter_status = if filter_admin_roles { "✅ Enabled" } else { "❌ Disabled" };
    let admin_filter_button = CreateButton::new(format!("admin_filter_toggle_{}", user_id_str))
        .label(format!("Re-assign Admin Roles: {}", admin_filter_status))
        .style(if filter_admin_roles { ButtonStyle::Success } else { ButtonStyle::Danger });
    let button_row = serenity::all::CreateActionRow::Buttons(vec![admin_filter_button]);

    let mut embed = CreateEmbed::new()
        .title("Setup")
        .description("Configure which roles should be automatically restored when members rejoin the server.")
        .field("Safe Roles", safe_roles_display, false)
        .field("Avoid Roles", avoid_roles_display, false)
        .field("Log Channel", log_channel_display, false);
    
    // Only show security rules if admin filter is disabled
    if !filter_admin_roles {
        embed = embed.field("Security Rules", 
                       "• Roles with **Admin** permission won't be auto-restored",
                       false);
    }
    
    embed = embed.colour(0x8660e2); // Gold/Amber
        

    command
        .create_response(ctx, CreateInteractionResponse::Message(
            CreateInteractionResponseMessage::new()
                .embed(embed)
                .components(vec![safe_row, avoid_row, log_row, button_row])
        ))
        .await?;
    
    // Get the message to get its ID for scheduling timeout
    if let Ok(message) = command.get_response(ctx).await {
        schedule_component_timeout(ctx.clone(), message.channel_id, message.id);
    }

    Ok(())
}

pub async fn handle_admin_filter_toggle(
    ctx: &Context,
    interaction: &ComponentInteraction,
    db: Arc<Database>,
) -> serenity::Result<()> {
    let guild_id = match interaction.guild_id {
        Some(id) => id,
        None => {
            let embed = CreateEmbed::new()
                .title("Error")
                .description("This can only be used in a server")
                .colour(0xe74c3c);
            
            interaction
                .create_response(ctx, CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::new()
                        .embed(embed)
                        .ephemeral(true)
                ))
                .await?;
            return Ok(());
        }
    };

    // Extract the original command user ID from the custom_id
    let custom_id = &interaction.data.custom_id;
    let prefix = "admin_filter_toggle_";
    let _user_id_str = if let Some(uid_str) = custom_id.strip_prefix(prefix) {
        if let Ok(original_user_id) = uid_str.parse::<u64>() {
            if original_user_id != interaction.user.id.get() {
                let embed = CreateEmbed::new()
                    .title("Permission Denied")
                    .description("Only the user who executed the `/setup` command can modify these settings.")
                    .colour(0xe74c3c);
                
                interaction
                    .create_response(ctx, CreateInteractionResponse::Message(
                        CreateInteractionResponseMessage::new()
                            .embed(embed)
                            .ephemeral(true)
                    ))
                    .await?;
                return Ok(());
            }
            uid_str.to_string()
        } else {
            return Ok(());
        }
    } else {
        return Ok(());
    };

    // Get current filter status and toggle it
    let current_filter = db.get_filter_admin_roles(guild_id.get()).unwrap_or(true);
    let new_filter = !current_filter;

    // Save the toggled filter status
    match db.set_filter_admin_roles(guild_id.get(), new_filter) {
        Ok(_) => {
            // Defer the interaction to allow time for response
            interaction.defer(ctx).await?;
            
            // Fetch guild for role information
            let guild = guild_id.to_partial_guild(&ctx.http).await.ok();
            
            // Get all current configuration
            let safe_roles = db.get_safe_roles(guild_id.get()).unwrap_or_default();
            let safe_role_mentions: Vec<String> = safe_roles.iter().map(|id| format!("<@&{}>", id)).collect();
            
            let avoid_roles = db.get_avoid_roles(guild_id.get()).unwrap_or_default();
            let avoid_role_mentions: Vec<String> = avoid_roles.iter().map(|id| format!("<@&{}>", id)).collect();
            
            let log_channel = db.get_log_channel(guild_id.get()).unwrap_or_default();
            
            let safe_roles_display = if safe_role_mentions.is_empty() {
                "None configured".to_string()
            } else {
                safe_role_mentions.join(" ")
            };
            
            let avoid_roles_display = if avoid_role_mentions.is_empty() {
                "None configured".to_string()
            } else {
                avoid_role_mentions.join(" ")
            };
            
            let log_channel_display = if let Some(ch_id) = log_channel {
                format!("<#{}>", ch_id)
            } else {
                "Not configured".to_string()
            };
            
            // Create updated embed
            let mut updated_embed = CreateEmbed::new()
                .title("Setup")
                .description("Configure which roles should be automatically restored when members rejoin the server.")
                .field("Safe Roles", safe_roles_display, false)
                .field("Avoid Roles", avoid_roles_display, false)
                .field("Log Channel", log_channel_display, false);
            
            // Only show security rules if admin filter is disabled (after toggle)
            if !new_filter {
                updated_embed = updated_embed.field("Security Rules", 
                       "• Roles with **Admin** permission won't be auto-restored",
                       false);
            }
            
            updated_embed = updated_embed.colour(0x8660e2);
            
            // Recreate all select menus with current selections
            let safe_roles_set: std::collections::HashSet<u64> = safe_roles.iter().cloned().collect();
            let avoid_roles_set: std::collections::HashSet<u64> = avoid_roles.iter().cloned().collect();
            
            let all_role_options: Vec<CreateSelectMenuOption> = guild
                .as_ref()
                .map(|g| {
                    g.roles
                        .values()
                        .filter(|r| !r.managed && r.name != "@everyone")
                        .map(|r| {
                            let role_id = r.id.get();
                            let role_id_str = r.id.to_string();
                            let mut option = CreateSelectMenuOption::new(r.name.clone(), role_id_str);
                            
                            // Mark as selected if it's in the safe roles list
                            if safe_roles_set.contains(&role_id) {
                                option = option.default_selection(true);
                            }
                            option
                        })
                        .collect()
                })
                .unwrap_or_default();
            
            let all_role_options_avoid: Vec<CreateSelectMenuOption> = guild
                .as_ref()
                .map(|g| {
                    g.roles
                        .values()
                        .filter(|r| !r.managed && r.name != "@everyone")
                        .map(|r| {
                            let role_id = r.id.get();
                            let role_id_str = r.id.to_string();
                            let mut option = CreateSelectMenuOption::new(r.name.clone(), role_id_str);
                            
                            // Mark as selected if it's in the avoid roles list
                            if avoid_roles_set.contains(&role_id) {
                                option = option.default_selection(true);
                            }
                            option
                        })
                        .collect()
                })
                .unwrap_or_default();
            
            let max_values = std::cmp::min(all_role_options.len() as u8, 25);
            
            let safe_select = CreateSelectMenu::new(
                format!("safe_roles_select_{}", _user_id_str),
                serenity::all::CreateSelectMenuKind::String {
                    options: all_role_options
                },
            )
            .placeholder("Select roles to ALLOW automatic restoration")
            .max_values(max_values)
            .min_values(0);

            let avoid_select = CreateSelectMenu::new(
                format!("avoid_roles_select_{}", _user_id_str),
                serenity::all::CreateSelectMenuKind::String {
                    options: all_role_options_avoid
                },
            )
            .placeholder("Select roles to PREVENT automatic restoration")
            .max_values(max_values)
            .min_values(0);

            let channel_options = get_bot_accessible_text_channels(ctx, guild_id).await;
            
            let log_channel_select = CreateSelectMenu::new(
                format!("log_channel_select_{}", _user_id_str),
                serenity::all::CreateSelectMenuKind::String {
                    options: channel_options,
                },
            )
            .placeholder("Select or clear log channel")
            .min_values(0)
            .max_values(1);

            let safe_row = serenity::all::CreateActionRow::SelectMenu(safe_select);
            let avoid_row = serenity::all::CreateActionRow::SelectMenu(avoid_select);
            let log_row = serenity::all::CreateActionRow::SelectMenu(log_channel_select);
            
            // Create the updated button with NEW color state
            let admin_filter_status = if new_filter { "✅ Enabled" } else { "❌ Disabled" };
            let admin_filter_button = CreateButton::new(format!("admin_filter_toggle_{}", _user_id_str))
                .label(format!("Re-assign Admin Roles: {}", admin_filter_status))
                .style(if new_filter { ButtonStyle::Success } else { ButtonStyle::Danger });
            let button_row = serenity::all::CreateActionRow::Buttons(vec![admin_filter_button]);
            
            // Edit the original message with updated embed and components
            interaction.edit_response(ctx, EditInteractionResponse::new()
                .embed(updated_embed)
                .components(vec![safe_row, avoid_row, log_row, button_row])
            ).await?;
        }
        Err(_) => {
            let embed = CreateEmbed::new()
                .title("Error")
                .description("Failed to save the admin filter setting. Please try again.")
                .colour(0xe74c3c);
            
            interaction
                .create_response(ctx, CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::new()
                        .embed(embed)
                        .ephemeral(true)
                ))
                .await?;
        }
    }

    Ok(())
}

pub async fn handle_safe_roles_select(
    ctx: &Context,
    interaction: &ComponentInteraction,
    db: Arc<Database>,
) -> serenity::Result<()> {
    let guild_id = match interaction.guild_id {
        Some(id) => id,
        None => {
            let embed = CreateEmbed::new()
                .title("Error")
                .description("This can only be used in a server")
                .colour(0xe74c3c);
            
            interaction
                .create_response(ctx, CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::new()
                        .embed(embed)
                        .ephemeral(true)
                ))
                .await?;
            return Ok(());
        }
    };
    
    // Extract the original command user ID from the custom_id
    let custom_id = &interaction.data.custom_id;
    let prefix = "safe_roles_select_";
    let user_id_str = if let Some(uid_str) = custom_id.strip_prefix(prefix) {
        if let Ok(original_user_id) = uid_str.parse::<u64>() {
            if original_user_id != interaction.user.id.get() {
                let embed = CreateEmbed::new()
                    .title("Permission Denied")
                    .description("Only the user who executed the `/setup` command can modify these settings.")
                    .colour(0xe74c3c);
                
                interaction
                    .create_response(ctx, CreateInteractionResponse::Message(
                        CreateInteractionResponseMessage::new()
                            .embed(embed)
                            .ephemeral(true)
                    ))
                    .await?;
                return Ok(());
            }
            uid_str.to_string()
        } else {
            return Ok(());
        }
    } else {
        return Ok(());
    };
    
    let role_ids: Vec<u64> = match &interaction.data.kind {
        serenity::all::ComponentInteractionDataKind::StringSelect { values } => {
            values
                .iter()
                .filter_map(|v: &String| v.parse::<u64>().ok())
                .collect()
        }
        _ => {
            vec![]
        }
    };
    
    
    match db.set_safe_roles(guild_id.get(), &role_ids) {
        Ok(_) => {
            // Remove any safe roles from avoid list (mutually exclusive)
            let mut avoid_roles = db.get_avoid_roles(guild_id.get()).unwrap_or_default();
            let safe_roles_set: std::collections::HashSet<u64> = role_ids.iter().cloned().collect();
            avoid_roles.retain(|r| !safe_roles_set.contains(r));
            let _ = db.set_avoid_roles(guild_id.get(), &avoid_roles);
            
            // Fetch guild and role information for the updated embed
            let guild = guild_id.to_partial_guild(&ctx.http).await.ok();
            
            // Get updated safe roles
            let safe_roles = db.get_safe_roles(guild_id.get()).unwrap_or_default();
            let safe_role_mentions: Vec<String> = safe_roles.iter().map(|id| format!("<@&{}>", id)).collect();
            
            // Get avoid roles
            let avoid_roles = db.get_avoid_roles(guild_id.get()).unwrap_or_default();
            let avoid_role_mentions: Vec<String> = avoid_roles.iter().map(|id| format!("<@&{}>", id)).collect();
            
            // Get log channel
            let log_channel = db.get_log_channel(guild_id.get()).unwrap_or_default();
            
            let safe_roles_display = if safe_role_mentions.is_empty() {
                "None configured".to_string()
            } else {
                safe_role_mentions.join(" ")
            };
            
            let avoid_roles_display = if avoid_role_mentions.is_empty() {
                "None configured".to_string()
            } else {
                avoid_role_mentions.join(" ")
            };
            
            let log_channel_display = if let Some(ch_id) = log_channel {
                format!("<#{}>", ch_id)
            } else {
                "Not configured".to_string()
            };
            
            // Get filter_admin_roles state
            let filter_admin_roles = db.get_filter_admin_roles(guild_id.get()).unwrap_or(true);
            
            // Create updated main embed
            let mut updated_embed = CreateEmbed::new()
                .title("Setup")
        .description("Configure which roles should be automatically restored when members rejoin the server.")
        .field("Safe Roles", safe_roles_display, false)
        .field("Avoid Roles", avoid_roles_display, false)
        .field("Log Channel", log_channel_display, false);
        
            // Only show security rules if admin filter is disabled
            if !filter_admin_roles {
                updated_embed = updated_embed.field("Security Rules", 
                       "• Roles with **Admin** permission won't be auto-restored",
                       false);
            }
            
            updated_embed = updated_embed.colour(0x8660e2); // Gold/Amber
            
            // Recreate select menus with updated state, marking selected roles
            let safe_roles_set: std::collections::HashSet<u64> = safe_roles.iter().cloned().collect();
            let avoid_roles_set: std::collections::HashSet<u64> = avoid_roles.iter().cloned().collect();
            
            let all_role_options: Vec<CreateSelectMenuOption> = guild
                .as_ref()
                .map(|g| {
                    g.roles
                        .values()
                        .filter(|r| !r.managed && r.name != "@everyone")
                        .map(|r| {
                            let role_id = r.id.get();
                            let role_id_str = r.id.to_string();
                            let mut option = CreateSelectMenuOption::new(r.name.clone(), role_id_str);
                            
                            // Mark as selected if it's in the safe roles list
                            if safe_roles_set.contains(&role_id) {
                                option = option.default_selection(true);
                            }
                            option
                        })
                        .collect()
                })
                .unwrap_or_default();
            
            let all_role_options_avoid: Vec<CreateSelectMenuOption> = guild
                .as_ref()
                .map(|g| {
                    g.roles
                        .values()
                        .filter(|r| !r.managed && r.name != "@everyone")
                        .map(|r| {
                            let role_id = r.id.get();
                            let role_id_str = r.id.to_string();
                            let mut option = CreateSelectMenuOption::new(r.name.clone(), role_id_str);
                            
                            // Mark as selected if it's in the avoid roles list
                            if avoid_roles_set.contains(&role_id) {
                                option = option.default_selection(true);
                            }
                            option
                        })
                        .collect()
                })
                .unwrap_or_default();
            
            let max_values = std::cmp::min(all_role_options.len() as u8, 25);
            
            let safe_select = CreateSelectMenu::new(
                format!("safe_roles_select_{}", user_id_str),
                serenity::all::CreateSelectMenuKind::String {
                    options: all_role_options
                },
            )
            .placeholder("Select roles to ALLOW automatic restoration")
            .max_values(max_values)
            .min_values(0);

            let avoid_select = CreateSelectMenu::new(
                format!("avoid_roles_select_{}", user_id_str),
                serenity::all::CreateSelectMenuKind::String {
                    options: all_role_options_avoid
                },
            )
            .placeholder("Select roles to PREVENT automatic restoration")
            .max_values(max_values)
            .min_values(0);

            let channel_options = get_bot_accessible_text_channels(ctx, guild_id).await;
            
            let log_channel_select = CreateSelectMenu::new(
                format!("log_channel_select_{}", user_id_str),
                serenity::all::CreateSelectMenuKind::String {
                    options: channel_options,
                },
            )
            .placeholder("Select or clear log channel")
            .min_values(0)
            .max_values(1);

            let safe_row = serenity::all::CreateActionRow::SelectMenu(safe_select);
            let avoid_row = serenity::all::CreateActionRow::SelectMenu(avoid_select);
            let log_row = serenity::all::CreateActionRow::SelectMenu(log_channel_select);
            
            // Create the admin filter button
            let filter_admin_roles = db.get_filter_admin_roles(guild_id.get()).unwrap_or(true);
            let admin_filter_status = if filter_admin_roles { "✅ Enabled" } else { "❌ Disabled" };
            let admin_filter_button = CreateButton::new(format!("admin_filter_toggle_{}", user_id_str))
                .label(format!("Re-assign Admin Roles: {}", admin_filter_status))
                .style(if filter_admin_roles { ButtonStyle::Success } else { ButtonStyle::Danger });
            let button_row = serenity::all::CreateActionRow::Buttons(vec![admin_filter_button]);
            
            // Edit the original message with updated embed and components
            interaction.defer(ctx).await?;
            interaction.edit_response(ctx, EditInteractionResponse::new()
                .embed(updated_embed)
                .components(vec![safe_row, avoid_row, log_row, button_row])
            ).await?;
            
            // Schedule timeout for this updated message (5 minutes of inactivity from now)
            schedule_component_timeout(ctx.clone(), interaction.channel_id, interaction.message.id);
        }
        Err(_) => {
            let embed = CreateEmbed::new()
                .title("Configuration Failed")
                .description("Failed to save the safe roles configuration. Please try again.")
                .colour(0xe74c3c);
            
            interaction
                .create_response(ctx, CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::new()
                        .embed(embed)
                        .ephemeral(true)
                ))
                .await?;
        }
    }

    Ok(())
}

pub async fn handle_avoid_roles_select(
    ctx: &Context,
    interaction: &ComponentInteraction,
    db: Arc<Database>,
) -> serenity::Result<()> {
    let guild_id = match interaction.guild_id {
        Some(id) => id,
        None => {
            let embed = CreateEmbed::new()
                .title("Error")
                .description("This can only be used in a server")
                .colour(0xe74c3c);
            
            interaction
                .create_response(ctx, CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::new()
                        .embed(embed)
                        .ephemeral(true)
                ))
                .await?;
            return Ok(());
        }
    };
    
    // Extract the original command user ID from the custom_id
    let custom_id = &interaction.data.custom_id;
    let prefix = "avoid_roles_select_";
    let user_id_str = if let Some(uid_str) = custom_id.strip_prefix(prefix) {
        if let Ok(original_user_id) = uid_str.parse::<u64>() {
            if original_user_id != interaction.user.id.get() {
                let embed = CreateEmbed::new()
                    .title("Permission Denied")
                    .description("Only the user who executed the `/setup` command can modify these settings.")
                    .colour(0xe74c3c);
                
                interaction
                    .create_response(ctx, CreateInteractionResponse::Message(
                        CreateInteractionResponseMessage::new()
                            .embed(embed)
                            .ephemeral(true)
                    ))
                    .await?;
                return Ok(());
            }
            uid_str.to_string()
        } else {
            return Ok(());
        }
    } else {
        return Ok(());
    };
    
    let role_ids: Vec<u64> = match &interaction.data.kind {
        serenity::all::ComponentInteractionDataKind::StringSelect { values } => {
            values
                .iter()
                .filter_map(|v: &String| v.parse::<u64>().ok())
                .collect()
        }
        _ => {
            vec![]
        }
    };
    
    
    match db.set_avoid_roles(guild_id.get(), &role_ids) {
        Ok(_) => {
            // Remove any avoid roles from safe list (mutually exclusive)
            let mut safe_roles = db.get_safe_roles(guild_id.get()).unwrap_or_default();
            let avoid_roles_set: std::collections::HashSet<u64> = role_ids.iter().cloned().collect();
            safe_roles.retain(|r| !avoid_roles_set.contains(r));
            let _ = db.set_safe_roles(guild_id.get(), &safe_roles);
            
            // Fetch guild and role information for the updated embed
            let guild = guild_id.to_partial_guild(&ctx.http).await.ok();
            
            // Get safe roles
            let safe_roles = db.get_safe_roles(guild_id.get()).unwrap_or_default();
            let safe_role_mentions: Vec<String> = safe_roles.iter().map(|id| format!("<@&{}>", id)).collect();
            
            // Get updated avoid roles
            let avoid_roles = db.get_avoid_roles(guild_id.get()).unwrap_or_default();
            let avoid_role_mentions: Vec<String> = avoid_roles.iter().map(|id| format!("<@&{}>", id)).collect();
            
            // Get log channel
            let log_channel = db.get_log_channel(guild_id.get()).unwrap_or_default();
            
            let safe_roles_display = if safe_role_mentions.is_empty() {
                "None configured".to_string()
            } else {
                safe_role_mentions.join(" ")
            };
            
            let avoid_roles_display = if avoid_role_mentions.is_empty() {
                "None configured".to_string()
            } else {
                avoid_role_mentions.join(" ")
            };
            
            let log_channel_display = if let Some(ch_id) = log_channel {
                format!("<#{}>", ch_id)
            } else {
                "Not configured".to_string()
            };
            
            // Get filter_admin_roles state
            let filter_admin_roles = db.get_filter_admin_roles(guild_id.get()).unwrap_or(true);
            
            // Create updated main embed
            let mut updated_embed = CreateEmbed::new()
                .title("Setup")
        .description("Configure which roles should be automatically restored when members rejoin the server.")
        .field("Safe Roles (Will Restore)", safe_roles_display, false)
        .field("Avoid Roles (Won't Restore)", avoid_roles_display, false)
        .field("Log Channel", log_channel_display, false);
        
            // Only show security rules if admin filter is disabled
            if !filter_admin_roles {
                updated_embed = updated_embed.field("Security Rules", 
                       "• Roles with **Admin** permission won't be auto-restored",
                       false);
            }
            
            updated_embed = updated_embed.colour(0x8660e2); // Gold/Amber
            
            // Recreate select menus with updated state, marking selected roles
            let safe_roles_set: std::collections::HashSet<u64> = safe_roles.iter().cloned().collect();
            let avoid_roles_set: std::collections::HashSet<u64> = avoid_roles.iter().cloned().collect();
            
            let all_role_options: Vec<CreateSelectMenuOption> = guild
                .as_ref()
                .map(|g| {
                    g.roles
                        .values()
                        .filter(|r| !r.managed && r.name != "@everyone")
                        .map(|r| {
                            let role_id = r.id.get();
                            let role_id_str = r.id.to_string();
                            let mut option = CreateSelectMenuOption::new(r.name.clone(), role_id_str);
                            
                            // Mark as selected if it's in the safe roles list
                            if safe_roles_set.contains(&role_id) {
                                option = option.default_selection(true);
                            }
                            option
                        })
                        .collect()
                })
                .unwrap_or_default();
            
            let all_role_options_avoid: Vec<CreateSelectMenuOption> = guild
                .as_ref()
                .map(|g| {
                    g.roles
                        .values()
                        .filter(|r| !r.managed && r.name != "@everyone")
                        .map(|r| {
                            let role_id = r.id.get();
                            let role_id_str = r.id.to_string();
                            let mut option = CreateSelectMenuOption::new(r.name.clone(), role_id_str);
                            
                            // Mark as selected if it's in the avoid roles list
                            if avoid_roles_set.contains(&role_id) {
                                option = option.default_selection(true);
                            }
                            option
                        })
                        .collect()
                })
                .unwrap_or_default();
            
            let max_values = std::cmp::min(all_role_options.len() as u8, 25);
            
            let safe_select = CreateSelectMenu::new(
                format!("safe_roles_select_{}", user_id_str),
                serenity::all::CreateSelectMenuKind::String {
                    options: all_role_options
                },
            )
            .placeholder("Select roles to ALLOW automatic restoration")
            .max_values(max_values)
            .min_values(0);

            let avoid_select = CreateSelectMenu::new(
                format!("avoid_roles_select_{}", user_id_str),
                serenity::all::CreateSelectMenuKind::String {
                    options: all_role_options_avoid
                },
            )
            .placeholder("Select roles to PREVENT automatic restoration")
            .max_values(max_values)
            .min_values(0);

            let log_channel_select = CreateSelectMenu::new(
                format!("log_channel_select_{}", user_id_str),
                serenity::all::CreateSelectMenuKind::Channel { default_channels: None, channel_types: Some(vec![serenity::all::ChannelType::Text]) },
            )
            .placeholder("Select or clear log channel")
            .min_values(0)
            .max_values(1);

            let safe_row = serenity::all::CreateActionRow::SelectMenu(safe_select);
            let avoid_row = serenity::all::CreateActionRow::SelectMenu(avoid_select);
            let log_row = serenity::all::CreateActionRow::SelectMenu(log_channel_select);
            
            // Create the admin filter button
            let filter_admin_roles = db.get_filter_admin_roles(guild_id.get()).unwrap_or(true);
            let admin_filter_status = if filter_admin_roles { "✅ Enabled" } else { "❌ Disabled" };
            let admin_filter_button = CreateButton::new(format!("admin_filter_toggle_{}", user_id_str))
                .label(format!("Re-assign Admin Roles: {}", admin_filter_status))
                .style(if filter_admin_roles { ButtonStyle::Success } else { ButtonStyle::Danger });
            let button_row = serenity::all::CreateActionRow::Buttons(vec![admin_filter_button]);
            
            // Edit the original message with updated embed and components
            interaction.defer(ctx).await?;
            interaction.edit_response(ctx, EditInteractionResponse::new()
                .embed(updated_embed)
                .components(vec![safe_row, avoid_row, log_row, button_row])
            ).await?;
            
            // Schedule timeout for this updated message (5 minutes of inactivity from now)
            schedule_component_timeout(ctx.clone(), interaction.channel_id, interaction.message.id);
        }
        Err(_) => {
            let embed = CreateEmbed::new()
                .title("Configuration Failed")
                .description("Failed to save the avoid roles configuration. Please try again.")
                .colour(0xe74c3c);
            
            interaction
                .create_response(ctx, CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::new()
                        .embed(embed)
                        .ephemeral(true)
                ))
                .await?;
        }
    }

    Ok(())
}

pub async fn handle_log_channel_select(
    ctx: &Context,
    interaction: &ComponentInteraction,
    db: Arc<Database>,
) -> serenity::Result<()> {
    let guild_id = match interaction.guild_id {
        Some(id) => id,
        None => {
            let embed = CreateEmbed::new()
                .title("Error")
                .description("This can only be used in a server")
                .colour(0xe74c3c);
            
            interaction
                .create_response(ctx, CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::new()
                        .embed(embed)
                        .ephemeral(true)
                ))
                .await?;
            return Ok(());
        }
    };
    
    // Extract the original command user ID from the custom_id
    let custom_id = &interaction.data.custom_id;
    let prefix = "log_channel_select_";
    let user_id_str = if let Some(uid_str) = custom_id.strip_prefix(prefix) {
        if let Ok(original_user_id) = uid_str.parse::<u64>() {
            if original_user_id != interaction.user.id.get() {
                let embed = CreateEmbed::new()
                    .title("Permission Denied")
                    .description("Only the user who executed the `/setup` command can modify these settings.")
                    .colour(0xe74c3c);
                
                interaction
                    .create_response(ctx, CreateInteractionResponse::Message(
                        CreateInteractionResponseMessage::new()
                            .embed(embed)
                            .ephemeral(true)
                    ))
                    .await?;
                return Ok(());
            }
            uid_str.to_string()
        } else {
            return Ok(());
        }
    } else {
        return Ok(());
    };
    
    let channel_id = match &interaction.data.kind {
        serenity::all::ComponentInteractionDataKind::ChannelSelect { values } => {
            values.first().map(|ch| ch.get())
        }
        serenity::all::ComponentInteractionDataKind::StringSelect { values } => {
            // Handle String select menu with channel IDs as string values
            values.first().and_then(|ch_id_str| ch_id_str.parse::<u64>().ok())
        }
        _ => None,
    };
    
    match db.set_log_channel(guild_id.get(), channel_id) {
        Ok(_) => {
            // Fetch guild and role information for the updated embed
            let guild = guild_id.to_partial_guild(&ctx.http).await.ok();
            
            // Get safe roles
            let safe_roles = db.get_safe_roles(guild_id.get()).unwrap_or_default();
            let safe_role_mentions: Vec<String> = safe_roles.iter().map(|id| format!("<@&{}>", id)).collect();
            
            // Get avoid roles
            let avoid_roles = db.get_avoid_roles(guild_id.get()).unwrap_or_default();
            let avoid_role_mentions: Vec<String> = avoid_roles.iter().map(|id| format!("<@&{}>", id)).collect();
            
            // Get log channel
            let log_channel = db.get_log_channel(guild_id.get()).unwrap_or_default();
            
            let safe_roles_display = if safe_role_mentions.is_empty() {
                "None configured".to_string()
            } else {
                safe_role_mentions.join(" ")
            };
            
            let avoid_roles_display = if avoid_role_mentions.is_empty() {
                "None configured".to_string()
            } else {
                avoid_role_mentions.join(" ")
            };
            
            let log_channel_display = if let Some(ch_id) = log_channel {
                format!("<#{}>", ch_id)
            } else {
                "Not configured".to_string()
            };
            
            // Get filter_admin_roles state
            let filter_admin_roles = db.get_filter_admin_roles(guild_id.get()).unwrap_or(true);
            
            // Create updated main embed
            let mut updated_embed = CreateEmbed::new()
                .title("Setup")
                .description("Configure which roles should be automatically restored when members rejoin the server.")
                .field("Safe Roles", safe_roles_display, false)
                .field("Avoid Roles", avoid_roles_display, false)
                .field("Log Channel", log_channel_display, false);
            
            // Only show security rules if admin filter is disabled
            if !filter_admin_roles {
                updated_embed = updated_embed.field("Security Rules", 
                       "• Roles with **Admin** permission won't be auto-restored",
                       false);
            }
            
            updated_embed = updated_embed.colour(0x8660e2);
            
            // Recreate select menus with updated state
            let safe_roles_set: std::collections::HashSet<u64> = safe_roles.iter().cloned().collect();
            let avoid_roles_set: std::collections::HashSet<u64> = avoid_roles.iter().cloned().collect();
            
            let all_role_options: Vec<CreateSelectMenuOption> = guild
                .as_ref()
                .map(|g| {
                    g.roles
                        .values()
                        .filter(|r| !r.managed && r.name != "@everyone")
                        .map(|r| {
                            let role_id = r.id.get();
                            let role_id_str = r.id.to_string();
                            let mut option = CreateSelectMenuOption::new(r.name.clone(), role_id_str);
                            
                            if safe_roles_set.contains(&role_id) {
                                option = option.default_selection(true);
                            }
                            option
                        })
                        .collect()
                })
                .unwrap_or_default();
            
            let all_role_options_avoid: Vec<CreateSelectMenuOption> = guild
                .as_ref()
                .map(|g| {
                    g.roles
                        .values()
                        .filter(|r| !r.managed && r.name != "@everyone")
                        .map(|r| {
                            let role_id = r.id.get();
                            let role_id_str = r.id.to_string();
                            let mut option = CreateSelectMenuOption::new(r.name.clone(), role_id_str);
                            
                            if avoid_roles_set.contains(&role_id) {
                                option = option.default_selection(true);
                            }
                            option
                        })
                        .collect()
                })
                .unwrap_or_default();
            
            let max_values = std::cmp::min(all_role_options.len() as u8, 25);
            
            // Get all accessible channels for the log channel dropdown with current selection as default
            let log_channel_options = get_bot_accessible_text_channels_with_default(ctx, guild_id, log_channel).await;
            
            let safe_select = CreateSelectMenu::new(
                format!("safe_roles_select_{}", user_id_str),
                serenity::all::CreateSelectMenuKind::String {
                    options: all_role_options
                },
            )
            .placeholder("Select roles to ALLOW automatic restoration")
            .max_values(max_values)
            .min_values(0);

            let avoid_select = CreateSelectMenu::new(
                format!("avoid_roles_select_{}", user_id_str),
                serenity::all::CreateSelectMenuKind::String {
                    options: all_role_options_avoid
                },
            )
            .placeholder("Select roles to PREVENT automatic restoration")
            .max_values(max_values)
            .min_values(0);

            let log_channel_select = CreateSelectMenu::new(
                format!("log_channel_select_{}", user_id_str),
                serenity::all::CreateSelectMenuKind::String {
                    options: log_channel_options
                },
            )
            .placeholder("Select or clear log channel")
            .min_values(0)
            .max_values(1);

            let safe_row = serenity::all::CreateActionRow::SelectMenu(safe_select);
            let avoid_row = serenity::all::CreateActionRow::SelectMenu(avoid_select);
            let log_row = serenity::all::CreateActionRow::SelectMenu(log_channel_select);
            
            // Create the admin filter button
            let filter_admin_roles = db.get_filter_admin_roles(guild_id.get()).unwrap_or(true);
            let admin_filter_status = if filter_admin_roles { "✅ Enabled" } else { "❌ Disabled" };
            let admin_filter_button = CreateButton::new(format!("admin_filter_toggle_{}", user_id_str))
                .label(format!("Re-assign Admin Roles: {}", admin_filter_status))
                .style(if filter_admin_roles { ButtonStyle::Success } else { ButtonStyle::Danger });
            let button_row = serenity::all::CreateActionRow::Buttons(vec![admin_filter_button]);
            
            // Edit the original message with updated embed and components
            interaction.defer(ctx).await?;
            interaction.edit_response(ctx, EditInteractionResponse::new()
                .embed(updated_embed)
                .components(vec![safe_row, avoid_row, log_row, button_row])
            ).await?;
            
            // Schedule timeout for this updated message (5 minutes of inactivity from now)
            schedule_component_timeout(ctx.clone(), interaction.channel_id, interaction.message.id);
        }
        Err(_) => {
            let embed = CreateEmbed::new()
                .title("Configuration Failed")
                .description("Failed to set the log channel. Please try again.")
                .colour(0xe74c3c);
            
            interaction
                .create_response(ctx, CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::new()
                        .embed(embed)
                        .ephemeral(true)
                ))
                .await?;
        }
    }
    
    Ok(())
}
