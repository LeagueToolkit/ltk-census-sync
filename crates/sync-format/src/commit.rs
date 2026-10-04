//! A build's commit on `history-v2` (`docs/FORMAT.md`, "Branches and commits"): its author, date
//! and message are functions of the build, so the same build appended twice makes the same commit.

use crate::yaml::BuildFacts;

/// The author and committer of every commit.
pub const AUTHOR: &str = "census <census@localhost>";

/// A build's commit message: its version, a blank line, then the `Census-*` trailers.
pub fn commit_message(build: &BuildFacts) -> String {
    format!(
        "{}\n\nCensus-Patch: {}\nCensus-Manifest: {:016x}\nCensus-Source: {}\nCensus-Realms: {}\n",
        build.version,
        build.patch,
        build.manifest,
        build.source(),
        build.sorted_realms().join(" ")
    )
}

/// A build's commit time: midnight UTC of its date, in seconds since the epoch. None for a date
/// that is not `YYYY-MM-DD`.
pub fn commit_time(date: &str) -> Option<i64> {
    let bytes = date.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let field = |range: std::ops::Range<usize>| date[range].parse::<i64>().ok();
    let (y, m, d) = (field(0..4)?, field(5..7)?, field(8..10)?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some(days_from_civil(y, m, d) * 86_400)
}

/// Days since the epoch, by Howard Hinnant's civil calendar.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}
