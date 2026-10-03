# RMAN manifest schema

The flatbuffer schema of an RMAN manifest's body, as the Riot Client reads it, with its slot
widths confirmed against `LeagueClient.exe` 10.11 and 13.6, `RiotClientFoundation.dll`, the macOS
and Android readers, and fifteen manifests across products and platforms. The files are kept as
they were written, comments included.

`ReleaseManifestBody.fbs` is the root; the others are its includes. The reader in
`src/rman/generated.rs` is generated from it and committed. After a change here, regenerate it
with `flatc` of the same series as the `flatbuffers` crate (25.12 for both), from the crate's
directory:

```
flatc --rust --gen-all -I rman -o src/rman rman/ReleaseManifestBody.fbs
```

and rename the output, `ReleaseManifestBody_generated.rs`, to `generated.rs`.

What the schema fixes against the older community reading (rlib, the C# LeagueToolkit reader):

| table | slot | is |
| --- | --- | --- |
| ChunkingParameter | 0..4 | `ID u16`, `Version u8` (1..4; selects the chunk hash: 1 SHA-512, 2 SHA-256, 3 RITO_HKDF, 4 BLAKE3), `MinChunkSize u32`, `TargetChunkSize u32`, `MaxChunkSize u32` |
| File | 1 | `DirectoryID`, an index into the directory table |
| File | 4 | `TagBitmask`; bit `(tag id - 1)` selects a tag, and every tag seen is a locale |
| File | 10 | `KeyID u16` into the encryption keys |
| File | 11 | `ChunkingParametersID u16` |
| File | 5, 6 | never written and never read by any reader |
| File | 8 | a macOS packaging bit, never read |
| Body | 4 | `EncryptionKeys` (`ID u16`, `Key [u8]`); none in League manifests |
| Body | 6..9 | id lookup tables (chunk, bundle, file, directory), an acceleration structure the loader rebuilds when absent |

The reader uses slots 0 to 3 and 5 of the body. The lookup tables and the keys are declared so
the verifier knows their shape, and otherwise ignored.
