use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TokenKind {
    Word(String),
    Phrase(String),
    Colon,
    Comparison(Comparison),
    LeftParen,
    RightParen,
    And,
    Or,
    Not,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PropertyKey {
    Name,
    Type,
    Size,
    DateModified,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Comparison {
    Equal,
    Greater,
    GreaterOrEqual,
    Less,
    LessOrEqual,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct SizeValue(pub u64);

/// A validated Gregorian date, kept as an ordinal suitable for comparisons.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct DateValue {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}

impl DateValue {
    pub fn parse(value: &str) -> Option<Self> {
        let mut fields = value.split('-');
        let year = fields.next()?.parse().ok()?;
        let month = fields.next()?.parse().ok()?;
        let day = fields.next()?.parse().ok()?;
        if fields.next().is_some() || year < 1601 || !(1..=12).contains(&month) {
            return None;
        }
        let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
        let days = [
            31,
            if leap { 29 } else { 28 },
            31,
            30,
            31,
            30,
            31,
            31,
            30,
            31,
            30,
            31,
        ];
        (day != 0 && day <= days[usize::from(month - 1)]).then_some(Self { year, month, day })
    }

    pub(crate) fn days_since_unix_epoch(self) -> i64 {
        // Howard Hinnant's civil-date conversion; comparisons remain valid before the epoch too.
        let mut year = i64::from(self.year);
        let month = i64::from(self.month);
        let day = i64::from(self.day);
        year -= i64::from(month <= 2);
        let era = if year >= 0 { year } else { year - 399 } / 400;
        let year_of_era = year - era * 400;
        let adjusted_month = month + if month > 2 { -3 } else { 9 };
        let day_of_year = (153 * adjusted_month + 2) / 5 + day - 1;
        let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
        era * 146_097 + day_of_era - 719_468
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Value {
    Text(String),
    Size(SizeValue),
    Date(DateValue),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Expr {
    Text {
        value: String,
        phrase: bool,
        glob: bool,
    },
    Filter {
        key: PropertyKey,
        comparison: Comparison,
        value: Value,
    },
    Not(Box<Self>),
    And(Box<Self>, Box<Self>),
    Or(Box<Self>, Box<Self>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParseError {
    pub span: Span,
    pub message: String,
    pub suggestion: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} at bytes {}..{}; {}",
            self.message, self.span.start, self.span.end, self.suggestion
        )
    }
}

impl std::error::Error for ParseError {}

/// Parses dedicated search text without applying address/location semantics.
///
/// # Errors
///
/// Returns an actionable byte span and suggestion for lexical or grammar errors.
pub fn parse(input: &str) -> Result<Expr, ParseError> {
    let tokens = lex(input)?;
    if tokens.is_empty() {
        return Err(error(0, 0, "搜尋內容是空的", "請輸入檔名或屬性條件"));
    }
    Parser {
        tokens: &tokens,
        cursor: 0,
        input_len: input.len(),
    }
    .parse_all()
}

#[allow(
    clippy::too_many_lines,
    reason = "the UTF-8 lexer keeps every cursor advance and source span in one auditable loop"
)]
fn lex(input: &str) -> Result<Vec<Token>, ParseError> {
    let mut tokens = Vec::new();
    let mut cursor = 0;
    while cursor < input.len() {
        let ch = next_char(input, cursor)?;
        if ch.is_whitespace() {
            cursor += ch.len_utf8();
            continue;
        }
        let start = cursor;
        let single = match ch {
            ':' => Some(TokenKind::Colon),
            '(' => Some(TokenKind::LeftParen),
            ')' => Some(TokenKind::RightParen),
            '>' | '<' | '=' => {
                cursor += ch.len_utf8();
                let equal = input
                    .get(cursor..)
                    .is_some_and(|rest| rest.starts_with('='));
                if equal {
                    cursor += 1;
                }
                let comparison = match (ch, equal) {
                    ('>', false) => Comparison::Greater,
                    ('>', true) => Comparison::GreaterOrEqual,
                    ('<', false) => Comparison::Less,
                    ('<', true) => Comparison::LessOrEqual,
                    ('=', _) => Comparison::Equal,
                    _ => {
                        return Err(error(
                            start,
                            cursor,
                            "invalid comparison operator",
                            "use >, >=, <, <=, or =",
                        ));
                    }
                };
                tokens.push(Token {
                    kind: TokenKind::Comparison(comparison),
                    span: Span { start, end: cursor },
                });
                continue;
            }
            _ => None,
        };
        if let Some(kind) = single {
            cursor += ch.len_utf8();
            tokens.push(Token {
                kind,
                span: Span { start, end: cursor },
            });
            continue;
        }
        if ch == '"' {
            cursor += 1;
            let mut value = String::new();
            let mut closed = false;
            while cursor < input.len() {
                let current = next_char(input, cursor)?;
                cursor += current.len_utf8();
                if current == '"' {
                    closed = true;
                    break;
                }
                if current == '\\' {
                    let Some(escaped) = input.get(cursor..).and_then(|rest| rest.chars().next())
                    else {
                        break;
                    };
                    if !matches!(escaped, '"' | '\\') {
                        return Err(error(
                            cursor - 1,
                            cursor + escaped.len_utf8(),
                            "不支援的跳脫字元",
                            r#"只可使用 \" 或 \\"#,
                        ));
                    }
                    cursor += escaped.len_utf8();
                    value.push(escaped);
                } else {
                    value.push(current);
                }
            }
            if !closed {
                return Err(error(
                    start,
                    input.len(),
                    "引號沒有結束",
                    "在片語尾端加入引號",
                ));
            }
            tokens.push(Token {
                kind: TokenKind::Phrase(value),
                span: Span { start, end: cursor },
            });
            continue;
        }
        while cursor < input.len() {
            let current = next_char(input, cursor)?;
            if current.is_whitespace() || matches!(current, ':' | '(' | ')' | '>' | '<' | '=' | '"')
            {
                break;
            }
            cursor += current.len_utf8();
        }
        let value = &input[start..cursor];
        let kind = if value.eq_ignore_ascii_case("AND") {
            TokenKind::And
        } else if value.eq_ignore_ascii_case("OR") {
            TokenKind::Or
        } else if value.eq_ignore_ascii_case("NOT") {
            TokenKind::Not
        } else {
            TokenKind::Word(value.to_owned())
        };
        tokens.push(Token {
            kind,
            span: Span { start, end: cursor },
        });
    }
    Ok(tokens)
}

fn next_char(input: &str, cursor: usize) -> Result<char, ParseError> {
    input
        .get(cursor..)
        .and_then(|rest| rest.chars().next())
        .ok_or_else(|| {
            error(
                cursor.min(input.len()),
                cursor.min(input.len()),
                "invalid UTF-8 cursor boundary",
                "edit the query near this position and try again",
            )
        })
}

struct Parser<'a> {
    tokens: &'a [Token],
    cursor: usize,
    input_len: usize,
}

impl Parser<'_> {
    fn parse_all(mut self) -> Result<Expr, ParseError> {
        let expression = self.parse_or()?;
        if let Some(token) = self.peek() {
            return Err(error(
                token.span.start,
                token.span.end,
                "這裡需要布林運算子或查詢結尾",
                "移除多餘符號，或加入 AND / OR",
            ));
        }
        Ok(expression)
    }

    fn parse_or(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_and()?;
        while self.consume(|kind| matches!(kind, TokenKind::Or)).is_some() {
            let right = self.parse_and().map_err(|_| self.missing_operand("OR"))?;
            left = Expr::Or(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_not()?;
        loop {
            if self
                .consume(|kind| matches!(kind, TokenKind::And))
                .is_some()
            {
                let right = self.parse_not().map_err(|_| self.missing_operand("AND"))?;
                left = Expr::And(Box::new(left), Box::new(right));
            } else if self
                .peek()
                .is_some_and(|token| starts_expression(&token.kind))
            {
                let right = self.parse_not()?;
                left = Expr::And(Box::new(left), Box::new(right));
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_not(&mut self) -> Result<Expr, ParseError> {
        if self
            .consume(|kind| matches!(kind, TokenKind::Not))
            .is_some()
        {
            return Ok(Expr::Not(Box::new(
                self.parse_not().map_err(|_| self.missing_operand("NOT"))?,
            )));
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        if self
            .consume(|kind| matches!(kind, TokenKind::LeftParen))
            .is_some()
        {
            let expression = self.parse_or()?;
            if self
                .consume(|kind| matches!(kind, TokenKind::RightParen))
                .is_none()
            {
                return Err(error(
                    self.input_len,
                    self.input_len,
                    "括號沒有結束",
                    "加入右括號 )",
                ));
            }
            return Ok(expression);
        }
        let Some(token) = self.next().cloned() else {
            return Err(error(
                self.input_len,
                self.input_len,
                "缺少查詢條件",
                "在運算子後加入檔名或條件",
            ));
        };
        match token.kind {
            TokenKind::Phrase(value) => Ok(Expr::Text {
                value,
                phrase: true,
                glob: false,
            }),
            TokenKind::Word(value) => {
                if self
                    .consume(|kind| matches!(kind, TokenKind::Colon))
                    .is_some()
                {
                    self.parse_filter(&value, token.span)
                } else {
                    let glob = has_unescaped_wildcard(&value);
                    Ok(Expr::Text {
                        value,
                        phrase: false,
                        glob,
                    })
                }
            }
            TokenKind::RightParen => Err(error(
                token.span.start,
                token.span.end,
                "多餘的右括號",
                "移除這個右括號",
            )),
            _ => Err(error(
                token.span.start,
                token.span.end,
                "此處需要查詢條件",
                "輸入檔名、片語或屬性條件",
            )),
        }
    }

    fn parse_filter(&mut self, property: &str, property_span: Span) -> Result<Expr, ParseError> {
        let key = match property.to_ascii_lowercase().as_str() {
            "name" => PropertyKey::Name,
            "type" | "ext" => PropertyKey::Type,
            "size" => PropertyKey::Size,
            "date" | "datemodified" | "modified" => PropertyKey::DateModified,
            _ => {
                return Err(error(
                    property_span.start,
                    property_span.end,
                    "未知的搜尋屬性",
                    "可用屬性為 name、type、size、date",
                ));
            }
        };
        let comparison = match self.peek().map(|token| &token.kind) {
            Some(TokenKind::Comparison(value)) => {
                let value = *value;
                self.cursor += 1;
                value
            }
            _ => Comparison::Equal,
        };
        let Some(value_token) = self.next().cloned() else {
            return Err(error(
                self.input_len,
                self.input_len,
                "屬性缺少值",
                "在冒號後加入值",
            ));
        };
        let (raw, phrase) = match value_token.kind {
            TokenKind::Word(value) => (value, false),
            TokenKind::Phrase(value) => (value, true),
            _ => {
                return Err(error(
                    value_token.span.start,
                    value_token.span.end,
                    "屬性值無效",
                    "使用文字、數值或加引號的片語",
                ));
            }
        };
        let value = match key {
            PropertyKey::Name | PropertyKey::Type => {
                if comparison != Comparison::Equal {
                    return Err(error(
                        value_token.span.start,
                        value_token.span.end,
                        "文字屬性不支援大小比較",
                        "移除 >、< 或改用 =",
                    ));
                }
                let _ = phrase;
                Value::Text(raw)
            }
            PropertyKey::Size => Value::Size(parse_size(&raw).ok_or_else(|| {
                error(
                    value_token.span.start,
                    value_token.span.end,
                    "檔案大小格式無效",
                    "例如 size:>=10KB、size:<2MB",
                )
            })?),
            PropertyKey::DateModified => Value::Date(DateValue::parse(&raw).ok_or_else(|| {
                error(
                    value_token.span.start,
                    value_token.span.end,
                    "日期格式或日期值無效",
                    "使用 YYYY-MM-DD，例如 date:>=2026-01-01",
                )
            })?),
        };
        Ok(Expr::Filter {
            key,
            comparison,
            value,
        })
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.cursor)
    }
    fn next(&mut self) -> Option<&Token> {
        let token = self.tokens.get(self.cursor);
        self.cursor += usize::from(token.is_some());
        token
    }
    fn consume(&mut self, predicate: impl FnOnce(&TokenKind) -> bool) -> Option<&Token> {
        if self.peek().is_some_and(|token| predicate(&token.kind)) {
            self.next()
        } else {
            None
        }
    }
    fn missing_operand(&self, operator: &str) -> ParseError {
        let span = self.tokens.get(self.cursor.saturating_sub(1)).map_or(
            Span {
                start: self.input_len,
                end: self.input_len,
            },
            |token| token.span,
        );
        error(
            span.start,
            span.end,
            format!("{operator} 缺少運算元"),
            "在運算子後加入查詢條件",
        )
    }
}

fn has_unescaped_wildcard(value: &str) -> bool {
    let mut escaped = false;
    for character in value.chars() {
        if escaped {
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if matches!(character, '*' | '?') {
            return true;
        }
    }
    false
}

fn starts_expression(kind: &TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Word(_) | TokenKind::Phrase(_) | TokenKind::LeftParen | TokenKind::Not
    )
}

fn parse_size(raw: &str) -> Option<SizeValue> {
    let split = raw
        .find(|ch: char| !ch.is_ascii_digit())
        .unwrap_or(raw.len());
    let number: u64 = raw[..split].parse().ok()?;
    let multiplier = match raw[split..].to_ascii_uppercase().as_str() {
        "" | "B" => 1,
        "KB" => 1_024,
        "MB" => 1_048_576,
        "GB" => 1_073_741_824,
        _ => return None,
    };
    number.checked_mul(multiplier).map(SizeValue)
}

fn error(
    start: usize,
    end: usize,
    message: impl Into<String>,
    suggestion: impl Into<String>,
) -> ParseError {
    ParseError {
        span: Span { start, end },
        message: message.into(),
        suggestion: suggestion.into(),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QueryParameter {
    Text(String),
    Unsigned(u64),
    Date(DateValue),
}

/// Query-helper input uses placeholders and separately owned values until final escaping.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundQuery {
    pub template: String,
    pub parameters: Vec<QueryParameter>,
}

pub fn bind_query(expression: &Expr) -> BoundQuery {
    fn bind(expression: &Expr, output: &mut String, parameters: &mut Vec<QueryParameter>) {
        match expression {
            Expr::Text { value, .. } => push_parameter(
                output,
                parameters,
                "System.FileName",
                Comparison::Equal,
                QueryParameter::Text(value.clone()),
            ),
            Expr::Filter {
                key,
                comparison,
                value,
            } => {
                let property = match key {
                    PropertyKey::Name => "System.FileName",
                    PropertyKey::Type => "System.FileExtension",
                    PropertyKey::Size => "System.Size",
                    PropertyKey::DateModified => "System.DateModified",
                };
                let parameter = match value {
                    Value::Text(v) => QueryParameter::Text(v.clone()),
                    Value::Size(v) => QueryParameter::Unsigned(v.0),
                    Value::Date(v) => QueryParameter::Date(*v),
                };
                push_parameter(output, parameters, property, *comparison, parameter);
            }
            Expr::Not(inner) => {
                output.push_str("NOT (");
                bind(inner, output, parameters);
                output.push(')');
            }
            Expr::And(left, right) | Expr::Or(left, right) => {
                output.push('(');
                bind(left, output, parameters);
                output.push_str(if matches!(expression, Expr::And(..)) {
                    ") AND ("
                } else {
                    ") OR ("
                });
                bind(right, output, parameters);
                output.push(')');
            }
        }
    }
    fn push_parameter(
        output: &mut String,
        parameters: &mut Vec<QueryParameter>,
        property: &str,
        comparison: Comparison,
        value: QueryParameter,
    ) {
        use std::fmt::Write as _;
        let operator = match comparison {
            Comparison::Equal => "=",
            Comparison::Greater => ">",
            Comparison::GreaterOrEqual => ">=",
            Comparison::Less => "<",
            Comparison::LessOrEqual => "<=",
        };
        let index = parameters.len();
        parameters.push(value);
        let _ = write!(output, "{property}{operator}{{{index}}}");
    }
    let mut template = String::new();
    let mut parameters = Vec::new();
    bind(expression, &mut template, &mut parameters);
    BoundQuery {
        template,
        parameters,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_text_phrase_escape_filters_boolean_precedence_and_unicode() {
        let expression = parse(r#"報告 "quarter \"four\"" name:專案 type:txt size:>=10KB date:<2027-01-01 OR NOT (草稿 AND old)"#).unwrap();
        assert!(matches!(expression, Expr::Or(_, _)));
        let bound = bind_query(&expression);
        assert_eq!(bound.parameters.len(), 8);
        assert!(bound.template.contains("System.Size>={"));
    }

    #[test]
    fn implicit_and_binds_more_tightly_than_or() {
        let expression = parse("alpha OR beta gamma").unwrap();
        assert!(matches!(expression, Expr::Or(_, right) if matches!(*right, Expr::And(_, _))));
    }

    #[test]
    fn validates_calendar_and_size_values() {
        assert!(parse("date:2024-02-29 size:1GB").is_ok());
        for input in [
            "date:2023-02-29",
            "date:2026-13-01",
            "size:2XB",
            "size:-1KB",
        ] {
            let failure = parse(input).unwrap_err();
            assert!(!failure.suggestion.is_empty());
            assert!(failure.span.end > failure.span.start);
        }
    }

    #[test]
    fn invalid_syntax_has_precise_actionable_span() {
        for input in [
            r#"name:"unterminated"#,
            "owner:me",
            "alpha AND",
            "(alpha OR beta",
            "alpha )",
        ] {
            let failure = parse(input).unwrap_err();
            assert!(failure.span.start <= input.len());
            assert!(failure.span.end <= input.len());
            assert!(!failure.message.is_empty() && !failure.suggestion.is_empty());
        }
    }

    #[test]
    fn binding_never_copies_user_text_into_template() {
        let expression = parse(r#"name:"x' OR 1=1 --""#).unwrap();
        let bound = bind_query(&expression);
        assert!(!bound.template.contains("x'"));
        assert_eq!(
            bound.parameters,
            vec![QueryParameter::Text("x' OR 1=1 --".to_owned())]
        );
    }

    #[test]
    fn lexical_failure_does_not_poison_the_next_query() {
        assert!(parse(r#"name:"unterminated"#).is_err());
        assert!(parse("name:report type:txt").is_ok());
    }

    #[test]
    fn unqualified_text_records_only_unescaped_wildcards() {
        assert!(matches!(
            parse("*.rs").unwrap(),
            Expr::Text { glob: true, .. }
        ));
        assert!(matches!(
            parse(r"literal\*star").unwrap(),
            Expr::Text { glob: false, .. }
        ));
        assert!(matches!(
            parse(r"literal\*star?.rs").unwrap(),
            Expr::Text { glob: true, .. }
        ));
        assert!(matches!(
            parse(r#""*.rs""#).unwrap(),
            Expr::Text {
                phrase: true,
                glob: false,
                ..
            }
        ));
    }
}
