//! An entry's files from bytes the tests build, for each kind with a section and each edge case
//! of `docs/FORMAT.md`.

use pretty_assertions::assert_eq;
use sha2::{Digest, Sha256};
use sync_format::{bank_facts, entry_files, skeleton_facts, BankFacts, EntryFiles, Joint, MediaFacts, KIND_LINK};

const PATH_HASH: u64 = 0x22e2_cf78_5bae_ac7e;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn sha(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn files(entry: &EntryFiles) -> Vec<(&str, &str)> {
    entry.files.iter().map(|(p, t)| (p.as_str(), t.as_str())).collect()
}

/// A `PROP` bin: version 3, its links, then (entry hash, class, properties) per object.
fn prop_bin(links: &[&str], objects: &[(u32, u32, Vec<u8>)]) -> Vec<u8> {
    let mut out = b"PROP".to_vec();
    out.extend(3u32.to_le_bytes());
    out.extend((links.len() as u32).to_le_bytes());
    for link in links {
        out.extend((link.len() as u16).to_le_bytes());
        out.extend(link.as_bytes());
    }
    out.extend((objects.len() as u32).to_le_bytes());
    for (_, class, _) in objects {
        out.extend(class.to_le_bytes());
    }
    for (entry, _, props) in objects {
        out.extend(((4 + props.len()) as u32).to_le_bytes());
        out.extend(entry.to_le_bytes());
        out.extend(props);
    }
    out
}

/// An object's properties: one string property.
fn string_property(name: u32, value: &str) -> Vec<u8> {
    let mut out = 1u16.to_le_bytes().to_vec();
    out.extend(name.to_le_bytes());
    out.push(16); // string
    out.extend((value.len() as u16).to_le_bytes());
    out.extend(value.as_bytes());
    out
}

#[test]
fn a_bin_is_its_links_its_keys_and_one_text_per_entry() {
    let nami = string_property(0xecf1_c6bc, "Nami_Base_Q_cas");
    let broken = vec![1, 0, 0xAA, 0xBB, 0xCC, 0xDD, 0xFE];
    let bytes = prop_bin(
        &["b/c.bin", "a/b.bin", "b/c.bin"],
        &[
            (0xb086_73e8, 0x45cd_899f, nami.clone()),
            (0x0000_0001, 0x1234_5678, broken.clone()),
            // A repeated entry hash: the first object stays.
            (0xb086_73e8, 0x45cd_899f, string_property(0xecf1_c6bc, "Nami_Base_W_cas")),
        ],
    );
    let entry = entry_files(PATH_HASH, Some(0xf77d_e45e_4dc9_43f9), "bin", &bytes, false);
    assert_eq!((entry.unrenderable, entry.repeated, entry.error.is_none()), (1, 1, true));

    let object = |entry: u32, props: &[u8]| [entry.to_le_bytes().as_slice(), props].concat();
    let (first, second) = (object(0xb086_73e8, &nami), object(1, &broken));
    let yaml = format!(
        "sha256: \"{}\"\nchecksum: \"f77de45e4dc943f9\"\nkind: \"bin\"\nlinks:\n - \"a/b.bin\"\n - \"b/c.bin\"\nobjects:\n \
         \"00000001\":\n  class: \"12345678\"\n  content: \"{}\"\n  norm: \"{}\"\n \
         \"b08673e8\":\n  class: \"45cd899f\"\n  content: \"{}\"\n  norm: \"{}\"\n",
        sha(&bytes),
        sha(&second),
        &sha(&second[4..])[..16],
        sha(&first),
        &sha(&first[4..])[..16],
    );
    let rito = "#PROP_text\ntype: string = \"PROP\"\nversion: u32 = 3\nlinked: list[string] = {}\n\
                entries: map[hash,embed] = {\n    0xb08673e8 = 0x45cd899f {\n        \
                0xecf1c6bc: string = \"Nami_Base_Q_cas\"\n    }\n}\n";
    assert_eq!(
        files(&entry),
        [("22/22e2cf785baeac7e.bin/b0/b08673e8.rito", rito), ("22/22e2cf785baeac7e.yaml", yaml.as_str())]
    );
}

#[test]
fn a_bin_with_no_objects_has_links_and_no_objects_key() {
    let bytes = prop_bin(&[], &[]);
    let entry = entry_files(PATH_HASH, Some(1), "bin", &bytes, false);
    let yaml = format!("sha256: \"{}\"\nchecksum: \"0000000000000001\"\nkind: \"bin\"\nlinks: []\n", sha(&bytes));
    assert_eq!(files(&entry), [("22/22e2cf785baeac7e.yaml", yaml.as_str())]);
}

#[test]
fn a_ptch_bin_does_not_split() {
    let bytes = b"PTCH\x01\0\0\0\x8b\0\0\0";
    let entry = entry_files(PATH_HASH, Some(2), "bin", bytes, false);
    assert!(entry.error.is_none());
    let yaml = format!("sha256: \"{}\"\nchecksum: \"0000000000000002\"\nkind: \"bin\"\n", sha(bytes));
    assert_eq!(files(&entry), [("22/22e2cf785baeac7e.yaml", yaml.as_str())]);
}

#[test]
fn a_link_an_unknown_kind_and_bytes_that_do_not_parse_have_no_section() {
    let target = b"\x0e\0\0\0data/maps/x.bin";
    let link = entry_files(PATH_HASH, Some(3), KIND_LINK, target, false);
    let yaml = format!("sha256: \"{}\"\nchecksum: \"0000000000000003\"\nkind: \"link\"\n", sha(target));
    assert_eq!(files(&link), [("22/22e2cf785baeac7e.yaml", yaml.as_str())]);

    let unknown = entry_files(PATH_HASH, None, "", b"text", false);
    assert_eq!(files(&unknown), [("22/22e2cf785baeac7e.yaml", format!("sha256: \"{}\"\n", sha(b"text")).as_str())]);

    let truncated = entry_files(PATH_HASH, None, "skn", &[0x33, 0x22, 0x11, 0x00], false);
    assert!(truncated.error.is_some());
    let yaml = format!("sha256: \"{}\"\nkind: \"skn\"\n", sha(&[0x33, 0x22, 0x11, 0x00]));
    assert_eq!(files(&truncated), [("22/22e2cf785baeac7e.yaml", yaml.as_str())]);
}

fn section(tag: &[u8; 4], body: &[u8]) -> Vec<u8> {
    [tag.as_slice(), &(body.len() as u32).to_le_bytes(), body].concat()
}

fn u32s(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

/// A HIRC object: its type, its size, its body.
fn hirc_object(kind: u8, body: &[u8]) -> Vec<u8> {
    [&[kind], (body.len() as u32).to_le_bytes().as_slice(), body].concat()
}

/// A version 145 bank: two events, one playing a random container that holds one sound, one
/// playing nothing; two wems, one listed twice.
fn events_and_media() -> Vec<u8> {
    // Sound 10: vorbis, streamed from wem 500, parent 20, then fields the facts skip.
    let mut sound = u32s(&[10, 0x0004_0001]);
    sound.push(2);
    sound.extend(u32s(&[500, 0]));
    sound.extend([0x80, 0, 0, 0, 0, 0]); // source bits; no fx, no metadata fx, attachment
    sound.extend(u32s(&[0xBEEF, 20]));
    sound.extend([7, 7, 7]);
    // Random container 20 with one effect, no parent.
    let mut random = u32s(&[20]);
    random.extend([1, 1, 0]);
    random.extend([0; 7]);
    random.extend([0, 0, 0]);
    random.extend(u32s(&[0xBEEF, 0]));
    // Action 30 plays 20; action 31 stops it.
    let play = [u32s(&[30]), 0x0403u16.to_le_bytes().to_vec(), u32s(&[20]), vec![0]].concat();
    let stop = [u32s(&[31]), 0x0103u16.to_le_bytes().to_vec(), u32s(&[20]), vec![0]].concat();
    // Event 40 runs both actions; event 41 only the stop.
    let event = [u32s(&[40]), vec![2], u32s(&[30, 31])].concat();
    let quiet = [u32s(&[41]), vec![1], u32s(&[31])].concat();

    let mut hirc = u32s(&[6]);
    for (kind, body) in [(2, sound), (5, random), (3, play), (3, stop), (4, event), (4, quiet)] {
        hirc.extend(hirc_object(kind, &body));
    }
    let wem = b"RIFF....WAVE";
    let mut bank = section(b"BKHD", &u32s(&[145, 0x0434_2ac6, 0, 0, 0]));
    bank.extend(section(b"DIDX", &u32s(&[600, 4, 8, 500, 0, 12, 600, 0, 4])));
    bank.extend(section(b"DATA", wem));
    bank.extend(section(b"HIRC", &hirc));
    bank
}

#[test]
fn a_bank_lists_its_wems_and_what_each_event_plays() {
    let bytes = events_and_media();
    let wem = |bytes: &[u8]| -> [u8; 8] { Sha256::digest(bytes)[..8].try_into().unwrap() };
    assert_eq!(
        bank_facts(&bytes).unwrap(),
        BankFacts {
            version: 145,
            bank_id: Some(0x0434_2ac6),
            // Wem 600's first record wins.
            media: vec![
                MediaFacts { id: 600, size: 8, hash: wem(b"....WAVE") },
                MediaFacts { id: 500, size: 12, hash: wem(b"RIFF....WAVE") },
            ],
            events: vec![(40, vec![500]), (41, vec![])],
        }
    );
    let entry = entry_files(PATH_HASH, None, "bnk", &bytes, false);
    let yaml = format!(
        "sha256: \"{}\"\nkind: \"bnk\"\nbank:\n version: 145\n bankId: \"04342ac6\"\n media:\n  \"000001f4\":\n   size: 12\n   \
         hash: \"{}\"\n\n  \"00000258\":\n   size: 8\n   hash: \"{}\"\n\n events:\n  \"00000028\":\n   - \"000001f4\"\n\n  \
         \"00000029\": []\n\n",
        sha(&bytes),
        hex(&wem(b"RIFF....WAVE")),
        hex(&wem(b"....WAVE")),
    );
    assert_eq!(files(&entry), [("22/22e2cf785baeac7e.yaml", yaml.as_str())]);
}

#[test]
fn a_wpk_lists_its_wems_by_the_ids_in_their_names() {
    let name: Vec<u8> = "500.wem".encode_utf16().flat_map(|c| c.to_le_bytes()).collect();
    let wem = b"RIFF";
    let mut bytes = b"r3d2".to_vec();
    bytes.extend(u32s(&[1, 2]));
    let entry_at = 4 + 4 + 4 + 2 * 4;
    let data_at = entry_at + 12 + name.len();
    bytes.extend(u32s(&[0, entry_at as u32])); // a dead slot, then the entry
    bytes.extend(u32s(&[data_at as u32, wem.len() as u32, 7]));
    bytes.extend(&name);
    bytes.extend(wem);
    let facts = bank_facts(&bytes).unwrap();
    assert_eq!((facts.version, facts.bank_id, facts.media.len(), facts.media[0].id), (1, None, 1, 500));
    assert!(facts.events.is_empty());
}

/// An `r3d2sklt` skeleton, version 2: (name, parent) per joint, then the influences.
fn legacy_skeleton(joints: &[(&str, i32)], influences: &[u32]) -> Vec<u8> {
    let mut bytes = b"r3d2sklt".to_vec();
    bytes.extend(u32s(&[2, 0x0016_600f, joints.len() as u32]));
    for (name, parent) in joints {
        let mut padded = [0u8; 32];
        padded[..name.len()].copy_from_slice(name.as_bytes());
        bytes.extend(padded);
        bytes.extend(parent.to_le_bytes());
        bytes.extend([0u8; 4 + 48]);
    }
    bytes.extend(u32s(&[influences.len() as u32]));
    bytes.extend(u32s(influences));
    bytes
}

#[test]
fn a_joint_names_its_parent_unless_another_joint_shares_the_name() {
    let bytes = legacy_skeleton(&[("root", -1), ("spine", 0), ("arm", 1), ("arm", 1), ("hand", 3)], &[0, 1]);
    let facts = skeleton_facts(&bytes).unwrap();
    assert_eq!(facts.joints[0], Joint { name: "root".into(), parent: None });
    assert_eq!((facts.influences, facts.name.as_str(), facts.asset.as_str()), (2, "", ""));
    let entry = entry_files(PATH_HASH, None, "skl", &bytes, false);
    let yaml = format!(
        "sha256: \"{}\"\nkind: \"skl\"\nskeleton:\n influences: 2\n joints:\n  - name: \"root\"\n  - name: \"spine\"\n    \
         parent: \"root\"\n  - name: \"arm\"\n    parent: \"spine\"\n  - name: \"arm\"\n    parent: \"spine\"\n  - name: \"hand\"\n    \
         parent: 3\n",
        sha(&bytes)
    );
    assert_eq!(files(&entry), [("22/22e2cf785baeac7e.yaml", yaml.as_str())]);
}
