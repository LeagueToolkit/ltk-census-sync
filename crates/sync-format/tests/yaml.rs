//! The renderers against texts of the published history, from facts the tests give.

use pretty_assertions::assert_eq;
use sync_format::{
    build_yaml, commit_message, commit_time, entry_yaml, wad_yaml, BuildFacts, EntryFacts, MeshFacts, Rads, Section,
    SubmeshFacts, TextureFacts,
};

fn tip() -> BuildFacts {
    let realms = "TW2 VN2 BR1 EUN1 EUW1 JP1 KR LA1 LA2 ME1 NA1 OC1 RU SG2 TR1 KR";
    BuildFacts {
        version: "16.19.8207193".into(),
        patch: "16.19".into(),
        manifest: 0xad68_96ce_5cf4_75a1,
        rads: None,
        date: "2026-09-21".into(),
        legacy_bins: false,
        realms: realms.split(' ').map(str::to_string).collect(),
    }
}

#[test]
fn an_rman_build_has_its_yaml_and_message() {
    let realms: String = "BR1 EUN1 EUW1 JP1 KR LA1 LA2 ME1 NA1 OC1 RU SG2 TR1 TW2 VN2".split(' ').map(|r| format!(" - \"{r}\"\n")).collect();
    assert_eq!(
        build_yaml(&tip()),
        format!(
            "version: \"16.19.8207193\"\npatch: \"16.19\"\nmanifest: \"ad6896ce5cf475a1\"\nsource: \"rman\"\n\
             date: \"2026-09-21\"\nlegacyBins: false\nrealms:\n{realms}"
        )
    );
    assert_eq!(
        commit_message(&tip()),
        "16.19.8207193\n\nCensus-Patch: 16.19\nCensus-Manifest: ad6896ce5cf475a1\nCensus-Source: rman\n\
         Census-Realms: BR1 EUN1 EUW1 JP1 KR LA1 LA2 ME1 NA1 OC1 RU SG2 TR1 TW2 VN2\n"
    );
}

#[test]
fn a_rads_build_has_its_yaml_and_message() {
    let build = BuildFacts {
        version: "8.20.2483196".into(),
        patch: "8.20".into(),
        manifest: 0xe39c_3864_7162_cf28,
        rads: Some(Rads { solution: "0.0.1.243".into(), release: "0.0.1.184".into(), exe: "8.20.248.3196".into() }),
        date: "2018-10-03".into(),
        legacy_bins: true,
        realms: vec!["rads:live".into()],
    };
    assert_eq!(
        build_yaml(&build),
        "version: \"8.20.2483196\"\npatch: \"8.20\"\nmanifest: \"e39c38647162cf28\"\nsource: \"rads\"\nsolution: \"0.0.1.243\"\n\
         release: \"0.0.1.184\"\nexe: \"8.20.248.3196\"\ndate: \"2018-10-03\"\nlegacyBins: true\nrealms:\n - \"rads:live\"\n"
    );
    assert_eq!(
        commit_message(&build),
        "8.20.2483196\n\nCensus-Patch: 8.20\nCensus-Manifest: e39c38647162cf28\nCensus-Source: rads\nCensus-Realms: rads:live\n"
    );
}

#[test]
fn a_commit_is_dated_midnight_utc_of_its_build() {
    assert_eq!(commit_time("2018-10-03"), Some(1_538_524_800));
    assert_eq!(commit_time("2000-02-29"), Some(951_782_400));
    assert_eq!(commit_time("1970-01-01"), Some(0));
    for bad in ["2018-13-01", "2018-10-3", "20181003", "2018/10/03", ""] {
        assert_eq!(commit_time(bad), None, "{bad}");
    }
}

#[test]
fn a_wad_lists_its_tags_sorted_without_none() {
    assert_eq!(
        wad_yaml(3, 4, 0x27f7_f49f_f6d4_1bc4, ["en_GB", "none", "en_AU", "en_GB"]),
        "version: \"3.4\"\nfileId: \"27f7f49ff6d41bc4\"\ntags:\n - \"en_AU\"\n - \"en_GB\"\n"
    );
    assert_eq!(wad_yaml(3, 4, 1, ["none"]), "version: \"3.4\"\nfileId: \"0000000000000001\"\ntags: []\n");
}

fn entry(kind: &str, section: Section) -> EntryFacts {
    EntryFacts { sha256: [0xab; 32], checksum: None, kind: kind.into(), links: None, section: Some(section) }
}

#[test]
fn a_mesh_lists_its_submeshes_in_order() {
    let mesh = MeshFacts {
        vertex_type: "Basic".into(),
        vertices: 48,
        indices: 180,
        submeshes: vec![SubmeshFacts { name: "lambert1".into(), vertices: 48, indices: 180, positions: 0x6621_6ca2_3130_bbcd, rounded: 0x7217_5e93_b32a_1d95 }],
    };
    assert_eq!(
        entry_yaml(&entry("skn", Section::Mesh(mesh))),
        format!(
            "sha256: \"{}\"\nkind: \"skn\"\nmesh:\n vertexType: \"Basic\"\n vertices: 48\n indices: 180\n submeshes:\n  - name: \"lambert1\"\n    \
             vertices: 48\n    indices: 180\n    positions: \"66216ca23130bbcd\"\n    rounded: \"72175e93b32a1d95\"\n",
            "ab".repeat(32)
        )
    );
    let empty = MeshFacts { vertex_type: String::new(), vertices: 0, indices: 0, submeshes: Vec::new() };
    assert!(entry_yaml(&entry("skn", Section::Mesh(empty))).ends_with("mesh:\n vertexType: \"\"\n vertices: 0\n indices: 0\n submeshes: []\n"));
}

#[test]
fn a_texture_has_its_format_size_mips_and_top() {
    let texture = TextureFacts { format: "bc3".into(), width: 512, height: 256, mips: 10, top: 0x0123_4567_89ab_cdef };
    assert!(entry_yaml(&entry("tex", Section::Texture(texture)))
        .ends_with("kind: \"tex\"\ntexture:\n format: \"bc3\"\n width: 512\n height: 256\n mips: 10\n top: \"0123456789abcdef\"\n"));
}
