use chrono::{DateTime, NaiveDate, ParseResult, TimeZone};
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct DateFormat(String);

impl DateFormat {
    pub fn new(pattern: &str) -> Self {
        let strftime = if pattern.contains('%') {
            pattern.to_string()
        } else {
            translate_dsl(pattern)
        };
        Self(strftime)
    }

    pub fn display<Tz: TimeZone>(&self, date: &DateTime<Tz>) -> String
    where
        Tz::Offset: std::fmt::Display,
    {
        date.format(&self.0).to_string()
    }

    pub fn parse(&self, s: &str) -> ParseResult<NaiveDate> {
        NaiveDate::parse_from_str(s, &self.0)
    }
}

impl Default for DateFormat {
    fn default() -> Self {
        Self::new("DD-MM-YYYY")
    }
}

impl<'de> Deserialize<'de> for DateFormat {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(Self::new(&s))
    }
}

fn translate_dsl(pattern: &str) -> String {
    let mut out = String::new();
    let mut remaining = pattern;
    while !remaining.is_empty() {
        if let Some(rest) = remaining.strip_prefix("YYYY") {
            out.push_str("%Y");
            remaining = rest;
        } else if let Some(rest) = remaining.strip_prefix("YY") {
            out.push_str("%y");
            remaining = rest;
        } else if let Some(rest) = remaining.strip_prefix("MM") {
            out.push_str("%m");
            remaining = rest;
        } else if let Some(rest) = remaining.strip_prefix("M") {
            out.push_str("%-m");
            remaining = rest;
        } else if let Some(rest) = remaining.strip_prefix("DD") {
            out.push_str("%d");
            remaining = rest;
        } else if let Some(rest) = remaining.strip_prefix("D") {
            out.push_str("%-d");
            remaining = rest;
        } else {
            let mut chars = remaining.chars();
            if let Some(c) = chars.next() {
                out.push(c);
            }
            remaining = chars.as_str();
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn dsl_translates_basic_tokens() {
        assert_eq!(translate_dsl("DD-MM-YYYY"), "%d-%m-%Y");
        assert_eq!(translate_dsl("D/M/YY"), "%-d/%-m/%y");
        assert_eq!(translate_dsl("YYYY.MM.DD"), "%Y.%m.%d");
    }

    #[test]
    fn dsl_passes_through_literals() {
        assert_eq!(translate_dsl("foo DD bar"), "foo %d bar");
        assert_eq!(translate_dsl(""), "");
    }

    #[test]
    fn new_accepts_raw_strftime() {
        let df = DateFormat::new("%d/%m/%Y");
        let date = Utc.with_ymd_and_hms(2026, 4, 23, 0, 0, 0).unwrap();
        assert_eq!(df.display(&date), "23/04/2026");
    }

    #[test]
    fn new_translates_dsl_to_strftime() {
        let df = DateFormat::new("DD/MM/YYYY");
        let date = Utc.with_ymd_and_hms(2026, 4, 23, 0, 0, 0).unwrap();
        assert_eq!(df.display(&date), "23/04/2026");
    }

    #[test]
    fn default_is_padded_dmy_dashes() {
        let df = DateFormat::default();
        let date = Utc.with_ymd_and_hms(2026, 4, 23, 0, 0, 0).unwrap();
        assert_eq!(df.display(&date), "23-04-2026");
    }

    #[test]
    fn display_formats_padded() {
        let df = DateFormat::new("DD-MM-YYYY");
        let date = Utc.with_ymd_and_hms(2026, 4, 23, 0, 0, 0).unwrap();
        assert_eq!(df.display(&date), "23-04-2026");
    }

    #[test]
    fn display_respects_unpadded_tokens() {
        let df = DateFormat::new("D/M/YY");
        let date = Utc.with_ymd_and_hms(2026, 4, 3, 0, 0, 0).unwrap();
        assert_eq!(df.display(&date), "3/4/26");
    }

    #[test]
    fn parse_round_trips() {
        let df = DateFormat::new("DD-MM-YYYY");
        let date = df.parse("23-04-2026").unwrap();
        assert_eq!(date, NaiveDate::from_ymd_opt(2026, 4, 23).unwrap());
    }
}
