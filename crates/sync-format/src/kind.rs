//! An entry's `kind`: what its bytes are by their magic, as an extension. `ltk_file` knows the
//! game's asset formats; the inibin family, the containers and the executables are told apart
//! here.

use ltk_file::LeagueFileKind;

use crate::read::{u16_at, u32_at};

/// The kind of an entry the WAD's table stores as a link to another file (ltk_wad's `Satellite`):
/// its stored bytes are the path of the file it stands for, not a format with a magic.
pub const KIND_LINK: &str = "link";

/// The kind of an entry by its magic; empty when unknown. Takes the whole entry: an inibin is told
/// by its exact length, so a prefix of the bytes is not enough.
pub fn kind_of(bytes: &[u8]) -> &'static str {
    // First: an inibin's only magic is one byte, but the check is exact, and ltk's tga pattern
    // takes some of them.
    if is_inibin(bytes) {
        return "inibin";
    }
    let ltk = match *bytes {
        [a, b, c] => three_byte_kind([a, b, c]),
        _ => LeagueFileKind::identify_from_bytes(bytes).extension(),
    };
    if let Some(ext) = ltk {
        return ext;
    }
    match bytes {
        [b'R', b'W', major, ..] if *major <= 3 => "wad",
        [b'M', b'Z', ..] => "exe",
        [b'R', b'I', b'F', b'F', ..] => "riff",
        [b'P', b'K', 3, 4, ..] => "zip",
        [b'O', b'g', b'g', b'S', ..] => "ogg",
        _ => "",
    }
}

/// ltk_file's kind of three bytes. Its JPEG pattern declares a minimum of three bytes and reads
/// four, so `identify_from_bytes` panics on three bytes that are not `RST`; these are its patterns
/// that fit three bytes, in its order. No entry of the history has three bytes.
fn three_byte_kind(bytes: [u8; 3]) -> Option<&'static str> {
    match bytes {
        [b'R', b'S', b'T'] => Some("stringtable"),
        [0xFF, 0xD8, 0xFF] => Some("jpg"),
        [_, 0 | 1, 1 | 2 | 3 | 9 | 10 | 11] => Some("tga"),
        _ => None,
    }
}

/// Whether the bytes are a whole inibin of version 1 or 2 (`.troybin` and `.cfgbin` are the same
/// format). Version 2's only magic is the version byte `\x02`, then the string table's length and
/// the flags, so the sections the flags name are walked (each a u16 count, the u32 keys, the
/// values) and the bytes must end exactly where the last one does. Layout after lolpytools'
/// `inibin2.read_2`.
fn is_inibin(bytes: &[u8]) -> bool {
    // Version 1 (until about 2014): `\x01`, three zero bytes, the entry count and the string
    // data's length, then (u32 key, u32 offset) per entry and the strings, to the end.
    if bytes.starts_with(&[1, 0, 0, 0]) {
        let (Some(count), Some(data)) = (u32_at(bytes, 4), u32_at(bytes, 8)) else { return false };
        return count > 0 && 12 + 8 * count as u64 + data as u64 == bytes.len() as u64;
    }
    if bytes.first() != Some(&2) {
        return false;
    }
    let (Some(strings), Some(mut flags)) = (u16_at(bytes, 1), u16_at(bytes, 3)) else {
        return false;
    };
    let mut pos = 5usize;
    // Some carry a zero before the flags.
    if flags == 0 && bytes.len() > pos {
        let Some(f) = u16_at(bytes, pos) else { return false };
        flags = f;
        pos += 2;
    }
    // An empty one: no strings, no sections, five bytes.
    if flags == 0 {
        return strings == 0 && bytes.len() == 5;
    }
    if flags & 0xC000 != 0 {
        return false;
    }
    for bit in 0..14 {
        if flags & (1 << bit) == 0 {
            continue;
        }
        let Some(count) = u16_at(bytes, pos) else { return false };
        let count = count as usize;
        let values = match bit {
            0 | 1 | 10 => 4 * count,
            2 | 4 => count,
            3 | 8 | 12 => 2 * count,
            5 => count.div_ceil(8),
            6 => 3 * count,
            7 => 12 * count,
            9 | 13 => 8 * count,
            11 => 16 * count,
            _ => unreachable!("bit {bit} of a u16 below 14"),
        };
        pos += 2 + 4 * count + values;
        if bit == 12 {
            pos += strings as usize;
        }
        if pos > bytes.len() {
            return false;
        }
    }
    pos == bytes.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tells_an_inibin_by_its_sections() {
        // Two bytes (flag 4) and one string (flag 12), the string table last: `0.0\0`.
        let mut b = vec![2, 4, 0, 0x10, 0x10];
        b.extend([2, 0, 1, 0, 0, 0, 2, 0, 0, 0, 3, 3]);
        b.extend([1, 0, 9, 0, 0, 0, 0, 0]);
        b.extend(b"0.0\0");
        assert!(is_inibin(&b));
        assert_eq!(kind_of(&b), "inibin");
        assert!(!is_inibin(&b[..b.len() - 1]));
        let mut longer = b.clone();
        longer.push(0);
        assert!(!is_inibin(&longer));
        assert!(!is_inibin(&[2, 0, 0, 0, 0x40]));
        assert!(is_inibin(&[2, 0, 0, 0, 0]));
        // Version 1: one key, at offset 0 of four bytes of strings.
        let v1 = [1, 0, 0, 0, 1, 0, 0, 0, 4, 0, 0, 0, 0x91, 0xea, 0x5b, 0x88, 0, 0, 0, 0, b'a', b'b', b'c', 0];
        assert!(is_inibin(&v1));
        assert!(!is_inibin(&v1[..v1.len() - 1]));
        assert!(!is_inibin(&[2, 0, 0, 0, 0, 0]));
        // From 4.20's troybins: one bool; two bools; a bool and a string.
        assert!(is_inibin(&[2, 0, 0, 0x20, 0, 1, 0, 0xf1, 0xae, 0x54, 0x79, 1]));
        assert!(is_inibin(&[2, 0, 0, 0x20, 0, 2, 0, 0xf1, 0xae, 0x54, 0x79, 0x44, 7, 0x9e, 0xea, 3]));
        let mut b = vec![2, 6, 0, 0x20, 0x10, 1, 0, 0xf1, 0xae, 0x54, 0x79, 1, 1, 0, 0x3a, 0x93, 0x16, 6, 0, 0];
        b.extend(b"Sound\0");
        assert!(is_inibin(&b));
    }
}
