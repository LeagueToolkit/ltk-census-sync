//! An animation clip's facts (`docs/FORMAT.md`, "clip"): the joints it animates, as hashes, in
//! stored order.
//!
//! The layouts are ltk_anim's. Its uncompressed reader keeps the joints in a map, which loses
//! their order, so the joint list is read here from the same offsets, and nothing else of the clip
//! is: a clip whose frames do not parse still lists its joints.
//!
//! - `r3d2canm` (compressed, versions 1 to 3): the joint count at 24 and the offset of the hashes
//!   at 124.
//! - `r3d2anmd` 5: the hashes are a section of their own, from its offset at 40 to the frames'
//!   offset at 60.
//! - `r3d2anmd` 4: each frame is one 12-byte record a track, starting with the joint's hash; the
//!   first frame names the joints.
//! - `r3d2anmd` 3: each track starts with the joint's name in 32 bytes, hashed with ELF as the game
//!   hashes a joint's name.
//!
//! Every offset a header gives is from byte 12.

use crate::read::u32_at;
use crate::yaml::ClipFacts;
use crate::Error;

/// What a header's offsets are from.
const BASE: usize = 12;

/// A clip's facts from its bytes.
pub fn clip_facts(bytes: &[u8]) -> Result<ClipFacts, Error> {
    let fail = |message: String| Error::Clip(message);
    let field = |at: usize| u32_at(bytes, at).map(|v| v as usize).ok_or_else(|| fail(format!("truncated at {at}")));
    let hashes = |at: usize, count: usize| -> Result<Vec<u32>, Error> {
        let end = count.checked_mul(4).and_then(|len| at.checked_add(len));
        let section = end.and_then(|end| bytes.get(at..end)).ok_or_else(|| fail(format!("{count} joints at {at} run past the end")))?;
        Ok(section.as_chunks::<4>().0.iter().map(|h| u32::from_le_bytes(*h)).collect())
    };
    let version = field(8)?;
    let joints = match (bytes.get(..8), version) {
        (Some(b"r3d2canm"), 1..=3) => hashes(field(124)? + BASE, field(24)?)?,
        (Some(b"r3d2anmd"), 5) => {
            let (tracks, start, end) = (field(28)?, field(40)?, field(60)?);
            let count = end.checked_sub(start).ok_or_else(|| fail(format!("joints at {start}, frames at {end}")))? / 4;
            if count > tracks {
                return Err(fail(format!("{count} joints for {tracks} tracks")));
            }
            hashes(start + BASE, count)?
        }
        (Some(b"r3d2anmd"), 4) => {
            let (tracks, frames, at) = (field(28)?, field(32)?, field(60)? + BASE);
            let mut joints = Vec::new();
            for track in 0..if frames == 0 { 0 } else { tracks } {
                joints.push(u32_at(bytes, at + 12 * track).ok_or_else(|| fail(format!("track {track} runs past the end")))?);
            }
            joints
        }
        (Some(b"r3d2anmd"), 3) => {
            let (tracks, frames) = (field(16)?, field(20)?);
            // A name, the track's flags, then a rotation and a translation a frame.
            let track_len = frames.checked_mul(28).and_then(|len| len.checked_add(36));
            let mut joints = Vec::new();
            for track in 0..tracks {
                let at = track_len.and_then(|len| len.checked_mul(track)).and_then(|at| at.checked_add(28));
                let name = at.and_then(|at| bytes.get(at..at + 32)).ok_or_else(|| fail(format!("track {track} runs past the end")))?;
                let name = name.split(|&b| b == 0).next().expect("split yields one item");
                joints.push(ltk_hash::elf::elf(name) as u32);
            }
            joints
        }
        _ => return Err(fail(format!("magic {:02x?}, version {version}", bytes.get(..8)))),
    };
    Ok(ClipFacts { joints })
}
