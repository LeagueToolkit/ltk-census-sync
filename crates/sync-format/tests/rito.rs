//! The ritobin text of an object with 8-byte hashes (`docs/FORMAT.md`, "A bin's entries"): the
//! width is not in the bytes, so the reader resolves it from the declared sizes, and the text
//! spells it as ltk_ritobin does, so the checks before a push parse it back.

use ltk_ritobin::Cst;
use pretty_assertions::assert_eq;
use sync_format::render_object;

const HASH: u8 = 17;
const U32: u8 = 7;
const LIST: u8 = 0x80;
const OPTION: u8 = 0x85;
const MAP: u8 = 0x86;

/// An object's bytes from its entry hash: the properties given as (name, kind, body).
fn object(entry: u32, props: &[(u32, u8, Vec<u8>)]) -> Vec<u8> {
    let mut out = entry.to_le_bytes().to_vec();
    out.extend((props.len() as u16).to_le_bytes());
    for (name, kind, body) in props {
        out.extend(name.to_le_bytes());
        out.push(*kind);
        out.extend(body);
    }
    out
}

/// A sized container body: the item kinds, then the size and the count, then `items`.
fn sized(kinds: &[u8], count: u32, items: &[u8]) -> Vec<u8> {
    let mut out = kinds.to_vec();
    out.extend((4 + items.len() as u32).to_le_bytes());
    out.extend(count.to_le_bytes());
    out.extend(items);
    out
}

#[test]
fn an_eight_byte_hash_is_typed_hash_u64_wherever_it_stands() {
    let mut pair = 2u64.to_le_bytes().to_vec();
    pair.extend(7u32.to_le_bytes());
    let mut some = vec![HASH, 1];
    some.extend(3u64.to_le_bytes());
    let bytes = object(
        0xabcd,
        &[
            (1, HASH, 0x1234_5678u32.to_le_bytes().to_vec()),
            (2, HASH, 0x5d07_ca0d_22ff_9588u64.to_le_bytes().to_vec()),
            (3, LIST, sized(&[HASH], 1, &1u64.to_le_bytes())),
            (4, LIST, sized(&[HASH], 0, &[])),
            (5, MAP, sized(&[HASH, U32], 1, &pair)),
            (6, OPTION, some),
        ],
    );
    let text = render_object(&bytes, 0x45cd_899f, false).unwrap();
    assert_eq!(
        text,
        "#PROP_text\n\
         type: string = \"PROP\"\n\
         version: u32 = 3\n\
         linked: list[string] = {}\n\
         entries: map[hash,embed] = {\n    \
             0x0000abcd = 0x45cd899f {\n        \
                 0x00000001: hash = 0x12345678\n        \
                 0x00000002: hash[u64] = 0x5d07ca0d22ff9588\n        \
                 0x00000003: list[hash[u64]] = {\n            \
                     0x0000000000000001\n        \
                 }\n        \
                 0x00000004: list[hash] = {}\n        \
                 0x00000005: map[hash[u64],u32] = {\n            \
                     0x0000000000000002 = 7\n        \
                 }\n        \
                 0x00000006: option[hash[u64]] = {\n            \
                     0x0000000000000003\n        \
                 }\n    \
             }\n\
         }\n"
    );

    let cst = Cst::parse(&text);
    assert!(cst.errors.is_empty(), "{:#?}", cst.errors);
    let partial = cst.build_bin(&text);
    assert!(partial.diagnostics.is_empty(), "{:#?}", partial.diagnostics);
}
