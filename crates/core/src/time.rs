//! Dagu writes timestamps as `...Z`, as `...-07:00` with milliseconds, and as `""` or `"-"` for
//! a time that has not happened. The last two mean "no time", which is not an error.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer};

pub fn parse(raw: &str) -> Option<DateTime<Utc>> {
    let raw = raw.trim();
    if raw.is_empty() || raw == "-" {
        return None;
    }
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

pub(crate) fn lenient<'de, D>(d: D) -> Result<Option<DateTime<Utc>>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(d)?.and_then(|s| parse(&s)))
}
