use std::{fmt::Debug, panic::Location};

use teloxide::RequestError;

pub enum Error<E> {
    Show(E),
    Req(RequestError),
}

impl<E> From<RequestError> for Error<E> {
    fn from(req: RequestError) -> Self {
        Self::Req(req)
    }
}

pub mod callback_query {
    use std::fmt;

    use crate::{
        error::{downloading::AlreadyDownloading, Error},
        query_command::DownloadTarget,
    };

    pub enum CallbackQueryError {
        InvalidButtonData { data: String },
        NoMessage,
        EmptyReply,
        ReplyIsNotSticker,
        AnimatedStickerNotSupported,
        VideoStickerNotSupported,
        AlreadyDownloading(AlreadyDownloading),
    }

    impl fmt::Display for CallbackQueryError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                CallbackQueryError::InvalidButtonData { data } => {
                    write!(f, "Invalid button data: `{data}`")
                }
                CallbackQueryError::NoMessage => write!(f, "No message? :c"),
                CallbackQueryError::EmptyReply => write!(f, "Reply is empty"),
                CallbackQueryError::ReplyIsNotSticker => write!(f, "Reply is not a sticker"),
                CallbackQueryError::AnimatedStickerNotSupported => {
                    write!(f, "Animated stickers are not yet supported")
                }
                CallbackQueryError::VideoStickerNotSupported => {
                    write!(f, "Video stickers are not yet supported")
                }
                CallbackQueryError::AlreadyDownloading(AlreadyDownloading(target)) => {
                    let what = match target {
                        DownloadTarget::Single => "sticker",
                        DownloadTarget::All => "set",
                    };

                    write!(f, "This {what} is already being downloaded")
                }
            }
        }
    }

    impl From<AlreadyDownloading> for Error<CallbackQueryError> {
        fn from(ad: AlreadyDownloading) -> Self {
            Error::Show(CallbackQueryError::AlreadyDownloading(ad))
        }
    }

    pub fn invalid_button_data(data: &str) -> Result<(), Error<CallbackQueryError>> {
        let data = data.to_owned();
        Err(Error::Show(CallbackQueryError::InvalidButtonData { data }))
    }

    pub fn no_message() -> Error<CallbackQueryError> {
        Error::Show(CallbackQueryError::NoMessage)
    }

    pub fn empty_reply() -> Error<CallbackQueryError> {
        Error::Show(CallbackQueryError::EmptyReply)
    }

    pub fn reply_is_not_sticker() -> Error<CallbackQueryError> {
        Error::Show(CallbackQueryError::ReplyIsNotSticker)
    }

    pub fn animated_sticker_not_supported() -> Error<CallbackQueryError> {
        Error::Show(CallbackQueryError::AnimatedStickerNotSupported)
    }

    pub fn video_sticker_not_supported() -> Error<CallbackQueryError> {
        Error::Show(CallbackQueryError::VideoStickerNotSupported)
    }
}

pub mod downloading {
    use teloxide::RequestError;

    use crate::query_command::DownloadTarget;

    pub struct SendDocumentError(pub RequestError);

    pub struct AlreadyDownloading(pub DownloadTarget);
}

pub trait ResultExt {
    type Item;
    type Err;

    fn fine(self)
    where
        Self::Err: Debug;
}

impl<T, E> ResultExt for Result<T, E> {
    type Item = T;

    type Err = E;

    #[track_caller]
    fn fine(self)
    where
        <Self as ResultExt>::Err: Debug,
    {
        let loc = Location::caller();

        if let Err(err) = self {
            log::error!("Ignoring error @ {loc}: {err:?}");
        }
    }
}
