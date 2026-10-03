//! A skeleton's and a mesh's facts (`docs/FORMAT.md`, "skeleton", "mesh").

use std::io::Cursor;

use glam::Vec3;
use ltk_anim::RigResource;
use ltk_mesh::mem::vertex::ElementName;
use ltk_mesh::SkinnedMesh;
use xxhash_rust::xxh3::xxh3_64;

use crate::read::u32_at;
use crate::yaml::{Joint, MeshFacts, SkeletonFacts, SubmeshFacts};
use crate::Error;

/// A skeleton's facts from its bytes: a rig resource, or the `r3d2sklt` format before it.
pub fn skeleton_facts(bytes: &[u8]) -> Result<SkeletonFacts, Error> {
    if bytes.starts_with(b"r3d2sklt") {
        return legacy_skeleton_facts(bytes);
    }
    let rig = RigResource::from_reader(&mut Cursor::new(bytes)).map_err(|e| Error::Skeleton(e.to_string()))?;
    // A parent is named by its joint id; the ids are the ordinals in every rig shipped, and one
    // that is not is an error rather than a guess.
    let mut joints = Vec::with_capacity(rig.joints().len());
    for (i, j) in rig.joints().iter().enumerate() {
        if j.id() as usize != i {
            return Err(Error::Skeleton(format!("joint {i} has id {}", j.id())));
        }
        // A root's parent is -1.
        joints.push(Joint { name: j.name().to_string(), parent: usize::try_from(j.parent_id() as i32).ok() });
    }
    Ok(SkeletonFacts {
        influences: rig.influences().len() as u64,
        name: rig.name().to_string(),
        asset: rig.asset_name().to_string(),
        joints,
    })
}

/// The skeleton format before the rig resource, shipped into season 11: `r3d2sklt`, u32 version,
/// u32 designer id, u32 joint count, then per joint a 32-byte name, an i32 parent, an f32 radius
/// and a 3 by 4 transform; version 2 ends with the influences, a u32 count and one u32 each. No
/// name or asset name.
fn legacy_skeleton_facts(bytes: &[u8]) -> Result<SkeletonFacts, Error> {
    const JOINT: usize = 32 + 4 + 4 + 12 * 4;
    let u32_at = |pos: usize| u32_at(bytes, pos).ok_or_else(|| Error::Skeleton(format!("legacy skeleton truncated at {pos}")));
    let version = u32_at(8)?;
    if !(1..=2).contains(&version) {
        return Err(Error::Skeleton(format!("legacy skeleton version {version}")));
    }
    let count = u32_at(16)? as usize;
    let mut joints = Vec::new();
    let mut pos = 20;
    for i in 0..count {
        let raw = bytes.get(pos..pos + 32).ok_or_else(|| Error::Skeleton(format!("legacy skeleton truncated in joint {i}")))?;
        let end = raw.iter().position(|&b| b == 0).unwrap_or(32);
        let name = std::str::from_utf8(&raw[..end]).map_err(|e| Error::Skeleton(e.to_string()))?;
        joints.push(Joint { name: name.to_string(), parent: usize::try_from(u32_at(pos + 32)? as i32).ok() });
        pos += JOINT;
    }
    let influences = match version {
        2 => u32_at(pos)? as usize,
        _ => count,
    };
    Ok(SkeletonFacts { influences: influences as u64, name: String::new(), asset: String::new(), joints })
}

/// A mesh's facts from its bytes. Each submesh's positions are hashed as stored, and rounded to a
/// sixteenth of a unit and sorted, so a re-export with float noise or another vertex order still
/// matches.
pub fn mesh_facts(bytes: &[u8]) -> Result<MeshFacts, Error> {
    let mesh = SkinnedMesh::from_reader(&mut Cursor::new(bytes)).map_err(|e| Error::Mesh(e.to_string()))?;
    let positions = mesh
        .vertex_buffer()
        .accessor::<Vec3>(ElementName::Position)
        .ok_or_else(|| Error::Mesh("no positions".to_string()))?;
    let mut submeshes = Vec::with_capacity(mesh.ranges().len());
    for range in mesh.ranges() {
        let start = range.start_vertex.max(0) as usize;
        let count = range.vertex_count.max(0) as usize;
        let end = start.saturating_add(count).min(positions.len());
        let mut exact = Vec::with_capacity(count * 12);
        let mut rounded: Vec<[i32; 3]> = Vec::with_capacity(count);
        for v in start..end {
            let p = positions.get(v);
            exact.extend_from_slice(&p.x.to_le_bytes());
            exact.extend_from_slice(&p.y.to_le_bytes());
            exact.extend_from_slice(&p.z.to_le_bytes());
            rounded.push([(p.x * 16.0).round() as i32, (p.y * 16.0).round() as i32, (p.z * 16.0).round() as i32]);
        }
        rounded.sort_unstable();
        let rounded: Vec<u8> = rounded.iter().flatten().flat_map(|c| c.to_le_bytes()).collect();
        submeshes.push(SubmeshFacts {
            name: range.material.clone(),
            vertices: (end - start) as u64,
            indices: range.index_count.max(0) as u64,
            positions: xxh3_64(&exact),
            rounded: xxh3_64(&rounded),
        });
    }
    Ok(MeshFacts {
        vertex_type: mesh.vertex_type().map(|t| format!("{t:?}")).unwrap_or_default(),
        vertices: mesh.vertex_buffer().count() as u64,
        indices: mesh.index_buffer().count() as u64,
        submeshes,
    })
}
