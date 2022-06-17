// Status:
// - Basic functionality (downloading sticker packs) works!
// - Converting to .png is implemented, but
//   - Is not rate limited (it's quite easy to DDOS the bot)
// - For some reason the bot is slow (need to check why)
// - Messages/interface are very much work in progress
// - The code is quite bad in some places/wip
// - Zips do not have thumbnails

mod download;
mod error;
mod progress;
mod query_command;
mod sticker_set_info;
mod stuff;

use std::future::ready;

use futures::{stream, StreamExt, TryStreamExt};
use lodepng::RGBA;
use teloxide::{
    adaptors::{DefaultParseMode, Throttle},
    dispatching::{update_listeners::polling, MessageFilterExt, UpdateHandler},
    dptree::{self, deps},
    payloads::SendDocumentSetters,
    prelude::{AutoSend, Dispatcher, RequesterExt},
    types::{CallbackQuery, ChatAction::UploadDocument, InputFile, ParseMode, StickerSet, Update},
    utils::command::parse_command,
    RequestError,
};
use teloxide::{
    dispatching::UpdateFilterExt,
    payloads::setters::*,
    prelude::Requester,
    types::{InlineKeyboardButton, InlineKeyboardMarkup, Me, Message, Sticker},
};

use crate::{
    download::{Downloader, Task, Tasks},
    error::{callback_query::CallbackQueryError, Error, ResultExt},
    progress::{KiB, Progress},
    query_command::{ActionDownload, DownloadFormat, DownloadTarget, QueryAction, QueryCommand},
    stuff::{archive, sticker_name},
};

type Bot = AutoSend<DefaultParseMode<Throttle<teloxide::Bot>>>;

fn main() {
    pretty_env_logger::init();

    let test = true;

    // Using single-thread runtime is not really needed, I could use multi-thread runtime here.
    // However, since I don't expect this bot to be used much, I can save some VPS resources (?probably).
    //
    // I don't use `#[tokio::main]` to reduce macros & magic used and speedup compilation a little bit
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let _rt_guard = rt.enter();

    let bot = teloxide::Bot::from_env()
        // This will protect the bot from Telegram limits, if it ever reaches them
        .throttle(<_>::default())
        // Set default parse mode
        .parse_mode(ParseMode::Html)
        // Allow using `.await` without `.send()` or requests
        .auto_send();

    let mut dp = Dispatcher::builder(bot.clone(), dispatch_tree())
        .distribution_function(|_| None::<()>)
        .dependencies(deps![Downloader::new(bot.clone())])
        .build();
    dp.setup_ctrlc_handler();

    if test {
        let listener = polling(bot, Some(std::time::Duration::from_secs(1)), None, None);

        rt.block_on(async {
            dp.dispatch_with_listener(
                listener,
                teloxide::error_handlers::LoggingErrorHandler::new(),
            )
            .await
        });

        return;
    }

    rt.block_on(async { dp.dispatch().await });
}

fn dispatch_tree() -> UpdateHandler<RequestError> {
    dptree::entry()
        .branch(
            Update::filter_message()
                .branch(Message::filter_sticker().endpoint(sticker))
                .branch(Message::filter_text().endpoint(text)),
        )
        .branch(Update::filter_callback_query().endpoint(callback_query))
}

async fn sticker(bot: Bot, message: Message) -> Result<(), RequestError> {
    let download_png = InlineKeyboardButton::callback(
        "sticker as .png",
        QueryCommand::download(DownloadTarget::Single, DownloadFormat::Png).encode(),
    );
    let download_webp = InlineKeyboardButton::callback(
        "sticker as .webp",
        QueryCommand::download(DownloadTarget::Single, DownloadFormat::Webp).encode(),
    );
    let download_png_set = InlineKeyboardButton::callback(
        "set as .png",
        QueryCommand::download(DownloadTarget::All, DownloadFormat::Png).encode(),
    );
    let download_webp_set = InlineKeyboardButton::callback(
        "set as .webp",
        QueryCommand::download(DownloadTarget::All, DownloadFormat::Webp).encode(),
    );

    let kb = InlineKeyboardMarkup::new([
        [download_png_set, download_webp_set],
        [download_png, download_webp],
    ]);

    bot.send_message(message.chat.id, "What do you want to download?")
        .reply_markup(kb)
        .reply_to_message_id(message.id)
        .await?;

    Ok(())
}

async fn text(bot: Bot, text: String, message: Message, me: Me) -> Result<(), RequestError> {
    let chat_id = message.chat.id;

    // We could use teloxide derive macros for commands, but for just /start and /help that's a bit of an overkill.
    if let Some((command, _args)) = parse_command(&text, me.username()) {
        match command {
            "start" => {
                bot.send_message(chat_id, format!("start (TODO)")).await?;
            }
            "help" => {
                bot.send_message(chat_id, format!("help (TODO)")).await?;
            }
            _ => {
                bot.send_message(
                    chat_id,
                    format!(
                        "Unknown command `{command}`, see /help for the list of available commands"
                    ),
                )
                .await?;
            }
        }

        return Ok(());
    }

    bot.send_message(
        chat_id,
        format!(
            "Use /help for the list of available commands and instructions on how to use the bot"
        ),
    )
    .await?;

    Ok(())
}

async fn callback_query(bot: Bot, query: CallbackQuery, d: Downloader) -> Result<(), RequestError> {
    match callback_query_inner(&bot, &query, d).await {
        Ok(()) => Ok(()),
        Err(Error::Req(e)) => Err(e),
        Err(Error::Show(e)) if !e.is_post() => {
            bot.answer_callback_query(query.id)
                .text(format!("Error: {e}"))
                .show_alert(true)
                .await?;

            Ok(())
        }
        Err(Error::Show(e)) => {
            if let Some(m) = &query.message {
                bot.edit_message_text(m.chat.id, m.id, format!("Error: {e}"))
                    .await?;
            }
            Ok(())
        }
    }
}

async fn callback_query_inner(
    bot: &Bot,
    query: &CallbackQuery,
    d: Downloader,
) -> Result<(), Error<CallbackQueryError>> {
    use error::callback_query as err;

    let data = match &query.data {
        Some(data) => data,
        None => {
            return Ok(());
        }
    };

    let command = match QueryCommand::decode(data) {
        Some(c) => c,
        None => return err::invalid_button_data(data),
    };

    let QueryAction::Download(action) = command.action;
    callback_query_download(bot, action, query, d).await?;

    Ok(())
}

async fn callback_query_download(
    bot: &Bot,
    action: ActionDownload,
    query: &CallbackQuery,
    d: Downloader,
) -> Result<(), Error<CallbackQueryError>> {
    use error::callback_query as err;
    use error::downloading::SendDocumentError;

    let message = query.message.as_ref().ok_or_else(err::no_message)?;
    let reply = message.reply_to_message().ok_or_else(err::empty_reply)?;
    let sticker = reply
        .sticker()
        .ok_or_else(err::reply_is_not_sticker)
        .and_then(check_supported_sticker)?;

    let mut progress = Progress::new(
        &bot,
        "Queueing download request...",
        message.chat.id,
        message.id,
    );

    let sticker_set_name = sticker.set_name.clone();
    let (tasks, set) =
        prepare_download_tasks(bot, message.id, sticker, action, &mut progress).await?;
    let total_size = tasks.total_size();

    let stream = d.download(tasks, action.target)?;

    bot.answer_callback_query(&query.id).await?;

    let bot = bot.clone();
    let chat_id = message.chat.id;
    let message_id = message.id;
    let reply_message_id = reply.id;

    let mut scope = progress
        .scope("Downloading stickers", total_size as _)
        .with_unit(KiB);

    let mut stickers: Vec<_> = stream
        .map(|(file_name, res)| {
            res.map(|bytes| {
                scope.inc_by(bytes.len() as _);
                (file_name, bytes)
            })
        })
        .try_collect()
        .await?;

    match action.format {
        DownloadFormat::Png => {
            let a = tokio::task::spawn_blocking(|| {
                let mut scope = progress.scope("Converting stickers to .png", stickers.len() as _);

                for (_file_name, bytes) in &mut stickers {
                    let (w, h, raw) = libwebp::WebPDecodeRGBA(bytes).unwrap();

                    *bytes =
                        lodepng::encode32(bytemuck::cast_slice::<u8, RGBA>(&*raw), w as _, h as _)
                            .unwrap();

                    scope.inc();
                }

                (progress, stickers)
            })
            .await
            .unwrap();

            progress = a.0;
            stickers = a.1;
        } // FIXME: not fine
        DownloadFormat::Webp => {}
    }

    // FIXME: fix the message when downloading a single sticker
    progress.title_imp("Uploading sticker set");

    bot.send_chat_action(chat_id, UploadDocument).await.fine();

    let file = if stickers.len() == 1 && action.format.is_fine_for_sending_alone() {
        let (name, bytes) = stickers.pop().unwrap();
        InputFile::memory(bytes).file_name(name)
    } else {
        if let Some(set) = &set {
            stickers.push((
                "sticker_info.json".to_owned(),
                serde_json::to_vec_pretty(&sticker_set_info::StickerSetInfo::new(&set, &stickers))
                    .unwrap(), // FIXME: unwrap bad
            ));
        }

        let zip = archive(sticker_set_name.as_deref().unwrap_or("stickers"), stickers);

        let file = match zip {
            Ok(z) => z,
            _ => return Ok(()), // FIXME
        };

        file
    };

    bot.send_document(chat_id, file)
        .caption(format_caption(set.as_ref()))
        .reply_to_message_id(reply_message_id)
        .await
        .map_err(SendDocumentError)?;

    bot.delete_message(chat_id, message_id).await.fine();

    Ok(())
}

async fn prepare_download_tasks(
    bot: &Bot,
    message_id: i32,
    sticker: &Sticker,
    ActionDownload { target, format }: ActionDownload,
    progress: &mut Progress,
) -> Result<(Tasks, Option<StickerSet>), Error<CallbackQueryError>> {
    let set = match &sticker.set_name {
        Some(name) => Some(bot.get_sticker_set(name).await?),
        None => None,
    };

    let named_and_identified = match (target, set.as_ref()) {
        (DownloadTarget::Single, set) | (DownloadTarget::All, set @ None) => {
            let idx = set.and_then(|set| {
                set.stickers
                    .iter()
                    .enumerate()
                    .find(|(_, s)| s.file_unique_id == sticker.file_unique_id)
                    .map(|(i, _)| i as u8)
            });

            vec![(
                sticker_name(idx, sticker.emoji.as_deref().unwrap_or_default()),
                sticker.file_id.clone(),
            )]
        }
        (DownloadTarget::All, Some(set)) => set
            .stickers
            .iter()
            .enumerate()
            .map(|(idx, s)| {
                (
                    sticker_name(Some(idx as u8), s.emoji.as_deref().unwrap_or_default()),
                    s.file_id.clone(),
                )
            })
            .collect(),
    };

    let mut scope = progress.scope("Fetching sticker info", named_and_identified.len() as _);

    let mut stickers = Vec::new();
    stream::iter(named_and_identified)
        .map(|(name, file_id)| async {
            bot.get_file(file_id).await.map(|f| Task {
                path: f.file_path,
                name,
                size: f.file_size as usize,
            })
        })
        .buffer_unordered(16 /* FIXME: choose constant */)
        //.try_collect()
        .try_for_each(|task| {
            stickers.push(task);
            scope.inc();

            ready(Ok(()))
        })
        .await?;

    let tasks = Tasks {
        message_id,
        format,
        stickers,
    };

    Ok((tasks, set))
}

fn check_supported_sticker(sticker: &Sticker) -> Result<&Sticker, Error<CallbackQueryError>> {
    use error::callback_query as err;

    match sticker {
        // FIXME: ideally we would simply either
        //        A) support animated/video stickers
        //        B) answer w/ error when the sticker is sent, not when the button is pressed
        s if s.is_animated => Err(err::animated_sticker_not_supported()),
        s if s.is_video => Err(err::video_sticker_not_supported()),
        s => Ok(s),
    }
}

fn format_caption(set: Option<&StickerSet>) -> String {
    set.map(|ss| {
        use teloxide::utils::html::*;

        let title = bold(&escape(&ss.title));
        let count = bold(&ss.stickers.len().to_string());
        format!("Stickers set: {title}\nStickers in set: {count}")
    })
    .unwrap_or_default()
}
