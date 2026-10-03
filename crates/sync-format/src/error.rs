/// Why an entry's bytes gave no facts for its kind.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A `PROP` bin that does not split.
    #[error("bin: {0}")]
    Bin(String),
    /// A bin object that ltk_meta does not read, or ltk_ritobin does not print.
    #[error("bin object: {0}")]
    Object(String),
    /// A sound bank that does not parse.
    #[error("bank: {0}")]
    Bank(String),
    /// A skeleton that does not parse.
    #[error("skeleton: {0}")]
    Skeleton(String),
    /// A mesh that does not parse.
    #[error("mesh: {0}")]
    Mesh(String),
    /// A texture that does not parse.
    #[error("texture: {0}")]
    Texture(String),
}
