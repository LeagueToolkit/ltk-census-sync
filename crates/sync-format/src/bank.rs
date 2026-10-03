//! A sound bank's facts (`docs/FORMAT.md`, "bank"): its wems, and the wems each event's play actions
//! reach through the bank's hierarchy.
//!
//! League splits audio into a media bank (`*_audio.bnk` with `DIDX` and `DATA`, or a `.wpk`)
//! holding the `.wem` files, and an events bank (`*_events.bnk`, `HIRC`) holding the flat object
//! hierarchy: every object has a type, an id and a size; a sound names its wem, an action its
//! target, a node its parent, an event its actions.
//!
//! The layouts follow bnk-extract and wwiser for the bank versions League ships: 132 (season 10
//! to 11.6), 134 (11.7 to 15.12) and 145 (15.13 on). Only the fields the facts need are read; an
//! object whose fields run past its bytes keeps what was read.

use std::collections::{HashMap, HashSet};

use sha2::{Digest, Sha256};

use crate::read::u32_at;
use crate::yaml::{BankFacts, MediaFacts};
use crate::Error;

// HIRC object types.
const SOUND: u8 = 2;
const ACTION: u8 = 3;
const EVENT: u8 = 4;
const RANDOM: u8 = 5;
const SWITCH: u8 = 6;
const ACTOR_MIXER: u8 = 7;
const LAYER: u8 = 9;
const MUSIC_SEGMENT: u8 = 10;
const MUSIC_SWITCH: u8 = 12;
const MUSIC_PLAYLIST: u8 = 13;

/// The action type that plays its target.
const ACTION_PLAY: u32 = 4;

/// One HIRC object, as far as the facts need it.
#[derive(Debug, Clone, Default)]
struct Object {
    id: u32,
    kind: u8,
    /// A node's direct parent (sounds and containers); none when it has none or is not a node.
    parent: Option<u32>,
    /// A sound's source: the wem id.
    media: Option<u32>,
    /// An action's target object.
    target: Option<u32>,
    /// A sound's stream type; an action's type.
    flag: Option<u32>,
    /// An event's actions, in order.
    actions: Vec<u32>,
}

/// A bank as stored, objects and media in stored order.
#[derive(Debug, Default)]
struct Bank {
    version: u32,
    bank_id: Option<u32>,
    objects: Vec<Object>,
    media: Vec<MediaFacts>,
}

/// A bank's facts from its bytes. An object id or wem id the bank repeats keeps its first record,
/// except that a later object with the same id adds the actions past the first one's count.
pub fn bank_facts(bytes: &[u8]) -> Result<BankFacts, Error> {
    let bank = parse_bank(bytes)?;
    let mut objects: Vec<Object> = Vec::new();
    let mut at: HashMap<u32, usize> = HashMap::new();
    for o in bank.objects {
        match at.get(&o.id) {
            None => {
                at.insert(o.id, objects.len());
                objects.push(o);
            }
            Some(&i) => {
                let first = &mut objects[i];
                if o.actions.len() > first.actions.len() {
                    first.actions.extend_from_slice(&o.actions[first.actions.len()..]);
                }
            }
        }
    }
    let mut seen = HashSet::new();
    let media = bank.media.into_iter().filter(|m| seen.insert(m.id)).collect();
    Ok(BankFacts {
        version: u64::from(bank.version),
        bank_id: bank.bank_id,
        media,
        events: events(&Hierarchy::new(objects)),
    })
}

/// Each event's id and the wem ids its play actions reach, through the sounds under their targets.
fn events(hierarchy: &Hierarchy) -> Vec<(u32, Vec<u32>)> {
    let mut events: Vec<(u32, Vec<u32>)> = hierarchy
        .events()
        .map(|event| {
            let mut wems: Vec<u32> = event
                .actions
                .iter()
                .filter_map(|&a| hierarchy.get(a))
                .filter(|a| a.flag == Some(ACTION_PLAY))
                .filter_map(|a| a.target)
                .flat_map(|t| hierarchy.sounds_under(t))
                .filter_map(|s| s.media)
                .collect();
            wems.sort_unstable();
            wems.dedup();
            (event.id, wems)
        })
        .collect();
    events.sort_unstable();
    events
}

/// A bank's objects with their parents resolved.
struct Hierarchy {
    objects: Vec<Object>,
    index: HashMap<u32, usize>,
    children: HashMap<u32, Vec<u32>>,
}

impl Hierarchy {
    fn new(objects: Vec<Object>) -> Self {
        let mut index = HashMap::with_capacity(objects.len());
        let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
        for (i, o) in objects.iter().enumerate() {
            index.entry(o.id).or_insert(i);
            if let Some(parent) = o.parent {
                children.entry(parent).or_default().push(o.id);
            }
        }
        Self { objects, index, children }
    }

    fn get(&self, id: u32) -> Option<&Object> {
        self.index.get(&id).map(|&i| &self.objects[i])
    }

    fn events(&self) -> impl Iterator<Item = &Object> {
        self.objects.iter().filter(|o| o.kind == EVENT)
    }

    /// The sounds under a node: itself when it is a sound, and every sound in its subtree.
    fn sounds_under(&self, id: u32) -> Vec<&Object> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        let mut stack = vec![id];
        while let Some(id) = stack.pop() {
            if !seen.insert(id) {
                continue;
            }
            if let Some(o) = self.get(id).filter(|o| o.kind == SOUND) {
                out.push(o);
            }
            if let Some(kids) = self.children.get(&id) {
                stack.extend(kids.iter().copied());
            }
        }
        out
    }
}

struct Cur<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Cur<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let slice = self.bytes.get(self.pos..self.pos.checked_add(n)?)?;
        self.pos += n;
        Some(slice)
    }

    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }

    fn u16(&mut self) -> Option<u16> {
        self.take(2).map(|b| u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Option<u32> {
        self.take(4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn skip(&mut self, n: usize) -> Option<()> {
        self.take(n).map(|_| ())
    }

    /// Wwise's variable-size count: seven bits a byte, high bits first, the top bit continuing.
    fn var(&mut self) -> Option<u32> {
        let mut value: u32 = 0;
        for _ in 0..5 {
            let byte = self.u8()?;
            value = (value << 7) | u32::from(byte & 0x7F);
            if byte & 0x80 == 0 {
                return Some(value);
            }
        }
        None
    }
}

/// A `.bnk` (`BKHD`) or a `.wpk` (`r3d2`).
fn parse_bank(bytes: &[u8]) -> Result<Bank, Error> {
    match bytes.get(..4) {
        Some(b"BKHD") => parse_bnk(bytes),
        Some(b"r3d2") => parse_wpk(bytes),
        _ => Err(Error::Bank("not a bnk or wpk".to_string())),
    }
}

/// A run of sections, each a four-byte tag and a u32 length: `BKHD` (u32 version, u32 bank id,
/// ...), `DIDX` (12 bytes per wem: id, offset, size), `DATA` (the wems), `HIRC`.
fn parse_bnk(bytes: &[u8]) -> Result<Bank, Error> {
    let mut bank = Bank::default();
    let mut didx: Vec<(u32, u32, u32)> = Vec::new();
    let mut data: Option<&[u8]> = None;
    let mut hirc: Option<&[u8]> = None;
    let mut header = false;
    let mut pos = 0;
    while let (Some(tag), Some(len)) = (bytes.get(pos..pos + 4), u32_at(bytes, pos + 4)) {
        let body = bytes
            .get(pos + 8..pos + 8 + len as usize)
            .ok_or_else(|| Error::Bank(format!("section {} runs past the end", String::from_utf8_lossy(tag))))?;
        match tag {
            b"BKHD" => {
                bank.version = u32_at(body, 0).ok_or_else(|| Error::Bank("BKHD too short for a version".to_string()))?;
                bank.bank_id = u32_at(body, 4);
                header = true;
            }
            b"DIDX" => {
                for entry in body.as_chunks::<12>().0 {
                    let field = |at| u32_at(entry, at).expect("a DIDX record of twelve bytes");
                    didx.push((field(0), field(4), field(8)));
                }
            }
            b"DATA" => data = Some(body),
            b"HIRC" => hirc = Some(body),
            _ => {}
        }
        pos += 8 + len as usize;
    }
    if !header {
        return Err(Error::Bank("no BKHD section".to_string()));
    }
    for (id, offset, size) in didx {
        let wem = data.and_then(|d| d.get(offset as usize..(offset as usize).checked_add(size as usize)?));
        if let Some(wem) = wem {
            bank.media.push(media(id, size, wem));
        }
    }
    if let Some(section) = hirc {
        parse_hirc(section, &mut bank)?;
    }
    Ok(bank)
}

fn media(id: u32, size: u32, wem: &[u8]) -> MediaFacts {
    let digest = Sha256::digest(wem);
    MediaFacts { id, size: u64::from(size), hash: digest[..8].try_into().expect("a SHA-256 has eight bytes") }
}

fn parse_hirc(section: &[u8], bank: &mut Bank) -> Result<(), Error> {
    let mut cur = Cur::new(section);
    let count = cur.u32().ok_or_else(|| Error::Bank("HIRC truncated at the count".to_string()))?;
    for i in 0..count {
        let truncated = || Error::Bank(format!("HIRC truncated at object {i}"));
        let kind = cur.u8().ok_or_else(truncated)?;
        let size = cur.u32().ok_or_else(truncated)?;
        let body = cur.take(size as usize).ok_or_else(|| Error::Bank(format!("HIRC object {i} runs past the section")))?;
        bank.objects.push(read_object(kind, body, bank.version));
    }
    Ok(())
}

/// One object's fields; those past its bytes stay unset.
fn read_object(kind: u8, body: &[u8], version: u32) -> Object {
    let mut cur = Cur::new(body);
    let mut object = Object { kind, ..Default::default() };
    let mut read = || -> Option<()> {
        object.id = cur.u32()?;
        match kind {
            SOUND => {
                let plugin = cur.u32()?;
                object.flag = Some(u32::from(cur.u8()?));
                object.media = Some(cur.u32()?);
                cur.u32()?; // in-memory media size
                cur.u8()?; // source bits
                if plugin & 0xF == 2 {
                    // a source plugin (silence, tone): its parameters
                    let len = cur.u32()?;
                    cur.skip(len as usize)?;
                }
                object.parent = node_parent(&mut cur, version)?;
            }
            ACTION => {
                let word = cur.u16()?;
                object.flag = Some(u32::from(word >> 8));
                object.target = Some(cur.u32()?);
            }
            EVENT => {
                let count = if version <= 122 { cur.u32()? } else { cur.var()? };
                for _ in 0..count {
                    object.actions.push(cur.u32()?);
                }
            }
            RANDOM | SWITCH | ACTOR_MIXER | LAYER => object.parent = node_parent(&mut cur, version)?,
            MUSIC_SEGMENT | MUSIC_SWITCH | MUSIC_PLAYLIST => {
                if version > 89 {
                    cur.u8()?; // midi flags
                }
                object.parent = node_parent(&mut cur, version)?;
            }
            _ => {}
        }
        Some(())
    };
    let _ = read();
    object
}

/// The direct parent at the head of a node's base parameters: after the initial effects (and,
/// from version 137, the metadata effects) and the override-bus id. Zero is no parent.
fn node_parent(cur: &mut Cur, version: u32) -> Option<Option<u32>> {
    cur.u8()?; // override parent fx
    let effects = cur.u8()? as usize;
    if effects > 0 {
        cur.u8()?; // bypass bits
        cur.skip(effects * if version <= 145 { 7 } else { 6 })?;
    }
    if version > 136 {
        cur.u8()?; // override parent metadata
        let effects = cur.u8()? as usize;
        cur.skip(effects * 6)?;
    }
    if version > 89 && version <= 145 {
        cur.u8()?; // override attachment params
    }
    cur.u32()?; // override bus id
    let parent = cur.u32()?;
    Some((parent != 0).then_some(parent))
}

/// `r3d2`, u32 version, u32 count, u32 offsets (zero for a dead slot); at each offset u32 data
/// offset, u32 length, u32 name length, the name in UTF-16 (`<id>.wem`).
fn parse_wpk(bytes: &[u8]) -> Result<Bank, Error> {
    let mut cur = Cur::new(bytes);
    cur.skip(4);
    let version = cur.u32().ok_or_else(|| Error::Bank("wpk truncated at the version".to_string()))?;
    let count = cur.u32().ok_or_else(|| Error::Bank("wpk truncated at the count".to_string()))?;
    let mut bank = Bank { version, ..Default::default() };
    for _ in 0..count {
        let offset = cur.u32().ok_or_else(|| Error::Bank("wpk offsets truncated".to_string()))? as usize;
        if offset == 0 {
            continue;
        }
        let Some(entry) = bytes.get(offset..) else { continue };
        let mut e = Cur::new(entry);
        let Some((data_offset, size, name_len)) = (|| Some((e.u32()?, e.u32()?, e.u32()?)))() else { continue };
        let Some(name) = e.take(name_len as usize * 2) else { continue };
        let name: String = char::decode_utf16(name.as_chunks::<2>().0.iter().map(|&c| u16::from_le_bytes(c)))
            .map(|c| c.unwrap_or('\u{FFFD}'))
            .collect();
        let digits: String = name.chars().take_while(char::is_ascii_digit).collect();
        let Ok(id) = digits.parse::<u32>() else { continue };
        if let Some(wem) = bytes.get(data_offset as usize..(data_offset as usize).saturating_add(size as usize)) {
            bank.media.push(media(id, size, wem));
        }
    }
    Ok(bank)
}
