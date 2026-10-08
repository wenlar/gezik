//! One compressed file (`notes.txt.gz`, `.xz`, `.bz2`, `.zst`): written as the archive's name
//! without its ending, or as the original name a gzip header keeps.

use std::io::Read;
use std::path::Path;

use flate2::bufread::MultiGzDecoder;
use gezik_core::batch::archive::{Codec, safe_join, single_stem};

use super::io::{decoder, read_full};
use super::{
    ArchiveSource, BUF, Entry, ExtractCx, IoResult, Meta, Volumes, damaged, report_stream, unix_time, unsafe_path,
    write_file,
};

pub(super) struct SingleSource {
    volumes: Volumes,
    codec: Codec,
}

impl SingleSource {
    pub(super) fn new(volumes: Volumes, codec: Codec) -> SingleSource {
        SingleSource { volumes, codec }
    }
}

impl ArchiveSource for SingleSource {
    fn list(&mut self, _cx: &dyn ExtractCx) -> IoResult<Option<Vec<Entry>>> {
        Ok(None)
    }

    fn extract(&mut self, dest: &Path, cx: &dyn ExtractCx) -> IoResult<()> {
        let mut name = single_stem(&self.volumes.name).to_owned();
        let mut meta = Meta::default();
        // The first piece is read before the file is made: a gzip header comes with it.
        let mut first = vec![0u8; BUF];
        let (len, mut stream): (usize, Box<dyn Read>) = if self.codec == Codec::Gz {
            let mut gz = MultiGzDecoder::new(self.volumes.reader()?);
            let len = read_full(&mut gz, &mut first).map_err(damaged)?;
            if let Some(header) = gz.header() {
                if let Some(inner) = header.filename().and_then(|n| std::str::from_utf8(n).ok())
                    && !inner.contains(['/', '\\'])
                    && safe_join(dest, inner).is_some()
                {
                    name = inner.to_owned();
                }
                meta.modified = (header.mtime() > 0).then(|| unix_time(i64::from(header.mtime()))).flatten();
            }
            (len, Box::new(gz))
        } else {
            let mut stream = decoder(self.codec, self.volumes.reader()?);
            (read_full(&mut stream, &mut first).map_err(damaged)?, stream)
        };
        let Some(path) = safe_join(dest, &name) else {
            cx.entry_failed(&name, &unsafe_path());
            return Ok(());
        };
        let mut all = (&first[..len]).chain(&mut stream);
        let result = write_file(&mut all, &path, None, &meta, cx).map(|()| true);
        report_stream(&name, result, cx)
    }
}
