//! Little-endian integers at an offset, none past the end.

pub(crate) fn u16_at(bytes: &[u8], pos: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(pos..pos.checked_add(2)?)?.try_into().ok()?))
}

pub(crate) fn u32_at(bytes: &[u8], pos: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(pos..pos.checked_add(4)?)?.try_into().ok()?))
}
