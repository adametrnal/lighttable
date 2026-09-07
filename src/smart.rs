//! A bounded data parser for Lightroom's saved rule tables. Never executes Lua.
use anyhow::{Context, Result, bail, ensure};
use chrono::{Days, Local, Months, NaiveDate};
use std::collections::BTreeMap;

pub fn today() -> NaiveDate {
    Local::now().date_naive()
}

pub fn date(value: &str) -> Option<NaiveDate> {
    (value.len() == 10 && value.as_bytes()[4] == b'-' && value.as_bytes()[7] == b'-')
        .then(|| NaiveDate::parse_from_str(value, "%Y-%m-%d").ok())
        .flatten()
}

#[derive(Debug)]
pub enum Rule {
    All(Vec<Rule>),
    Any(Vec<Rule>),
    Rating(Comparison, i8),
    Capture(Comparison, NaiveDate),
    CaptureRange(NaiveDate, NaiveDate),
    CaptureLast(u32, Unit),
}

#[derive(Debug, Clone, Copy)]
pub enum Comparison {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl Comparison {
    fn parse(value: &str) -> Result<Self> {
        Ok(match value {
            "==" => Self::Eq,
            "!=" => Self::Ne,
            "<" => Self::Lt,
            "<=" => Self::Le,
            ">" => Self::Gt,
            ">=" => Self::Ge,
            _ => bail!("Unsupported comparison: {value}"),
        })
    }
    fn matches<T: Ord>(&self, actual: T, expected: T) -> bool {
        match self {
            Self::Eq => actual == expected,
            Self::Ne => actual != expected,
            Self::Lt => actual < expected,
            Self::Le => actual <= expected,
            Self::Gt => actual > expected,
            Self::Ge => actual >= expected,
        }
    }
    fn label(&self) -> &'static str {
        match self {
            Self::Eq => "=",
            Self::Ne => "≠",
            Self::Lt => "<",
            Self::Le => "≤",
            Self::Gt => ">",
            Self::Ge => "≥",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Unit {
    Days,
    Months,
}

impl Rule {
    pub fn matches(&self, rating: i8, captured: Option<NaiveDate>, today: NaiveDate) -> bool {
        match self {
            Self::All(rules) => rules.iter().all(|r| r.matches(rating, captured, today)),
            Self::Any(rules) => rules.iter().any(|r| r.matches(rating, captured, today)),
            Self::Rating(op, value) => op.matches(rating, *value),
            Self::Capture(op, value) => captured.is_some_and(|d| op.matches(d, *value)),
            Self::CaptureRange(start, end) => {
                captured.is_some_and(|d| (*start..=*end).contains(&d))
            }
            Self::CaptureLast(amount, unit) => {
                let start = match unit {
                    Unit::Days => today.checked_sub_days(Days::new(u64::from(*amount))),
                    Unit::Months => today.checked_sub_months(Months::new(*amount)),
                };
                captured
                    .zip(start)
                    .is_some_and(|(d, start)| (start..=today).contains(&d))
            }
        }
    }

    pub fn description(&self) -> String {
        match self {
            Self::All(rules) | Self::Any(rules) => {
                let join = if matches!(self, Self::All(_)) {
                    " AND "
                } else {
                    " OR "
                };
                format!(
                    "({})",
                    rules
                        .iter()
                        .map(Self::description)
                        .collect::<Vec<_>>()
                        .join(join)
                )
            }
            Self::Rating(op, value) => format!("Rating {} {value}", op.label()),
            Self::Capture(op, value) => format!("Capture date {} {value}", op.label()),
            Self::CaptureRange(start, end) => format!("Capture date {start} through {end}"),
            Self::CaptureLast(n, unit) => format!(
                "Capture date in last {n} {}",
                match unit {
                    Unit::Days => "days",
                    Unit::Months => "months",
                }
            ),
        }
    }
}

#[derive(Debug)]
enum Value {
    Text(String),
    Number(f64),
    Table(Table),
}

#[derive(Debug, Default)]
struct Table {
    fields: BTreeMap<String, Value>,
    items: Vec<Value>,
}

impl Table {
    fn text(&self, name: &str) -> Result<&str> {
        match self.fields.get(name) {
            Some(Value::Text(s)) => Ok(s),
            _ => bail!("Missing or invalid '{name}' in Smart rule"),
        }
    }
    fn integer(&self, name: &str, max: u32) -> Result<u32> {
        match self.fields.get(name) {
            Some(Value::Number(n)) if n.fract() == 0.0 && (0.0..=f64::from(max)).contains(n) => {
                Ok(*n as u32)
            }
            _ => bail!("Invalid '{name}': expected an integer from 0 to {max}"),
        }
    }
    fn only(&self, keys: &[&str]) -> Result<()> {
        for key in self.fields.keys() {
            ensure!(
                keys.contains(&key.as_str()),
                "Unsupported Smart rule field: {key}"
            );
        }
        Ok(())
    }
    fn rule(self) -> Result<Rule> {
        if self.fields.contains_key("combine") {
            self.only(&["combine"])?;
            ensure!(!self.items.is_empty(), "Empty Smart rule group");
            let mode = self.text("combine")?.to_owned();
            // Compile every branch, including OR branches. Never drop an unsupported predicate.
            let rules = self
                .items
                .into_iter()
                .map(|v| match v {
                    Value::Table(t) => t.rule(),
                    _ => bail!("Expected a Smart rule table"),
                })
                .collect::<Result<Vec<_>>>()?;
            return Ok(match mode.as_str() {
                "intersect" => Rule::All(rules),
                "union" => Rule::Any(rules),
                _ => bail!("Unsupported Smart rule combination: {mode}"),
            });
        }
        ensure!(
            self.items.is_empty(),
            "Unexpected nested values in a Smart predicate"
        );
        let criterion = self.text("criteria")?;
        let op = self.text("operation")?;
        match criterion {
            "rating" => {
                self.only(&["criteria", "operation", "value", "value2"])?;
                Ok(Rule::Rating(
                    Comparison::parse(op)?,
                    self.integer("value", 5)? as i8,
                ))
            }
            "captureTime" => {
                if op == "inLast" {
                    self.only(&["criteria", "operation", "value", "value2", "value_units"])?;
                    let unit = match self.text("value_units")? {
                        "days" => Unit::Days,
                        "months" => Unit::Months,
                        other => bail!("Unsupported capture-date unit: {other}"),
                    };
                    return Ok(Rule::CaptureLast(self.integer("value", 100_000)?, unit));
                }
                self.only(&["criteria", "operation", "value", "value2"])?;
                let start = date(self.text("value")?)
                    .context("Invalid capture date; expected YYYY-MM-DD")?;
                if op == "in" {
                    let end = date(self.text("value2")?).context("Invalid capture range end")?;
                    ensure!(end >= start, "Capture date range ends before it starts");
                    Ok(Rule::CaptureRange(start, end))
                } else {
                    // Lightroom retains inactive value2 fields after changing the operator.
                    // Equality compares only value, even when a previous range end remains.
                    Ok(Rule::Capture(Comparison::parse(op)?, start))
                }
            }
            "touchTime" => bail!("Lightroom edit-date rules are not supported yet"),
            "labelColor" => bail!("Color label rules are not supported yet"),
            "collection" => bail!("Rules matching collection names are not supported yet"),
            "fileFormat" => bail!("File type rules are not supported yet"),
            "filename" => bail!("Filename rules are not supported yet"),
            "keywords" => bail!("Keyword rules are not supported yet"),
            "aperture" => bail!("Aperture rules are not supported yet"),
            other => bail!("Unsupported Smart criterion: {other}"),
        }
    }
}

struct Parser<'a> {
    input: &'a [u8],
    pos: usize,
    values: usize,
}

impl Parser<'_> {
    fn space(&mut self) {
        while self
            .input
            .get(self.pos)
            .is_some_and(u8::is_ascii_whitespace)
        {
            self.pos += 1;
        }
    }
    fn take(&mut self, byte: u8) -> bool {
        self.space();
        if self.input.get(self.pos) == Some(&byte) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    fn expect(&mut self, byte: u8) -> Result<()> {
        ensure!(
            self.take(byte),
            "Expected '{}' at byte {}",
            char::from(byte),
            self.pos
        );
        Ok(())
    }
    fn identifier(&mut self) -> Result<String> {
        self.space();
        let start = self.pos;
        ensure!(
            self.input
                .get(start)
                .is_some_and(|b| b.is_ascii_alphabetic() || *b == b'_'),
            "Expected field name"
        );
        while self
            .input
            .get(self.pos)
            .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
        {
            self.pos += 1;
        }
        Ok(std::str::from_utf8(&self.input[start..self.pos])?.into())
    }
    fn value(&mut self, depth: usize) -> Result<Value> {
        self.values += 1;
        ensure!(
            self.values <= 512 && depth <= 16,
            "Smart rule exceeds complexity limit"
        );
        self.space();
        match self.input.get(self.pos).copied() {
            Some(b'{') => {
                self.pos += 1;
                let mut table = Table::default();
                while !self.take(b'}') {
                    self.space();
                    if self
                        .input
                        .get(self.pos)
                        .is_some_and(|b| b.is_ascii_alphabetic() || *b == b'_')
                    {
                        let key = self.identifier()?;
                        self.expect(b'=')?;
                        let value = self.value(depth + 1)?;
                        ensure!(
                            table.fields.insert(key, value).is_none(),
                            "Duplicate Smart rule field"
                        );
                    } else {
                        table.items.push(self.value(depth + 1)?);
                    }
                    if !self.take(b',') {
                        self.expect(b'}')?;
                        break;
                    }
                }
                Ok(Value::Table(table))
            }
            Some(quote @ (b'"' | b'\'')) => {
                self.pos += 1;
                let mut bytes = Vec::new();
                loop {
                    let b = *self
                        .input
                        .get(self.pos)
                        .context("Unterminated Smart rule string")?;
                    self.pos += 1;
                    if b == quote {
                        break;
                    }
                    bytes.push(if b == b'\\' {
                        let b = *self
                            .input
                            .get(self.pos)
                            .context("Unterminated string escape")?;
                        self.pos += 1;
                        match b {
                            b'\\' | b'"' | b'\'' => b,
                            b'n' => b'\n',
                            b'r' => b'\r',
                            b't' => b'\t',
                            _ => bail!("Unsupported string escape"),
                        }
                    } else {
                        b
                    });
                }
                Ok(Value::Text(String::from_utf8(bytes)?))
            }
            Some(b'-' | b'0'..=b'9') => {
                let start = self.pos;
                while self
                    .input
                    .get(self.pos)
                    .is_some_and(|b| b.is_ascii_digit() || b"+-.eE".contains(b))
                {
                    self.pos += 1;
                }
                let n: f64 = std::str::from_utf8(&self.input[start..self.pos])?.parse()?;
                ensure!(n.is_finite(), "Invalid Smart rule number");
                Ok(Value::Number(n))
            }
            _ => bail!("Unsupported Smart rule syntax at byte {}", self.pos),
        }
    }
}

pub fn parse(source: &str) -> Result<Rule> {
    ensure!(source.len() <= 65_536, "Smart rule exceeds size limit");
    let mut parser = Parser {
        input: source.as_bytes(),
        pos: 0,
        values: 0,
    };
    ensure!(
        parser.identifier()? == "s",
        "Expected saved Smart rule table 's'"
    );
    parser.expect(b'=')?;
    let Value::Table(table) = parser.value(0)? else {
        bail!("Expected Smart rule table");
    };
    parser.space();
    ensure!(
        parser.pos == source.len(),
        "Unexpected content after Smart rule table"
    );
    table.rule()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(value: &str) -> NaiveDate {
        date(value).unwrap()
    }
    fn rating(op: &str, value: i8) -> Rule {
        parse(&format!("s = {{ {{ criteria='rating', operation='{op}', value={value}, }}, combine='intersect', }}")).unwrap()
    }

    #[test]
    fn stars_include_zero_and_all_comparisons() {
        for (op, expected) in [
            ("==", vec![3]),
            ("!=", vec![0, 1, 2, 4, 5]),
            ("<", vec![0, 1, 2]),
            ("<=", vec![0, 1, 2, 3]),
            (">", vec![4, 5]),
            (">=", vec![3, 4, 5]),
        ] {
            let rule = rating(op, 3);
            let actual: Vec<_> = (0..=5)
                .filter(|&n| rule.matches(n, None, d("2026-01-01")))
                .collect();
            assert_eq!(actual, expected);
        }
        assert!(rating("==", 0).matches(0, None, d("2026-01-01")));
    }

    #[test]
    fn observed_rating_and_date_range_include_both_whole_end_days() {
        let rule = parse(r#"s = {
            { criteria = "rating", operation = ">=", value = 3, },
            { criteria = "captureTime", operation = "in", value = "2021-03-25", value2 = "2021-07-18", },
            combine = "intersect", }"#).unwrap();
        let today = d("2026-09-06");
        for day in ["2021-03-25", "2021-05-01", "2021-07-18"] {
            assert!(rule.matches(3, Some(d(day)), today));
            assert!(!rule.matches(2, Some(d(day)), today));
        }
        for day in ["2021-03-24", "2021-07-19"] {
            assert!(!rule.matches(5, Some(d(day)), today));
        }
        assert!(!rule.matches(5, None, today));
    }

    #[test]
    fn exact_date_ignores_inactive_range_end_and_unknown_dates_never_match() {
        let rule = parse("s={{criteria='captureTime',operation='==',value='2009-08-15',value2='2009-08-19'},combine='intersect'}").unwrap();
        assert!(rule.matches(0, Some(d("2009-08-15")), d("2026-01-01")));
        assert!(!rule.matches(5, Some(d("2009-08-19")), d("2026-01-01")));
        let neq = parse(
            "s={{criteria='captureTime',operation='!=',value='2009-08-15'},combine='intersect'}",
        )
        .unwrap();
        assert!(!neq.matches(5, None, d("2026-01-01")));
        for bad in [
            "",
            "2026-02-30",
            "2026-9-06",
            "2026-01-01T12:00:00",
            "abcdefghij",
        ] {
            assert!(date(bad).is_none());
        }
    }

    #[test]
    fn relative_dates_use_frozen_local_day_and_calendar_months() {
        let days = parse("s={{criteria='captureTime',operation='inLast',value=2,value2=1,value_units='days'},combine='intersect'}").unwrap();
        let today = d("2024-03-31");
        for (day, expected) in [
            ("2024-03-28", false),
            ("2024-03-29", true),
            ("2024-03-31", true),
            ("2024-04-01", false),
        ] {
            assert_eq!(days.matches(0, Some(d(day)), today), expected);
        }
        assert!(!days.matches(5, Some(d("2024-03-29")), d("2024-04-01")));
        let month = parse("s={{criteria='captureTime',operation='inLast',value=1,value_units='months'},combine='intersect'}").unwrap();
        assert!(month.matches(0, Some(d("2024-02-29")), today));
        assert!(!month.matches(0, Some(d("2024-02-28")), today));
        assert!(month.matches(0, Some(d("2023-02-28")), d("2023-03-31")));
        assert!(month.matches(0, Some(d("2023-12-31")), d("2024-01-31")));
        assert!(!month.matches(0, None, today));
    }

    #[test]
    fn nested_all_any_and_unsupported_or_branches_are_not_dropped() {
        let rule = parse(
            "s={
            {criteria='rating',operation='==',value=5},
            {{criteria='rating',operation='>=',value=3},
             {criteria='captureTime',operation='==',value='2026-01-01'}, combine='intersect'},
             combine='union'}",
        )
        .unwrap();
        let today = d("2026-01-01");
        assert!(rule.matches(5, None, today));
        assert!(rule.matches(3, Some(today), today));
        assert!(!rule.matches(2, Some(today), today));
        assert!(!rule.matches(3, Some(d("2025-12-31")), today));
        let error = parse("s={{criteria='rating',operation='>=',value=0}, {criteria='labelColor',operation='==',value=3},combine='union'}").unwrap_err();
        assert!(error.to_string().contains("Color label"));
    }

    #[test]
    fn malformed_unknown_and_executable_input_is_rejected_with_bounds() {
        for text in [
            "s={combine='intersect'}",
            "s={}",
            "s={{criteria='rating',operation='>=',value=6},combine='intersect'}",
            "s={{criteria='rating',operation='>=',value=3,value=4},combine='intersect'}",
            "s={{criteria='rating',operation='>=',value=3,negate=true},combine='intersect'}",
            "s={{criteria='rating',operation='>=',value=2+3},combine='intersect'}",
            "s={{criteria='rating',operation='>=',value=1e999},combine='intersect'}",
            "s={{criteria='rating',operation='>=',value=os.execute('anything')},combine='intersect'}",
            "s={{criteria='rating',operation='>=',value=3},combine='union'}; os.execute('anything')",
            "s={{criteria='captureTime',operation='in',value='2026-02-02',value2='2026-01-01'},combine='intersect'}",
            "s={{criteria='captureTime',operation='inLast',value=1,value_units='hours'},combine='intersect'}",
            "s={{criteria='captureTime',operation='inLast',value=-1,value_units='days'},combine='intersect'}",
            "s={{criteria='captureTime',operation='inLast',value=1.5,value_units='days'},combine='intersect'}",
            "s={{criteria='rating',operation='>=',value=3},combine='exclude'}",
            "s={criteria='rating",
            "s={{criteria='rating',operation='>=',value=3},combine='intersect',unexpected=1}",
        ] {
            assert!(parse(text).is_err(), "accepted {text}");
        }
        assert!(parse(&" ".repeat(65_537)).is_err());
        assert!(parse(&format!("s={}{}", "{".repeat(50), "}".repeat(50))).is_err());
        assert!(
            parse(&format!(
                "s={{{}combine='union'}}",
                "{criteria='rating',operation='==',value=3},".repeat(300)
            ))
            .is_err()
        );
    }
}
