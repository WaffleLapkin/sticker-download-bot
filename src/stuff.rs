use std::io::Write;

use bytes::Bytes;
use teloxide::types::InputFile;
use zip::{result::ZipError, write::FileOptions, ZipWriter};

/// Archive files together making a `.zip` file.
///
/// Files are represented by a `(name, bytes)` tuple.
pub fn archive(name: &str, files: Vec<(String, Vec<Bytes>)>) -> Result<InputFile, ZipError> {
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
            for b in bytes {
                zip.write_all(&b)?;
            }
        }

        zip.finish()?.into_inner()
    };

    let archive_name = format!("{}.zip", name);
    let file = InputFile::memory(zip).file_name(archive_name);

    Ok(file)
}
