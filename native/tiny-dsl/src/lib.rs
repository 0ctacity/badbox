//! Parser and syntax model for Badbox's tiny rule language.

use std::{error::Error as StdError, fmt};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Document {
    pub version: u32,
    pub rules: Vec<Rule>,
    pub tests: Vec<RuleTest>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    pub id: String,
    pub language: String,
    pub summary: String,
    pub parameters: Vec<Parameter>,
    pub selection: Selection,
    pub group: Group,
    pub where_clauses: Vec<WhereClause>,
    pub condition: Condition,
    pub report: Report,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parameter {
    pub name: String,
    pub value: ComparisonValue,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComparisonValue {
    Integer(u32),
    Boolean(bool),
    String(String),
    Parameter(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Selection {
    Code {
        captures: Vec<Capture>,
        template: String,
    },
    Node(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Capture {
    pub name: String,
    pub multiple: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Group {
    Callable,
    Node(Vec<String>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Condition {
    pub aggregate: Aggregate,
    pub value: ComparisonValue,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Aggregate {
    Count,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WhereClause {
    Text {
        capture: String,
        operator: TextOperator,
        values: Vec<String>,
    },
    Relation {
        target: RelationTarget,
        relation: Relation,
        require_all: bool,
        selections: Vec<Selection>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextOperator {
    Equal,
    NotEqual,
    In,
    NotIn,
    Matches,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelationTarget {
    Match,
    Group,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Relation {
    Has,
    Lacks,
    Inside,
    Follows,
    Precedes,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    pub severity: Severity,
    pub message: String,
    pub evidence: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleTest {
    pub rule_id: String,
    pub name: String,
    pub input: String,
    pub expect: TestExpectation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestExpectation {
    pub findings: u32,
    pub count: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    message: String,
    line: usize,
    column: usize,
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} at {}:{}",
            self.message, self.line, self.column
        )
    }
}

impl StdError for ParseError {}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Token {
    kind: TokenKind,
    line: usize,
    column: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TokenKind {
    Ident(String),
    Number(u32),
    String(String),
    Code(String),
    LeftBrace,
    RightBrace,
    LeftParen,
    RightParen,
    LeftBracket,
    RightBracket,
    Comma,
    Equal,
    EqualEqual,
    NotEqual,
    Greater,
    Ellipsis,
}

pub fn parse(source: &str) -> Result<Document, ParseError> {
    Parser::new(lex(source)?).parse_document()
}

struct Parser {
    tokens: Vec<Token>,
    cursor: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, cursor: 0 }
    }

    fn parse_document(mut self) -> Result<Document, ParseError> {
        self.keyword("badbox")?;
        let version = self.number()?;
        if version != 1 {
            return Err(self.error(format!(
                "unsupported Badbox syntax version {version}; expected 1"
            )));
        }
        let mut rules = Vec::new();
        let mut tests = Vec::new();
        while !self.done() {
            if self.at_keyword("rule") {
                rules.push(self.parse_rule()?);
            } else if self.at_keyword("test") {
                tests.push(self.parse_test()?);
            } else {
                return Err(self.error("expected rule or test"));
            }
        }
        Ok(Document {
            version,
            rules,
            tests,
        })
    }

    fn parse_rule(&mut self) -> Result<Rule, ParseError> {
        self.keyword("rule")?;
        let id = self.ident()?;
        if !valid_rule_id(&id) {
            return Err(self.error("invalid rule ID: expected lowercase namespace/name"));
        }
        self.keyword("for")?;
        let language = self.ident()?;
        self.symbol(TokenKind::LeftBrace)?;

        let mut summary = None;
        let mut parameters = Vec::new();
        let mut selection = None;
        let mut group = None;
        let mut where_clauses = Vec::new();
        let mut condition = None;
        let mut report = None;
        while !self.take_symbol(TokenKind::RightBrace) {
            let field = self.peek_ident()?.to_owned();
            match field.as_str() {
                "summary" => {
                    self.keyword("summary")?;
                    set_once(&mut summary, self.string()?, "summary", self)?;
                }
                "param" => parameters.push(self.parse_parameter()?),
                "find" => {
                    self.keyword("find")?;
                    let value = self.parse_selection()?;
                    set_once(&mut selection, value, "find", self)?;
                }
                "group" => {
                    let value = self.parse_group()?;
                    set_once(&mut group, value, "group", self)?;
                }
                "where" => where_clauses.push(self.parse_where()?),
                "when" => {
                    let value = self.parse_condition()?;
                    set_once(&mut condition, value, "when", self)?;
                }
                "report" => {
                    let value = self.parse_report()?;
                    set_once(&mut report, value, "report", self)?;
                }
                _ => return Err(self.error(format!("unknown rule field {field}"))),
            }
        }

        let summary = required(summary, "summary", self)?;
        if summary.trim().is_empty() {
            return Err(self.error("summary must not be empty"));
        }
        let selection = required(selection, "find", self)?;
        let group = required(group, "group", self)?;
        let condition = required(condition, "when", self)?;
        let report = required(report, "report", self)?;
        let mut parameter_names = std::collections::BTreeSet::new();
        for parameter in &parameters {
            if !parameter_names.insert(&parameter.name) {
                return Err(self.error(format!("duplicate parameter {}", parameter.name)));
            }
        }
        if let ComparisonValue::Parameter(name) = &condition.value {
            let Some(parameter) = parameters.iter().find(|parameter| &parameter.name == name)
            else {
                return Err(self.error(format!("unknown parameter {name}")));
            };
            if !matches!(parameter.value, ComparisonValue::Integer(_)) {
                return Err(self.error(format!(
                    "count threshold parameter {name} must be an integer"
                )));
            }
        }
        let primary_captures: &[Capture] = match &selection {
            Selection::Code { captures, .. } => captures.as_slice(),
            Selection::Node(_) => &[],
        };
        for clause in &where_clauses {
            if let WhereClause::Text { capture, .. } = clause
                && !primary_captures
                    .iter()
                    .any(|declared| declared.name == *capture)
            {
                return Err(self.error(format!("unknown primary capture {capture}")));
            }
        }
        Ok(Rule {
            id,
            language,
            summary,
            parameters,
            selection,
            group,
            where_clauses,
            condition,
            report,
        })
    }

    fn parse_parameter(&mut self) -> Result<Parameter, ParseError> {
        self.keyword("param")?;
        let name = self.ident()?;
        self.symbol(TokenKind::Equal)?;
        let value = self.scalar()?;
        if matches!(value, ComparisonValue::Parameter(_)) {
            return Err(self.error("parameter defaults must be scalar values"));
        }
        Ok(Parameter { name, value })
    }

    fn parse_selection(&mut self) -> Result<Selection, ParseError> {
        if self.take_keyword("node") {
            return Ok(Selection::Node(self.ident()?));
        }
        self.keyword("code")?;
        self.symbol(TokenKind::LeftParen)?;
        let mut captures = Vec::new();
        if !self.take_symbol(TokenKind::RightParen) {
            loop {
                let name = self.ident()?;
                let multiple = self.take_symbol(TokenKind::Ellipsis);
                if captures
                    .iter()
                    .any(|capture: &Capture| capture.name == name)
                {
                    return Err(self.error(format!("duplicate capture {name}")));
                }
                captures.push(Capture { name, multiple });
                if self.take_symbol(TokenKind::RightParen) {
                    break;
                }
                self.symbol(TokenKind::Comma)?;
            }
        }
        let template = self.code()?;
        if template.trim().is_empty() {
            return Err(self.error("code pattern must not be empty"));
        }
        Ok(Selection::Code { captures, template })
    }

    fn parse_group(&mut self) -> Result<Group, ParseError> {
        self.keyword("group")?;
        self.keyword("by")?;
        self.keyword("nearest")?;
        if self.take_keyword("callable") {
            return Ok(Group::Callable);
        }
        self.keyword("node")?;
        self.symbol(TokenKind::LeftParen)?;
        let mut kinds = Vec::new();
        loop {
            kinds.push(self.ident()?);
            if self.take_symbol(TokenKind::RightParen) {
                break;
            }
            self.symbol(TokenKind::Comma)?;
        }
        Ok(Group::Node(kinds))
    }

    fn parse_where(&mut self) -> Result<WhereClause, ParseError> {
        self.keyword("where")?;
        if self.take_keyword("text") {
            self.symbol(TokenKind::LeftParen)?;
            let capture = self.ident()?;
            self.symbol(TokenKind::RightParen)?;
            let operator = if self.take_symbol(TokenKind::EqualEqual) {
                TextOperator::Equal
            } else if self.take_symbol(TokenKind::NotEqual) {
                TextOperator::NotEqual
            } else if self.take_keyword("not") {
                self.keyword("in")?;
                TextOperator::NotIn
            } else if self.take_keyword("in") {
                TextOperator::In
            } else if self.take_keyword("matches") {
                TextOperator::Matches
            } else {
                return Err(self.error("expected ==, !=, in, not in, or matches"));
            };
            let values = if matches!(operator, TextOperator::In | TextOperator::NotIn) {
                self.string_list()?
            } else {
                vec![self.string()?]
            };
            return Ok(WhereClause::Text {
                capture,
                operator,
                values,
            });
        }

        let target = if self.take_keyword("match") {
            RelationTarget::Match
        } else if self.take_keyword("group") {
            RelationTarget::Group
        } else {
            return Err(self.error("expected text, match, or group after where"));
        };
        let relation = match self.ident()?.as_str() {
            "has" => Relation::Has,
            "lacks" => Relation::Lacks,
            "inside" => Relation::Inside,
            "follows" => Relation::Follows,
            "precedes" => Relation::Precedes,
            other => return Err(self.error(format!("unknown relation {other}"))),
        };
        let require_all = if self.take_keyword("all") {
            true
        } else {
            self.keyword("any")?;
            false
        };
        self.symbol(TokenKind::LeftBrace)?;
        let mut selections = Vec::new();
        while !self.take_symbol(TokenKind::RightBrace) {
            selections.push(self.parse_selection()?);
        }
        if selections.is_empty() {
            return Err(self.error("relational condition must contain at least one selector"));
        }
        Ok(WhereClause::Relation {
            target,
            relation,
            require_all,
            selections,
        })
    }

    fn parse_condition(&mut self) -> Result<Condition, ParseError> {
        self.keyword("when")?;
        self.keyword("count")?;
        self.symbol(TokenKind::Greater)?;
        let value = match self.peek_kind() {
            Some(TokenKind::Number(_)) => ComparisonValue::Integer(self.number()?),
            Some(TokenKind::Ident(_)) => ComparisonValue::Parameter(self.ident()?),
            _ => return Err(self.error("count threshold must be an integer or parameter")),
        };
        Ok(Condition {
            aggregate: Aggregate::Count,
            value,
        })
    }

    fn parse_report(&mut self) -> Result<Report, ParseError> {
        self.keyword("report")?;
        self.symbol(TokenKind::LeftBrace)?;
        let mut severity = None;
        let mut message = None;
        let mut evidence = None;
        while !self.take_symbol(TokenKind::RightBrace) {
            let field = self.ident()?;
            match field.as_str() {
                "severity" => {
                    let value = match self.ident()?.as_str() {
                        "info" => Severity::Info,
                        "warning" => Severity::Warning,
                        "error" => Severity::Error,
                        other => return Err(self.error(format!("unknown severity {other}"))),
                    };
                    set_once(&mut severity, value, "severity", self)?;
                }
                "message" => set_once(&mut message, self.string()?, "message", self)?,
                "evidence" => set_once(&mut evidence, self.string()?, "evidence", self)?,
                _ => return Err(self.error(format!("unknown report field {field}"))),
            }
        }
        Ok(Report {
            severity: required(severity, "severity", self)?,
            message: nonempty(required(message, "message", self)?, "message", self)?,
            evidence: nonempty(required(evidence, "evidence", self)?, "evidence", self)?,
        })
    }

    fn parse_test(&mut self) -> Result<RuleTest, ParseError> {
        self.keyword("test")?;
        let rule_id = self.ident()?;
        if !valid_rule_id(&rule_id) {
            return Err(self.error("invalid test rule ID: expected lowercase namespace/name"));
        }
        let name = self.string()?;
        self.symbol(TokenKind::LeftBrace)?;
        self.keyword("input")?;
        let input = self.string()?;
        self.keyword("expect")?;
        let expect = if self.take_symbol(TokenKind::LeftBrace) {
            let mut findings = None;
            let mut count = None;
            while !self.take_symbol(TokenKind::RightBrace) {
                let field = self.ident()?;
                match field.as_str() {
                    "findings" => set_once(&mut findings, self.number()?, "findings", self)?,
                    "count" => set_once(&mut count, self.number()?, "count", self)?,
                    _ => return Err(self.error(format!("unknown expectation field {field}"))),
                }
            }
            TestExpectation {
                findings: required(findings, "findings", self)?,
                count,
            }
        } else {
            self.keyword("findings")?;
            TestExpectation {
                findings: self.number()?,
                count: None,
            }
        };
        self.symbol(TokenKind::RightBrace)?;
        Ok(RuleTest {
            rule_id,
            name,
            input,
            expect,
        })
    }

    fn string_list(&mut self) -> Result<Vec<String>, ParseError> {
        self.symbol(TokenKind::LeftBracket)?;
        let mut values = Vec::new();
        if self.take_symbol(TokenKind::RightBracket) {
            return Err(self.error("text value list must not be empty"));
        }
        loop {
            values.push(self.string()?);
            if self.take_symbol(TokenKind::RightBracket) {
                break;
            }
            self.symbol(TokenKind::Comma)?;
        }
        Ok(values)
    }

    fn scalar(&mut self) -> Result<ComparisonValue, ParseError> {
        match self.peek_kind() {
            Some(TokenKind::Number(_)) => Ok(ComparisonValue::Integer(self.number()?)),
            Some(TokenKind::String(_)) => Ok(ComparisonValue::String(self.string()?)),
            Some(TokenKind::Ident(value)) if value == "true" || value == "false" => {
                Ok(ComparisonValue::Boolean(self.ident()? == "true"))
            }
            _ => Err(self.error("expected an integer, boolean, or string")),
        }
    }

    fn keyword(&mut self, expected: &str) -> Result<(), ParseError> {
        let actual = self.ident()?;
        if actual == expected {
            Ok(())
        } else {
            Err(self.error(format!("expected {expected}, found {actual}")))
        }
    }

    fn take_keyword(&mut self, expected: &str) -> bool {
        if self.at_keyword(expected) {
            self.cursor += 1;
            true
        } else {
            false
        }
    }

    fn at_keyword(&self, expected: &str) -> bool {
        matches!(self.peek_kind(), Some(TokenKind::Ident(value)) if value == expected)
    }

    fn ident(&mut self) -> Result<String, ParseError> {
        match self.next_kind() {
            Some(TokenKind::Ident(value)) => Ok(value),
            _ => Err(self.error("expected identifier")),
        }
    }

    fn peek_ident(&self) -> Result<&str, ParseError> {
        match self.peek_kind() {
            Some(TokenKind::Ident(value)) => Ok(value),
            _ => Err(self.error("expected identifier")),
        }
    }

    fn number(&mut self) -> Result<u32, ParseError> {
        match self.next_kind() {
            Some(TokenKind::Number(value)) => Ok(value),
            _ => Err(self.error("expected unsigned integer")),
        }
    }

    fn string(&mut self) -> Result<String, ParseError> {
        match self.next_kind() {
            Some(TokenKind::String(value)) => Ok(value),
            _ => Err(self.error("expected string")),
        }
    }

    fn code(&mut self) -> Result<String, ParseError> {
        match self.next_kind() {
            Some(TokenKind::Code(value)) => Ok(value),
            _ => Err(self.error("expected backtick-delimited code pattern")),
        }
    }

    fn symbol(&mut self, expected: TokenKind) -> Result<(), ParseError> {
        if self.take_symbol(expected.clone()) {
            Ok(())
        } else {
            Err(self.error(format!("expected {}", symbol_name(&expected))))
        }
    }

    fn take_symbol(&mut self, expected: TokenKind) -> bool {
        if self.peek_kind() == Some(&expected) {
            self.cursor += 1;
            true
        } else {
            false
        }
    }

    fn peek_kind(&self) -> Option<&TokenKind> {
        self.tokens.get(self.cursor).map(|token| &token.kind)
    }

    fn next_kind(&mut self) -> Option<TokenKind> {
        let kind = self.tokens.get(self.cursor)?.kind.clone();
        self.cursor += 1;
        Some(kind)
    }

    fn done(&self) -> bool {
        self.cursor == self.tokens.len()
    }

    fn error(&self, message: impl Into<String>) -> ParseError {
        let (line, column) = self
            .tokens
            .get(self.cursor)
            .map(|token| (token.line, token.column))
            .or_else(|| {
                self.tokens
                    .last()
                    .map(|token| (token.line, token.column + 1))
            })
            .unwrap_or((1, 1));
        ParseError {
            message: message.into(),
            line,
            column,
        }
    }
}

fn required<T>(value: Option<T>, field: &str, parser: &Parser) -> Result<T, ParseError> {
    value.ok_or_else(|| parser.error(format!("missing {field}")))
}

fn set_once<T>(
    slot: &mut Option<T>,
    value: T,
    field: &str,
    parser: &Parser,
) -> Result<(), ParseError> {
    if slot.replace(value).is_some() {
        Err(parser.error(format!("duplicate {field}")))
    } else {
        Ok(())
    }
}

fn nonempty(value: String, field: &str, parser: &Parser) -> Result<String, ParseError> {
    if value.trim().is_empty() {
        Err(parser.error(format!("{field} must not be empty")))
    } else {
        Ok(value)
    }
}

fn valid_rule_id(id: &str) -> bool {
    let mut parts = id.split('/');
    let valid_part = |part: &str| {
        part.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
            && part
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    };
    matches!((parts.next(), parts.next(), parts.next()), (Some(namespace), Some(name), None) if valid_part(namespace) && valid_part(name))
}

fn symbol_name(kind: &TokenKind) -> &'static str {
    match kind {
        TokenKind::LeftBrace => "{",
        TokenKind::RightBrace => "}",
        TokenKind::LeftParen => "(",
        TokenKind::RightParen => ")",
        TokenKind::LeftBracket => "[",
        TokenKind::RightBracket => "]",
        TokenKind::Comma => ",",
        TokenKind::Equal => "=",
        TokenKind::EqualEqual => "==",
        TokenKind::NotEqual => "!=",
        TokenKind::Greater => ">",
        TokenKind::Ellipsis => "...",
        _ => "symbol",
    }
}

fn lex(source: &str) -> Result<Vec<Token>, ParseError> {
    let mut tokens = Vec::new();
    let mut chars = source.char_indices().peekable();
    let mut line = 1;
    let mut column = 1;
    while let Some((start, character)) = chars.next() {
        let token_line = line;
        let token_column = column;
        if character == '\n' {
            line += 1;
            column = 1;
            continue;
        }
        if character.is_whitespace() {
            column += 1;
            continue;
        }
        if character == '#' {
            column += 1;
            while let Some((_, next)) = chars.next() {
                if next == '\n' {
                    line += 1;
                    column = 1;
                    break;
                }
                column += 1;
            }
            continue;
        }
        let simple = match character {
            '{' => Some(TokenKind::LeftBrace),
            '}' => Some(TokenKind::RightBrace),
            '(' => Some(TokenKind::LeftParen),
            ')' => Some(TokenKind::RightParen),
            '[' => Some(TokenKind::LeftBracket),
            ']' => Some(TokenKind::RightBracket),
            ',' => Some(TokenKind::Comma),
            '>' => Some(TokenKind::Greater),
            _ => None,
        };
        if let Some(kind) = simple {
            tokens.push(Token {
                kind,
                line: token_line,
                column: token_column,
            });
            column += 1;
            continue;
        }
        if character == '=' {
            let kind = if chars.peek().is_some_and(|(_, next)| *next == '=') {
                chars.next();
                column += 2;
                TokenKind::EqualEqual
            } else {
                column += 1;
                TokenKind::Equal
            };
            tokens.push(Token {
                kind,
                line: token_line,
                column: token_column,
            });
            continue;
        }
        if character == '!' && chars.peek().is_some_and(|(_, next)| *next == '=') {
            chars.next();
            column += 2;
            tokens.push(Token {
                kind: TokenKind::NotEqual,
                line: token_line,
                column: token_column,
            });
            continue;
        }
        if character == '.' {
            let mut count = 1;
            while count < 3 && chars.peek().is_some_and(|(_, next)| *next == '.') {
                chars.next();
                count += 1;
            }
            if count != 3 {
                return Err(ParseError {
                    message: "expected ...".into(),
                    line: token_line,
                    column: token_column,
                });
            }
            column += 3;
            tokens.push(Token {
                kind: TokenKind::Ellipsis,
                line: token_line,
                column: token_column,
            });
            continue;
        }
        if character == '"' || character == '`' {
            let delimiter = character;
            let mut value = String::new();
            let mut closed = false;
            while let Some((_, next)) = chars.next() {
                column += 1;
                if next == delimiter {
                    closed = true;
                    break;
                }
                if next == '\n' {
                    line += 1;
                    column = 1;
                }
                if next == '\\' {
                    let Some((_, escaped)) = chars.next() else {
                        break;
                    };
                    column += 1;
                    match (delimiter, escaped) {
                        ('"', 'n') => value.push('\n'),
                        ('"', 't') => value.push('\t'),
                        ('"', '"') => value.push('"'),
                        ('"', '\\') => value.push('\\'),
                        ('`', '`') => value.push('`'),
                        _ => {
                            value.push('\\');
                            value.push(escaped);
                        }
                    }
                } else {
                    value.push(next);
                }
            }
            if !closed {
                return Err(ParseError {
                    message: format!(
                        "unterminated {}",
                        if delimiter == '"' {
                            "string"
                        } else {
                            "code pattern"
                        }
                    ),
                    line: token_line,
                    column: token_column,
                });
            }
            tokens.push(Token {
                kind: if delimiter == '"' {
                    TokenKind::String(value)
                } else {
                    TokenKind::Code(value)
                },
                line: token_line,
                column: token_column,
            });
            column += 1;
            continue;
        }
        if character.is_ascii_digit() {
            let mut end = start + character.len_utf8();
            while let Some(&(index, next)) = chars.peek() {
                if !next.is_ascii_digit() {
                    break;
                }
                chars.next();
                end = index + next.len_utf8();
            }
            let value = source[start..end].parse::<u32>().map_err(|_| ParseError {
                message: "integer exceeds u32".into(),
                line: token_line,
                column: token_column,
            })?;
            column += end - start;
            tokens.push(Token {
                kind: TokenKind::Number(value),
                line: token_line,
                column: token_column,
            });
            continue;
        }
        if is_ident_start(character) {
            let mut end = start + character.len_utf8();
            while let Some(&(index, next)) = chars.peek() {
                if !is_ident_continue(next) {
                    break;
                }
                chars.next();
                end = index + next.len_utf8();
            }
            let value = source[start..end].to_owned();
            column += value.chars().count();
            tokens.push(Token {
                kind: TokenKind::Ident(value),
                line: token_line,
                column: token_column,
            });
            continue;
        }
        return Err(ParseError {
            message: format!("unexpected character {character:?}"),
            line: token_line,
            column: token_column,
        });
    }
    Ok(tokens)
}

fn is_ident_start(character: char) -> bool {
    character.is_ascii_alphabetic() || character == '_'
}

fn is_ident_continue(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '/')
}
