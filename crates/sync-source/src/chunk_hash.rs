//! Chunk ids: the first eight bytes of a hash of the chunk's uncompressed bytes, little-endian
//! (`docs/SOURCES.md`, "RMAN manifests"). The hash is chosen by the version of the chunking
//! parameters a file uses; a recompressed chunk keeps its id.

use sha2::{Digest, Sha256, Sha512};

/// The hash a chunking parameter set's version selects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChunkHash {
    Sha512,
    Sha256,
    /// PBKDF2-HMAC-SHA256 over the SHA-256 of the bytes, empty salt, 32 rounds.
    RitoHkdf,
    /// From 16.x.
    Blake3,
}

impl ChunkHash {
    /// The hash of a chunking parameter set's version; none for a version not seen.
    pub fn from_version(version: u8) -> Option<Self> {
        match version {
            1 => Some(Self::Sha512),
            2 => Some(Self::Sha256),
            3 => Some(Self::RitoHkdf),
            4 => Some(Self::Blake3),
            _ => None,
        }
    }

    /// The chunking parameter set version this hash belongs to: Riot's number for it.
    pub fn version(self) -> u8 {
        match self {
            Self::Sha512 => 1,
            Self::Sha256 => 2,
            Self::RitoHkdf => 3,
            Self::Blake3 => 4,
        }
    }

    /// The id of a chunk's uncompressed bytes.
    pub fn id_of(self, data: &[u8]) -> u64 {
        match self {
            Self::Sha512 => first_eight(&Sha512::digest(data)),
            Self::Sha256 => first_eight(&Sha256::digest(data)),
            Self::RitoHkdf => {
                // The client keys the HMAC with the digest in a zeroed 64-byte block, which is
                // what HMAC does with a 32-byte key anyway.
                let mut out = [0u8; 8];
                pbkdf2::pbkdf2_hmac::<Sha256>(&Sha256::digest(data), b"", 32, &mut out);
                u64::from_le_bytes(out)
            }
            Self::Blake3 => first_eight(blake3::hash(data).as_bytes()),
        }
    }
}

fn first_eight(bytes: &[u8]) -> u64 {
    u64::from_le_bytes(bytes[..8].try_into().expect("a digest has eight bytes"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // From Python: hashlib.pbkdf2_hmac('sha256', sha256(b'abc').digest(), b'', 32)[:8],
    // blake3.blake3(b'abc').digest(8), sha256(b'abc')[:8], sha512(b'abc')[:8], read little-endian.
    #[test]
    fn known_vectors() {
        assert_eq!(ChunkHash::RitoHkdf.id_of(b"abc"), 0x3d4e_da0a_b8cf_0d24);
        assert_eq!(ChunkHash::Blake3.id_of(b"abc"), 0x3351_4638_acb3_3764);
        assert_eq!(ChunkHash::Sha256.id_of(b"abc"), 0xeacf_018f_bf16_78ba);
        assert_eq!(ChunkHash::Sha512.id_of(b"abc"), 0xba7a_6193_a135_afdd);
        for version in 1..=4 {
            assert_eq!(ChunkHash::from_version(version).map(ChunkHash::version), Some(version));
        }
        assert_eq!(ChunkHash::from_version(0), None);
        assert_eq!(ChunkHash::from_version(5), None);
    }
}
