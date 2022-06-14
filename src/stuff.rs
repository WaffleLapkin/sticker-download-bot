//! Random stuff lives here.

use std::io::Write;

use emojis::Emoji;
use teloxide::types::InputFile;
use unicode_segmentation::UnicodeSegmentation;
use zip::{result::ZipError, write::FileOptions, ZipWriter};

/// Archive files together making a `.zip` file.
///
/// Files are represented by a `(name, bytes)` tuple.
pub fn archive(name: &str, files: Vec<(String, Vec<u8>)>) -> Result<InputFile, ZipError> {
    let zip = {
        // Technically this does a blocking write.
        // But since it writes to memory and does not do compression, it takes negligible time (around 5ms).
        // So it doesn't seem to make sense to use `spawn_blocking` here.

        let mut zip = ZipWriter::new(std::io::Cursor::new(Vec::with_capacity(0 /* FIXME */)));

        let options = FileOptions::default()
            // Compressing images is pointless because they are already compressed.
            //
            // From my non-exhaustive testing using default `deflate` compression
            // makes archiving 29 times slower while making the resulting .zip a little bit bigger.
            .compression_method(zip::CompressionMethod::Stored);

        for (name, bytes) in files {
            zip.start_file(name, options)?;
            zip.write_all(&bytes)?;
        }

        zip.finish()?.into_inner()
    };

    let archive_name = format!("{}.zip", name);
    let file = InputFile::memory(zip).file_name(archive_name);

    Ok(file)
}

/// Returns a file name for a sticker that is `idx`-th in its sticker pack (`None` if it isn't in any) given its associated `emojis`.
pub fn sticker_name(idx: Option<u8>, emojis: &str) -> String {
    let name = emojis
        .graphemes(true)
        .flat_map(|cluster| emojis::get(cluster))
        .map(Emoji::name)
        .next()
        .unwrap_or_else(|| /* FIXME: warn */ "malformed_emoji")
        .replace(' ', "_");

    match idx {
        Some(idx) => format!("{idx:03}_{name}"),
        None => name,
    }
}
