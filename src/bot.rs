use crate::{
    config::Config,
    db, text,
    voteban::{ActiveVoteban, VoteCounts, VoteResult},
};
use sqlx::SqlitePool;
use std::error::Error;
use std::{collections::HashMap, sync::Arc};
use teloxide::{
    dispatching::UpdateHandler,
    payloads::{AnswerCallbackQuerySetters, EditMessageTextSetters, SendMessageSetters},
    prelude::*,
    requests::Requester,
    types::{
        ChatId, ChatMemberStatus, InlineKeyboardButton, InlineKeyboardMarkup, MessageId,
        ReplyParameters, User, UserId,
    },
};
use tokio::sync::Mutex;
use tracing::{error, info};

type HandlerResult = Result<(), Box<dyn Error + Send + Sync>>;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub pool: SqlitePool,
    pub active_votebans: Arc<Mutex<HashMap<i32, ActiveVoteban>>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoteThresholdAction {
    Continue,
    Ban,
    Cancel,
}

pub fn threshold_action(counts: VoteCounts, need_count: usize) -> VoteThresholdAction {
    if counts.for_ban >= need_count {
        VoteThresholdAction::Ban
    } else if counts.against >= need_count {
        VoteThresholdAction::Cancel
    } else {
        VoteThresholdAction::Continue
    }
}

pub fn format_voteban_text(
    vote: &ActiveVoteban,
    pro_usernames: &[String],
    against_usernames: &[String],
    need_count: usize,
) -> String {
    let counts = vote.counts();
    format!(
        "🗳️ Голосування за бан @{}\n\n✅ За ({}/{}): {}\n❌ Проти ({}/{}): {}",
        vote.target_username,
        counts.for_ban,
        need_count,
        if pro_usernames.is_empty() {
            "немає".to_string()
        } else {
            pro_usernames.join(", ")
        },
        counts.against,
        need_count,
        if against_usernames.is_empty() {
            "немає".to_string()
        } else {
            against_usernames.join(", ")
        },
    )
}

pub fn voteban_keyboard(counts: VoteCounts, need_count: usize) -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![vec![
        InlineKeyboardButton::callback(
            format!("✅ За ({}/{})", counts.for_ban, need_count),
            "vote_ban",
        ),
        InlineKeyboardButton::callback(
            format!("❌ Проти ({}/{})", counts.against, need_count),
            "vote_against",
        ),
    ]])
}

pub async fn run(bot: Bot, state: AppState) {
    Dispatcher::builder(bot, schema())
        .dependencies(dptree::deps![state])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;
}

pub fn schema() -> UpdateHandler<Box<dyn Error + Send + Sync>> {
    dptree::entry()
        .branch(Update::filter_message().endpoint(handle_message))
        .branch(Update::filter_callback_query().endpoint(handle_callback_query))
}

pub async fn log_to_admins(bot: &Bot, state: &AppState, message: impl AsRef<str>) {
    let message = message.as_ref();
    info!("{message}");
    for admin_id in &state.config.admin_ids {
        if let Err(err) = bot
            .send_message(ChatId(*admin_id), message.to_string())
            .await
        {
            error!("Помилка при надсиланні повідомлення адміну {admin_id}: {err}");
        }
    }
}

async fn handle_message(bot: Bot, msg: Message, state: AppState) -> HandlerResult {
    if let Some(text) = msg.text() {
        if let Some((command, args)) = parse_command(text) {
            match command.as_str() {
                "addword" => {
                    handle_addword(&bot, &msg, &state, args).await?;
                    return Ok(());
                }
                "listwords" => {
                    handle_listwords(&bot, &msg, &state).await?;
                    return Ok(());
                }
                "removeword" => {
                    handle_removeword(&bot, &msg, &state, args).await?;
                    return Ok(());
                }
                "voteban" => {
                    handle_voteban(&bot, &msg, &state).await?;
                    return Ok(());
                }
                _ => {}
            }
        }
    }

    handle_regular_message(&bot, &msg, &state).await
}

fn parse_command(text: &str) -> Option<(String, &str)> {
    let trimmed = text.trim();
    if !trimmed.starts_with('/') {
        return None;
    }

    let token_end = trimmed.find(char::is_whitespace).unwrap_or(trimmed.len());
    let token = &trimmed[..token_end];
    let args = trimmed[token_end..].trim();
    let command = token
        .trim_start_matches('/')
        .split('@')
        .next()
        .unwrap_or_default()
        .to_lowercase();

    (!command.is_empty()).then_some((command, args))
}

fn user_id_to_i64(user_id: UserId) -> Option<i64> {
    i64::try_from(user_id.0).ok()
}

fn user_username(user: &User, fallback: &str) -> String {
    user.username
        .clone()
        .unwrap_or_else(|| fallback.to_string())
}

fn message_text_for_ban_check<'a>(text: Option<&'a str>, caption: Option<&'a str>) -> &'a str {
    text.or(caption).unwrap_or_default()
}

fn message_text_from_update(msg: &Message) -> &str {
    message_text_for_ban_check(msg.text(), msg.caption())
}

fn is_bot_admin(state: &AppState, user: &User) -> bool {
    user_id_to_i64(user.id)
        .map(|user_id| state.config.is_bot_admin(user_id))
        .unwrap_or(false)
}

async fn handle_addword(bot: &Bot, msg: &Message, state: &AppState, args: &str) -> HandlerResult {
    let Some(from) = msg.from.as_ref() else {
        return Ok(());
    };

    if !is_bot_admin(state, from) {
        bot.send_message(msg.chat.id, "Тільки адміністратори можуть додавати слова.")
            .await?;
        return Ok(());
    }

    let words = text::split_addword_args(args);
    if words.is_empty() {
        bot.send_message(
            msg.chat.id,
            "Будь ласка, вкажіть хоча б одне слово після команди /addword.",
        )
        .await?;
        return Ok(());
    }

    let added_count = db::add_words(&state.pool, &words).await?;
    if added_count > 0 {
        bot.send_message(msg.chat.id, format!("Додано {added_count} нових слів."))
            .await?;
        log_to_admins(
            bot,
            state,
            format!(
                "@{}: Додав {} {}: {}",
                user_username(from, "Користувач"),
                added_count,
                if added_count == 1 {
                    "нове слово"
                } else {
                    "нових слів"
                },
                words.join(",")
            ),
        )
        .await;
    } else {
        bot.send_message(
            msg.chat.id,
            "Жодне нове слово не було додано (можливо, всі вже є в списку).",
        )
        .await?;
    }

    Ok(())
}

async fn handle_listwords(bot: &Bot, msg: &Message, state: &AppState) -> HandlerResult {
    let Some(from) = msg.from.as_ref() else {
        return Ok(());
    };

    if !is_bot_admin(state, from) {
        bot.send_message(msg.chat.id, "Тільки адміністратори можуть дивитися слова.")
            .await?;
        return Ok(());
    }

    let words = db::list_words(&state.pool).await?;
    if words.is_empty() {
        bot.send_message(msg.chat.id, "Список слів порожній.")
            .await?;
    } else {
        bot.send_message(
            msg.chat.id,
            format!("Заборонені слова: {}", words.join(", ")),
        )
        .await?;
    }
    Ok(())
}

async fn handle_removeword(
    bot: &Bot,
    msg: &Message,
    state: &AppState,
    args: &str,
) -> HandlerResult {
    let Some(from) = msg.from.as_ref() else {
        return Ok(());
    };

    if !is_bot_admin(state, from) {
        bot.send_message(msg.chat.id, "Тільки адміністратори можуть видаляти слова.")
            .await?;
        return Ok(());
    }

    let word = args.trim();
    if word.is_empty() {
        bot.send_message(
            msg.chat.id,
            "Будь ласка, вкажіть слово після команди /removeword",
        )
        .await?;
        return Ok(());
    }

    let deleted = db::remove_word(&state.pool, word).await?;
    if deleted > 0 {
        bot.send_message(msg.chat.id, format!("Слово \"{word}\" видалено зі списку."))
            .await?;
        log_to_admins(
            bot,
            state,
            format!(
                "@{}: Видалив слово {word} зі списку.",
                user_username(from, "Користувач")
            ),
        )
        .await;
    } else {
        bot.send_message(msg.chat.id, format!("Слово \"{word}\" не знайдено."))
            .await?;
    }

    Ok(())
}

async fn handle_voteban(bot: &Bot, msg: &Message, state: &AppState) -> HandlerResult {
    if msg.chat.is_private() {
        bot.send_message(msg.chat.id, "Ця команда працює тільки в групових чатах.")
            .await?;
        return Ok(());
    }

    let Some(target_message) = msg.reply_to_message() else {
        bot.send_message(
            msg.chat.id,
            "Будь ласка, відповідьте на повідомлення для /voteban",
        )
        .await?;
        return Ok(());
    };

    let Some(target_user) = target_message.from.as_ref() else {
        bot.send_message(msg.chat.id, "Не вдалося визначити користувача або чат")
            .await?;
        return Ok(());
    };
    let Some(initiator) = msg.from.as_ref() else {
        bot.send_message(msg.chat.id, "Не вдалося визначити користувача або чат")
            .await?;
        return Ok(());
    };

    let Some(target_user_id) = user_id_to_i64(target_user.id) else {
        bot.send_message(msg.chat.id, "Не вдалося визначити користувача або чат")
            .await?;
        return Ok(());
    };
    let Some(initiator_id) = user_id_to_i64(initiator.id) else {
        bot.send_message(msg.chat.id, "Не вдалося визначити користувача або чат")
            .await?;
        return Ok(());
    };

    let target_username = user_username(target_user, "Користувач");
    if is_group_admin(bot, state, msg.chat.id, target_user.id).await {
        log_to_admins(
            bot,
            state,
            format!(
                "@{} спробував банити адміністратора.",
                user_username(initiator, "Користувач")
            ),
        )
        .await;
        if let Err(err) = bot.delete_message(msg.chat.id, msg.id).await {
            error!("Не вдалося видалити повідомлення /voteban: {err}");
        }
        return Ok(());
    }

    if let Err(err) = bot.delete_message(msg.chat.id, msg.id).await {
        error!("Не вдалося видалити повідомлення /voteban: {err}");
    }

    let initiator_username = user_username(initiator, "Користувач");
    let preliminary_vote = ActiveVoteban::new(
        target_user_id,
        target_message.id.0,
        0,
        initiator_id,
        target_username.clone(),
    );
    let text = format!(
        "🗳️ Голосування за бан @{target_username}\n\n✅ За (1/{}): @{initiator_username}\n❌ Проти (0/{}):",
        state.config.voteban_need_count, state.config.voteban_need_count
    );
    let vote_message = bot
        .send_message(msg.chat.id, text)
        .reply_parameters(ReplyParameters::new(target_message.id))
        .reply_markup(voteban_keyboard(
            preliminary_vote.counts(),
            state.config.voteban_need_count,
        ))
        .await?;

    let vote = ActiveVoteban::new(
        target_user_id,
        target_message.id.0,
        vote_message.id.0,
        initiator_id,
        target_username,
    );
    state
        .active_votebans
        .lock()
        .await
        .insert(vote_message.id.0, vote);

    Ok(())
}

async fn handle_regular_message(bot: &Bot, msg: &Message, state: &AppState) -> HandlerResult {
    let Some(from) = msg.from.as_ref() else {
        return Ok(());
    };
    let Some(user_id) = user_id_to_i64(from.id) else {
        return Ok(());
    };
    if is_group_admin(bot, state, msg.chat.id, from.id).await {
        return Ok(());
    }

    let username = user_username(from, "Без імені");

    if let Some(forward_chat) = msg.forward_from_chat() {
        if forward_chat.id != msg.chat.id {
            log_to_admins(
                bot,
                state,
                format!("Переслане повідомлення від @{username} ({user_id}). Блокування."),
            )
            .await;
            ban_user(bot, state, msg.chat.id, from.id, msg.id, from).await;
            return Ok(());
        }
    }

    let text = message_text_from_update(msg);
    match db::contains_word(&state.pool, text).await {
        Ok(true) => {
            log_to_admins(
                bot,
                state,
                format!("Заборонене слово в повідомленні від @{username} ({user_id}). Блокування."),
            )
            .await;
            ban_user(bot, state, msg.chat.id, from.id, msg.id, from).await;
        }
        Ok(false) => {}
        Err(err) => {
            log_to_admins(
                bot,
                state,
                format!("Помилка при перевірці заборонених слів: {err}"),
            )
            .await;
        }
    }

    Ok(())
}

pub async fn is_group_admin(bot: &Bot, state: &AppState, chat_id: ChatId, user_id: UserId) -> bool {
    match bot.get_chat_member(chat_id, user_id).await {
        Ok(member) => matches!(
            member.kind.status(),
            ChatMemberStatus::Administrator | ChatMemberStatus::Owner
        ),
        Err(err) => {
            log_to_admins(
                bot,
                state,
                format!(
                    "Помилка при перевірці прав користувача {}: {err}",
                    user_id.0
                ),
            )
            .await;
            false
        }
    }
}

async fn ban_user(
    bot: &Bot,
    state: &AppState,
    chat_id: ChatId,
    user_id: UserId,
    message_id: MessageId,
    user: &User,
) {
    match async {
        bot.delete_message(chat_id, message_id).await?;
        bot.ban_chat_member(chat_id, user_id).await?;
        Ok::<(), teloxide::RequestError>(())
    }
    .await
    {
        Ok(()) => {
            log_to_admins(
                bot,
                state,
                format!("Користувач {} заблокований.", user_id.0),
            )
            .await;
        }
        Err(err) => {
            log_to_admins(
                bot,
                state,
                format!(
                    "Не вдалось заблокувати користувача @{} {}.",
                    user_username(user, "Без імені"),
                    user_id.0
                ),
            )
            .await;
            log_to_admins(bot, state, format!("Помилка API: {err}")).await;
        }
    }
}

async fn handle_callback_query(bot: Bot, q: CallbackQuery, state: AppState) -> HandlerResult {
    let Some(data) = q.data.as_deref() else {
        return Ok(());
    };
    if data != "vote_ban" && data != "vote_against" {
        return Ok(());
    }

    let Some(message) = q.regular_message() else {
        return Ok(());
    };
    let chat_id = message.chat.id;
    let voteban_message_id = message.id.0;
    let Some(user_id) = user_id_to_i64(q.from.id) else {
        return Ok(());
    };
    let is_ban_vote = data == "vote_ban";

    let vote_result = {
        let mut active = state.active_votebans.lock().await;
        let Some(vote) = active.get_mut(&voteban_message_id) else {
            return Ok(());
        };
        let result = vote.record_vote(user_id, is_ban_vote);
        (result, vote.clone())
    };

    match vote_result.0 {
        VoteResult::TargetCannotVote => {
            bot.answer_callback_query(q.id.clone())
                .text("Ви не можете голосувати за себе!")
                .await?;
            return Ok(());
        }
        VoteResult::AlreadyVoted => {
            bot.answer_callback_query(q.id.clone())
                .text("Ви вже голосували")
                .await?;
            return Ok(());
        }
        VoteResult::Recorded => {
            bot.answer_callback_query(q.id.clone()).await?;
        }
    }

    let vote = vote_result.1;
    update_voteban_message(&bot, chat_id, &vote, state.config.voteban_need_count).await;

    match threshold_action(vote.counts(), state.config.voteban_need_count) {
        VoteThresholdAction::Ban => {
            match async {
                bot.ban_chat_member(chat_id, UserId(vote.target_user_id as u64))
                    .await?;
                bot.delete_message(chat_id, MessageId(vote.target_message_id))
                    .await?;
                bot.delete_message(chat_id, MessageId(vote.voteban_message_id))
                    .await?;
                Ok::<(), teloxide::RequestError>(())
            }
            .await
            {
                Ok(()) => {
                    log_to_admins(
                        &bot,
                        &state,
                        format!(
                            "Користувач @{} заблокований через голосування.",
                            vote.target_username
                        ),
                    )
                    .await;
                }
                Err(err) => {
                    log_to_admins(
                        &bot,
                        &state,
                        format!(
                            "Не вдалося заблокувати користувача {}: {err}",
                            vote.target_username
                        ),
                    )
                    .await;
                }
            }
            state
                .active_votebans
                .lock()
                .await
                .remove(&voteban_message_id);
        }
        VoteThresholdAction::Cancel => {
            match bot
                .delete_message(chat_id, MessageId(vote.voteban_message_id))
                .await
            {
                Ok(_) => {
                    log_to_admins(
                        &bot,
                        &state,
                        format!(
                            "Користувач @{} не був заблокований через голосування.",
                            vote.target_username
                        ),
                    )
                    .await;
                }
                Err(err) => {
                    log_to_admins(
                        &bot,
                        &state,
                        format!("Не вдалося видалити повідомлення з голосуванням: {err}"),
                    )
                    .await;
                }
            }
        }
        VoteThresholdAction::Continue => {}
    }

    Ok(())
}

async fn update_voteban_message(
    bot: &Bot,
    chat_id: ChatId,
    vote: &ActiveVoteban,
    need_count: usize,
) {
    let pro_usernames = usernames_for_voters(bot, chat_id, vote.for_voters()).await;
    let against_usernames = usernames_for_voters(bot, chat_id, vote.against_voters()).await;
    let text = format_voteban_text(vote, &pro_usernames, &against_usernames, need_count);
    let markup = voteban_keyboard(vote.counts(), need_count);

    if let Err(err) = bot
        .edit_message_text(chat_id, MessageId(vote.voteban_message_id), text)
        .reply_markup(markup)
        .await
    {
        let error_text = err.to_string();
        if !error_text.contains("message is not modified") {
            error!("Помилка при оновленні повідомлення: {err}");
        }
    }
}

async fn usernames_for_voters(bot: &Bot, chat_id: ChatId, voters: Vec<i64>) -> Vec<String> {
    let mut usernames = Vec::with_capacity(voters.len());
    for user_id in voters {
        match bot.get_chat_member(chat_id, UserId(user_id as u64)).await {
            Ok(member) => usernames.push(format!(
                "@{}",
                member
                    .user
                    .username
                    .unwrap_or_else(|| "Користувач".to_string())
            )),
            Err(_) => usernames.push("Користувач".to_string()),
        }
    }
    usernames
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voteban::{ActiveVoteban, VoteCounts};

    #[test]
    fn format_voteban_text_preserves_current_message_shape() {
        let vote = ActiveVoteban::new(11, 22, 33, 44, "target".to_string());
        let text = format_voteban_text(&vote, &["@starter".to_string()], &[], 2);
        assert_eq!(
            text,
            "🗳️ Голосування за бан @target\n\n✅ За (1/2): @starter\n❌ Проти (0/2): немає"
        );
    }

    #[test]
    fn message_text_for_ban_check_uses_caption_when_text_is_absent() {
        assert_eq!(
            message_text_for_ban_check(None, Some("caption spam")),
            "caption spam"
        );
        assert_eq!(
            message_text_for_ban_check(Some("plain spam"), Some("caption spam")),
            "plain spam"
        );
        assert_eq!(message_text_for_ban_check(None, None), "");
    }

    #[test]
    fn admin_skip_noise_log_is_not_present() {
        let source = include_str!("bot.rs");
        let noisy_log = format!("{}, {}", "адміністратор", "дії не виконуються");

        assert!(!source.contains(&noisy_log));
    }

    #[test]
    fn vote_threshold_returns_ban_or_cancel_action() {
        assert_eq!(
            threshold_action(
                VoteCounts {
                    for_ban: 2,
                    against: 0
                },
                2
            ),
            VoteThresholdAction::Ban
        );
        assert_eq!(
            threshold_action(
                VoteCounts {
                    for_ban: 1,
                    against: 2
                },
                2
            ),
            VoteThresholdAction::Cancel
        );
        assert_eq!(
            threshold_action(
                VoteCounts {
                    for_ban: 1,
                    against: 1
                },
                2
            ),
            VoteThresholdAction::Continue
        );
    }
}
