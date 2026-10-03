//! A texture's facts (`docs/FORMAT.md`, "texture"): `tex` through ltk_texture, `dds` through
//! ddsfile, whose format names are the ones the history holds.

use std::io::Cursor;

use ltk_texture::Tex;
use xxhash_rust::xxh3::xxh3_64;

use crate::yaml::TextureFacts;
use crate::Error;

/// The tex header: magic, u16 width, u16 height, u8 depth, u8 format, u8 type, u8 flags.
const TEX_HEADER: usize = 12;

/// A texture's facts from its bytes: format, size, mips, and the XXH3 of the top mip's stored
/// bytes.
pub fn texture_facts(bytes: &[u8]) -> Result<TextureFacts, Error> {
    let fail = |e: &dyn std::fmt::Display| Error::Texture(e.to_string());
    match bytes.get(..4) {
        Some(b"TEX\0") => {
            let tex = Tex::from_reader(&mut Cursor::new(bytes)).map_err(|e| fail(&e))?;
            let (width, height) = (u64::from(tex.width), u64::from(tex.height));
            let (bw, bh) = tex.format.block_size();
            let top_len = (width as usize).div_ceil(bw)
                * (height as usize).div_ceil(bh)
                * tex.format.bytes_per_block()
                * tex.depth.max(1) as usize;
            let payload = bytes.get(TEX_HEADER..).unwrap_or_default();
            // Mips are stored smallest first, so the top one ends the payload.
            let top = payload.get(payload.len().saturating_sub(top_len)..).unwrap_or(payload);
            Ok(TextureFacts {
                format: format!("{:?}", tex.format).to_ascii_lowercase(),
                width,
                height,
                mips: u64::from(tex.mip_count),
                top: xxh3_64(top),
            })
        }
        Some(b"DDS ") => {
            let dds = ddsfile::Dds::read(&mut Cursor::new(bytes)).map_err(|e| fail(&e))?;
            let top_len = dds.get_main_texture_size().map_or(dds.data.len(), |n| n as usize).min(dds.data.len());
            let format = dds
                .get_dxgi_format()
                .map(|f| format!("{f:?}"))
                .or_else(|| dds.get_d3d_format().map(|f| format!("{f:?}")))
                .unwrap_or_else(|| "?".to_string())
                .to_ascii_lowercase();
            Ok(TextureFacts {
                format,
                width: u64::from(dds.get_width()),
                height: u64::from(dds.get_height()),
                mips: u64::from(dds.get_num_mipmap_levels().max(1)),
                top: xxh3_64(&dds.data[..top_len]),
            })
        }
        _ => Err(Error::Texture("not a tex or dds".to_string())),
    }
}
