/// Why a manifest, a chunk or a file could not be read.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A file could not be opened or read; `fs_err`'s errors name the path.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// A manifest that does not have the shape of one.
    #[error("manifest: {0}")]
    Manifest(String),
    /// A chunk no source holds.
    #[error("chunk {0:016x} is in no source")]
    MissingChunk(u64),
    /// A chunk whose bytes do not decompress to its size or do not hash to its id.
    #[error("chunk {id:016x}: {message}")]
    BadChunk { id: u64, message: String },
    /// A range outside its file.
    #[error("{offset}+{len} runs past the end of a file of {size} bytes")]
    Range { offset: u64, len: u64, size: u64 },
    /// A download from the CDN that failed, or brought back something else.
    #[error("{url}: {message}")]
    Download { url: String, message: String },
    /// The chunk cache could not be opened, read or written.
    #[error("chunk cache: {0}")]
    Cache(#[from] fjall::Error),
    /// A WAD whose header or table does not have the shape of one.
    #[error("WAD: {0}")]
    Wad(String),
}
