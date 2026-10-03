//! An answer to a range request, by its shape (`docs/SOURCES.md`, "Ranges, not bundles"): a 206
//! with a `multipart/byteranges` body, a 206 with one range and a `Content-Range` header, or a 200
//! with the whole resource. Each part is read by the length its `Content-Range` gives, never by
//! looking for the boundary, which compressed bytes can hold.

use std::ops::Range;

/// The bytes a range request brought back, by where they start in the resource.
pub(crate) struct Ranges {
    body: Vec<u8>,
    /// Each part's start in the resource, and its bytes in `body`.
    parts: Vec<(u64, Range<usize>)>,
}

impl Ranges {
    /// The whole resource, from a 200.
    pub(crate) fn whole(body: Vec<u8>) -> Self {
        let parts = vec![(0, 0..body.len())];
        Self { body, parts }
    }

    /// One range, from a 206 whose `Content-Range` is `content_range`.
    pub(crate) fn single(body: Vec<u8>, content_range: &str) -> Result<Self, String> {
        let (start, end) = parse_content_range(content_range)?;
        if end - start != body.len() as u64 {
            return Err(format!("a body of {} bytes for Content-Range {content_range:?}", body.len()));
        }
        let parts = vec![(start, 0..body.len())];
        Ok(Self { body, parts })
    }

    /// The parts of a `multipart/byteranges` body. Anything before the first boundary is skipped.
    pub(crate) fn multipart(body: Vec<u8>, boundary: &str) -> Result<Self, String> {
        let delimiter = format!("--{boundary}").into_bytes();
        let mut at = find(&body, &delimiter, 0).ok_or("no boundary in the body")?;
        let mut parts = Vec::new();
        loop {
            at += delimiter.len();
            if body[at..].starts_with(b"--") {
                return Ok(Self { body, parts });
            }
            if !body[at..].starts_with(b"\r\n") {
                return Err(format!("a boundary at {at} not followed by a line end"));
            }
            at += 2;
            let head_end = find(&body, b"\r\n\r\n", at).ok_or("a part's headers do not end")?;
            let head = std::str::from_utf8(&body[at..head_end]).map_err(|_| "a part's headers are not UTF-8")?;
            let range = head
                .split("\r\n")
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.trim().eq_ignore_ascii_case("content-range").then_some(value.trim())
                })
                .ok_or_else(|| format!("a part with no Content-Range: {head:?}"))?;
            let (start, end) = parse_content_range(range)?;
            let data = head_end + 4;
            let data_end = usize::try_from(end - start)
                .ok()
                .and_then(|len| data.checked_add(len))
                .filter(|&end| end <= body.len())
                .ok_or_else(|| format!("the part {range:?} runs past the end of the body"))?;
            parts.push((start, data..data_end));
            at = data_end;
            if !body[at..].starts_with(b"\r\n") || !body[at + 2..].starts_with(&delimiter) {
                return Err(format!("the part {range:?} is not followed by a boundary"));
            }
            at += 2;
        }
    }

    /// The parts, for an error: how many, and the first and last few as `start-end`.
    pub(crate) fn shape(&self) -> String {
        let span = |(from, place): &(u64, Range<usize>)| format!("{from}-{}", from + place.len() as u64 - 1);
        let mut shown: Vec<String> = self.parts.iter().take(3).map(span).collect();
        if self.parts.len() > 6 {
            shown.push("...".into());
        }
        shown.extend(self.parts.iter().skip(3.max(self.parts.len().saturating_sub(3))).map(span));
        format!("{} parts in {} bytes: {}", self.parts.len(), self.body.len(), shown.join(","))
    }

    /// The bytes at `[start, start + len)` of the resource, if one part holds them all.
    pub(crate) fn get(&self, start: u64, len: u64) -> Option<&[u8]> {
        self.parts.iter().find_map(|(from, place)| {
            let skip = usize::try_from(start.checked_sub(*from)?).ok()?;
            let end = skip.checked_add(usize::try_from(len).ok()?)?;
            self.body[place.clone()].get(skip..end)
        })
    }
}

/// `bytes 10-19/100` as `(10, 20)`.
fn parse_content_range(value: &str) -> Result<(u64, u64), String> {
    let bad = || format!("Content-Range {value:?}");
    let span = value.strip_prefix("bytes ").and_then(|v| v.split_once('/')).ok_or_else(bad)?.0;
    let (first, last) = span.split_once('-').ok_or_else(bad)?;
    let first: u64 = first.parse().map_err(|_| bad())?;
    let last: u64 = last.parse().map_err(|_| bad())?;
    if last < first {
        return Err(bad());
    }
    Ok((first, last + 1))
}

fn find(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    haystack.get(from..)?.windows(needle.len()).position(|w| w == needle).map(|at| at + from)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(range: &str, data: &[u8]) -> Vec<u8> {
        let mut out = format!("\r\n--B0\r\nContent-Type: binary/octet-stream\r\nContent-Range: bytes {range}/100\r\n\r\n").into_bytes();
        out.extend(data);
        out
    }

    #[test]
    fn parts_are_read_by_their_length_even_when_they_hold_the_boundary() {
        let mut body = part("10-19", b"0123\r\n--B0");
        body.extend(part("40-41", b"xy"));
        body.extend(b"\r\n--B0--\r\n");
        let ranges = Ranges::multipart(body, "B0").unwrap();
        assert_eq!(ranges.get(10, 10), Some(&b"0123\r\n--B0"[..]));
        assert_eq!(ranges.get(14, 2), Some(&b"\r\n"[..]));
        assert_eq!(ranges.get(40, 2), Some(&b"xy"[..]));
        assert_eq!(ranges.get(19, 2), None);
        assert_eq!(ranges.get(30, 1), None);
    }

    #[test]
    fn a_short_or_unterminated_body_is_refused() {
        let mut short = part("10-19", b"0123");
        short.extend(b"\r\n--B0--\r\n");
        assert!(Ranges::multipart(short, "B0").is_err());
        assert!(Ranges::multipart(part("10-11", b"01"), "B0").is_err());
        assert!(Ranges::single(b"012".to_vec(), "bytes 5-8/100").is_err());
        assert_eq!(Ranges::single(b"0123".to_vec(), "bytes 5-8/100").unwrap().get(6, 2), Some(&b"12"[..]));
    }
}
