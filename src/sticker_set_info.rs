use bytes::Bytes;
use serde::Serialize;
use teloxide::types::{Sticker, StickerSet};

#[derive(Serialize)]
pub(crate) struct StickerSetInfo {
    name: String,
    title: String,
    kind: StickerSetKind,
    // contains_masks: bool, // FIXME: do we need to interact with masks in any way?...
    stickers: Vec<StickerInfo>,
}

#[derive(Serialize)]
enum StickerSetKind {
    Common,
    Animated,
    Video,
}

#[derive(Serialize)]
struct StickerInfo {
    path: String,
    file_unique_id: String,
    width: u16,
    height: u16,
    emoji: Option<String>,
    size_bytes: u32,
    // mask_position: Option<MaskPosition>, // FIXME: see above
}

impl StickerSetInfo {
    pub(crate) fn new(set: &StickerSet, stickers: &[(String, Vec<Bytes>)]) -> StickerSetInfo {
        StickerSetInfo {
            name: set.name.clone(),
            title: set.title.clone(),
            kind: match (set.is_animated, set.is_video) {
                (true, _) => StickerSetKind::Animated,
                (_, true) => StickerSetKind::Video,
                (_, _) => StickerSetKind::Common,
            },
            stickers: set
                .stickers
                .iter()
                .zip(stickers)
                .map(
                    |(
                        &Sticker {
                            ref file_unique_id,
                            width,
                            height,
                            ref emoji,
                            ..
                        },
                        (path, bytes),
                    )| StickerInfo {
                        path: path.clone(),
                        file_unique_id: file_unique_id.clone(),
                        width,
                        height,
                        emoji: emoji.clone(),
                        size_bytes: bytes.iter().map(|b| b.len() as u32).sum(),
                    },
                )
                .collect(),
        }
    }
}
