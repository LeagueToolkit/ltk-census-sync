use sync_format::kind_of;

#[test]
fn each_magic_has_its_kind() {
    let cases: &[(&[u8], &str)] = &[
        (b"r3d2anmd\0\0\0\0", "anm"),
        (b"r3d2canm\0\0\0\0", "anm"),
        (b"OEGM\x11\0\0\0", "mapgeo"),
        (b"PreLoad\0", "preload"),
        (b"PROP\x03\0\0\0", "bin"),
        (b"PTCH\x01\0\0\0", "bin"),
        (b"RST\x05\0\0\0\0", "stringtable"),
        (&[0x33, 0x22, 0x11, 0x00, 4, 0, 1, 0], "skn"),
        (b"r3d2sklt\x02\0\0\0", "skl"),
        (b"[ObjectBegin]", "sco"),
        (b"r3d2Mesh\x02\0\0\0", "scb"),
        (b"<svg xmlns", "svg"),
        (b"TEX\0\x10\0\x10\0", "tex"),
        (b"DDS \x7c\0\0\0", "dds"),
        (b"WGEO\x05\0\0\0", "wgeo"),
        (b"BKHD\x18\0\0\0", "bnk"),
        (b"\x1bLuaQ\0\x01\x04", "luaobj"),
        (b"\x89PNG\r\n\x1a\n", "png"),
        (b"r3d2\x01\0\0\0\x02\0\0\0", "wpk"),
        (&[0, 0, 0, 0, 0xC3, 0x4F, 0xFD, 0x22], "skl"),
        (&[0xFF, 0xD8, 0xFF, 0xE0, 0, 0x10], "jpg"),
        (&[0, 0, 2, 0, 0, 0, 0, 0], "tga"),
        (&[3, 0, 0, 0, 0, 0, 0, 0], "lightgrid"),
        (b"RW\x03\x04\0\0\0\0", "wad"),
        (b"MZ\x90\0\x03\0", "exe"),
        (b"RIFF\x24\0\0\0WAVE", "riff"),
        (b"PK\x03\x04\x14\0", "zip"),
        (b"OggS\0\x02", "ogg"),
        (b"plain text", ""),
        (b"", ""),
    ];
    for (bytes, kind) in cases {
        assert_eq!(kind_of(bytes), *kind, "{bytes:02x?}");
    }
}

#[test]
fn three_bytes_get_the_kinds_that_fit_them() {
    assert_eq!(kind_of(b"RST"), "stringtable");
    assert_eq!(kind_of(&[0xFF, 0xD8, 0xFF]), "jpg");
    assert_eq!(kind_of(&[0, 1, 10]), "tga");
    assert_eq!(kind_of(b"RW\x01"), "wad");
    assert_eq!(kind_of(b"abc"), "");
}

#[test]
fn an_inibin_is_not_taken_for_a_tga() {
    // Version 2 with a 512-byte string table: bytes 1 and 2 (0x00, 0x02) fit the tga guess. One
    // string section (flag 12): one key, one u16 offset, then the strings.
    let mut bytes = vec![2, 0x00, 0x02, 0x00, 0x10, 1, 0, 0xAA, 0xBB, 0xCC, 0xDD, 0, 0];
    bytes.extend([b'x'; 512]);
    assert_eq!(kind_of(&bytes), "inibin");
    bytes.pop();
    assert_eq!(kind_of(&bytes), "tga");
}
