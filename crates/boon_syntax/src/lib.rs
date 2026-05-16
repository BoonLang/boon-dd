use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceFile {
    pub path: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedModule {
    pub source: SourceFile,
    pub definitions: Vec<Definition>,
    pub diagnostics: Vec<SyntaxDiagnostic>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Definition {
    pub name: String,
    pub parameters: Vec<String>,
    pub is_function: bool,
    pub expression: Expr,
    pub span: SourceSpan,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyntaxDiagnostic {
    pub message: String,
    pub span: SourceSpan,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Expr {
    Missing,
    Path(String),
    Number(String),
    Source,
    SourceAt {
        target: Box<Expr>,
    },
    Link {
        target: Option<Box<Expr>>,
    },
    Skip,
    Tag(String),
    Text(String),
    Record(Vec<RecordField>),
    List(Vec<Expr>),
    Block(Vec<Expr>),
    Latest(Vec<Expr>),
    Call {
        callee: String,
        args: Vec<CallArg>,
    },
    Constructor {
        callee: String,
        fields: Vec<RecordField>,
    },
    FieldAccess {
        base: Box<Expr>,
        field: String,
    },
    Pipe {
        input: Box<Expr>,
        stage: Box<Expr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Then {
        body: Vec<Expr>,
    },
    Hold {
        binder: String,
        body: Vec<Expr>,
    },
    Match {
        kind: MatchKind,
        arms: Vec<MatchArm>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordField {
    pub name: String,
    pub value: Expr,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CallArg {
    Positional(Expr),
    Named { name: String, value: Expr },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinaryOp {
    Add,
    Subtract,
    Equal,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MatchKind {
    When,
    While,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchArm {
    pub pattern: String,
    pub value: Expr,
}

pub fn parse_source(path: impl Into<String>, text: impl Into<String>) -> ParsedModule {
    let path = path.into();
    let text = text.into();
    let mut diagnostics = Vec::new();
    let definitions = parse_definitions(&text, &mut diagnostics);
    ParsedModule {
        source: SourceFile { path, text },
        definitions,
        diagnostics,
    }
}

fn parse_definitions(text: &str, diagnostics: &mut Vec<SyntaxDiagnostic>) -> Vec<Definition> {
    let lines = text.lines().collect::<Vec<_>>();
    let mut starts = Vec::new();
    let mut offset = 0_usize;
    for line in &lines {
        let trimmed = line.trim();
        if !trimmed.is_empty()
            && leading_spaces(line) == 0
            && (value_definition_label(trimmed).is_some()
                || function_definition_header(trimmed).is_some())
        {
            starts.push(offset);
        }
        offset += line.len() + 1;
    }

    let mut definitions = Vec::new();
    for (index, start) in starts.iter().copied().enumerate() {
        let end = starts.get(index + 1).copied().unwrap_or(text.len());
        let block = &text[start..end];
        let Some(header) = definition_header(block, start, diagnostics) else {
            continue;
        };
        let expr_text = header.expression.trim();
        let expr_start = header.expression_offset + header.expression.len()
            - header.expression.trim_start().len();
        let expression = parse_expr_text(expr_text, expr_start, diagnostics);
        definitions.push(Definition {
            name: header.name,
            parameters: header.parameters,
            is_function: header.is_function,
            expression,
            span: SourceSpan { start, end },
        });
    }
    definitions
}

fn parse_expr_text(
    text: &str,
    base_offset: usize,
    diagnostics: &mut Vec<SyntaxDiagnostic>,
) -> Expr {
    let tokens = lex(text, base_offset, diagnostics);
    let mut parser = Parser {
        source: text,
        base_offset,
        tokens,
        index: 0,
        diagnostics,
    };
    parser.parse_expr_until(&[])
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Token {
    kind: TokenKind,
    start: usize,
    end: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TokenKind {
    Word(String),
    Number(String),
    Colon,
    Comma,
    Plus,
    Minus,
    Dot,
    Equals,
    DoubleEquals,
    Pipe,
    FatArrow,
    LBracket,
    RBracket,
    LParen,
    RParen,
    LBrace,
    RBrace,
}

fn lex(text: &str, base_offset: usize, diagnostics: &mut Vec<SyntaxDiagnostic>) -> Vec<Token> {
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0_usize;
    while index < bytes.len() {
        let ch = text[index..].chars().next().unwrap();
        if ch.is_whitespace() {
            index += ch.len_utf8();
            continue;
        }
        if ch == '#' {
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            continue;
        }
        if text[index..].starts_with("--") {
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            continue;
        }
        let start = index;
        let kind = match ch {
            ':' => {
                index += 1;
                TokenKind::Colon
            }
            ',' => {
                index += 1;
                TokenKind::Comma
            }
            '+' => {
                index += 1;
                TokenKind::Plus
            }
            '-' => {
                index += 1;
                TokenKind::Minus
            }
            '.' => {
                index += 1;
                TokenKind::Dot
            }
            '[' => {
                index += 1;
                TokenKind::LBracket
            }
            ']' => {
                index += 1;
                TokenKind::RBracket
            }
            '(' => {
                index += 1;
                TokenKind::LParen
            }
            ')' => {
                index += 1;
                TokenKind::RParen
            }
            '{' => {
                index += 1;
                TokenKind::LBrace
            }
            '}' => {
                index += 1;
                TokenKind::RBrace
            }
            '|' if text[index..].starts_with("|>") => {
                index += 2;
                TokenKind::Pipe
            }
            '=' if text[index..].starts_with("=>") => {
                index += 2;
                TokenKind::FatArrow
            }
            '=' if text[index..].starts_with("==") => {
                index += 2;
                TokenKind::DoubleEquals
            }
            '=' => {
                index += 1;
                TokenKind::Equals
            }
            value if value.is_ascii_digit() => {
                index += value.len_utf8();
                while index < bytes.len() {
                    let next = text[index..].chars().next().unwrap();
                    if next.is_ascii_digit() || next == '.' {
                        index += next.len_utf8();
                    } else {
                        break;
                    }
                }
                TokenKind::Number(text[start..index].to_owned())
            }
            value if is_word_start(value) => {
                index += value.len_utf8();
                while index < bytes.len() {
                    let next = text[index..].chars().next().unwrap();
                    if is_word_continue(next) {
                        index += next.len_utf8();
                    } else {
                        break;
                    }
                }
                TokenKind::Word(text[start..index].to_owned())
            }
            _ => {
                index += ch.len_utf8();
                diagnostics.push(SyntaxDiagnostic {
                    message: format!("unexpected character `{ch}`"),
                    span: SourceSpan {
                        start: base_offset + start,
                        end: base_offset + index,
                    },
                });
                continue;
            }
        };
        tokens.push(Token {
            kind,
            start: base_offset + start,
            end: base_offset + index,
        });
    }
    tokens
}

fn is_word_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

fn is_word_continue(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | '/' | '.')
}

struct Parser<'a, 'd> {
    source: &'a str,
    base_offset: usize,
    tokens: Vec<Token>,
    index: usize,
    diagnostics: &'d mut Vec<SyntaxDiagnostic>,
}

impl Parser<'_, '_> {
    fn parse_expr_until(&mut self, stop: &[TokenStop]) -> Expr {
        let mut expr = self.parse_equality(stop);
        while !self.is_at_end() && !self.at_stop(stop) && self.match_kind(&TokenKind::Pipe) {
            let stage = self.parse_equality(stop);
            expr = Expr::Pipe {
                input: Box::new(expr),
                stage: Box::new(stage),
            };
        }
        expr
    }

    fn parse_equality(&mut self, stop: &[TokenStop]) -> Expr {
        let mut expr = self.parse_add(stop);
        while !self.is_at_end() && !self.at_stop(stop) && self.match_kind(&TokenKind::DoubleEquals)
        {
            let right = self.parse_add(stop);
            expr = Expr::Binary {
                op: BinaryOp::Equal,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        expr
    }

    fn parse_add(&mut self, stop: &[TokenStop]) -> Expr {
        let mut expr = self.parse_primary(stop);
        while !self.is_at_end() && !self.at_stop(stop) {
            let op = if self.match_kind(&TokenKind::Plus) {
                Some(BinaryOp::Add)
            } else if self.match_kind(&TokenKind::Minus) {
                Some(BinaryOp::Subtract)
            } else {
                None
            };
            let Some(op) = op else {
                break;
            };
            let right = self.parse_primary(stop);
            expr = Expr::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        expr
    }

    fn parse_primary(&mut self, stop: &[TokenStop]) -> Expr {
        if self.is_at_end() || self.at_stop(stop) {
            return Expr::Missing;
        }
        let Some(token) = self.advance().cloned() else {
            return Expr::Missing;
        };
        let span_token = token.clone();
        let mut expr = match token.kind {
            TokenKind::Number(value) => Expr::Number(value),
            TokenKind::Minus => match self.advance().cloned() {
                Some(Token {
                    kind: TokenKind::Number(value),
                    ..
                }) => Expr::Number(format!("-{value}")),
                Some(next) => {
                    self.diagnostics.push(SyntaxDiagnostic {
                        message: format!("expected number after unary `-`, found {:?}", next.kind),
                        span: SourceSpan {
                            start: token.start,
                            end: next.end,
                        },
                    });
                    Expr::Missing
                }
                None => {
                    self.diagnostics.push(SyntaxDiagnostic {
                        message: "expected number after unary `-`".to_owned(),
                        span: SourceSpan {
                            start: token.start,
                            end: token.end,
                        },
                    });
                    Expr::Missing
                }
            },
            TokenKind::Word(word) => self.parse_word(word, &span_token, stop),
            TokenKind::LBracket => self.parse_record(),
            TokenKind::LParen => {
                let expr = self.parse_expr_until(&[TokenStop::RParen]);
                self.expect(TokenKind::RParen, "expected `)` after expression");
                expr
            }
            unexpected => {
                self.diagnostics.push(SyntaxDiagnostic {
                    message: format!("expected expression, found {unexpected:?}"),
                    span: SourceSpan {
                        start: token.start,
                        end: token.end,
                    },
                });
                Expr::Missing
            }
        };
        while !self.is_at_end() && !self.at_stop(stop) && self.match_kind(&TokenKind::Dot) {
            let Some(field) = self.take_word() else {
                self.diagnostics.push(SyntaxDiagnostic {
                    message: "expected field name after `.`".to_owned(),
                    span: SourceSpan {
                        start: self
                            .previous()
                            .map(|token| token.start)
                            .unwrap_or(span_token.end),
                        end: self.current_end().unwrap_or(span_token.end),
                    },
                });
                break;
            };
            expr = Expr::FieldAccess {
                base: Box::new(expr),
                field,
            };
        }
        expr
    }

    fn parse_word(&mut self, word: String, token: &Token, stop: &[TokenStop]) -> Expr {
        match word.as_str() {
            "SOURCE" if self.match_kind(&TokenKind::LBrace) => Expr::SourceAt {
                target: Box::new(self.parse_braced_single_expr()),
            },
            "SOURCE" => Expr::Source,
            "LINK" if self.match_kind(&TokenKind::LBrace) => Expr::Link {
                target: Some(Box::new(self.parse_braced_single_expr())),
            },
            "LINK" => Expr::Link { target: None },
            "SKIP" => Expr::Skip,
            "True" | "False" => Expr::Tag(word),
            "TEXT" if self.match_kind(&TokenKind::LBrace) => Expr::Text(self.collect_text(token)),
            "LIST" if self.match_kind(&TokenKind::LBrace) => {
                Expr::List(self.parse_expr_list(TokenStop::RBrace))
            }
            "BLOCK" if self.match_kind(&TokenKind::LBrace) => {
                Expr::Block(self.parse_expr_list(TokenStop::RBrace))
            }
            "LATEST" if self.match_kind(&TokenKind::LBrace) => {
                Expr::Latest(self.parse_expr_list(TokenStop::RBrace))
            }
            "THEN" if self.match_kind(&TokenKind::LBrace) => Expr::Then {
                body: self.parse_expr_list(TokenStop::RBrace),
            },
            "HOLD" => {
                let binder = self.take_word().unwrap_or_else(|| {
                    self.diagnostics.push(SyntaxDiagnostic {
                        message: "expected HOLD binder".to_owned(),
                        span: SourceSpan {
                            start: token.start,
                            end: token.end,
                        },
                    });
                    "state".to_owned()
                });
                self.expect(TokenKind::LBrace, "expected `{` after HOLD binder");
                Expr::Hold {
                    binder,
                    body: self.parse_expr_list(TokenStop::RBrace),
                }
            }
            "WHEN" if self.match_kind(&TokenKind::LBrace) => Expr::Match {
                kind: MatchKind::When,
                arms: self.parse_match_arms(),
            },
            "WHILE" if self.match_kind(&TokenKind::LBrace) => Expr::Match {
                kind: MatchKind::While,
                arms: self.parse_match_arms(),
            },
            _ if self.match_kind(&TokenKind::LParen) => Expr::Call {
                callee: word,
                args: self.parse_call_args(),
            },
            _ if self.match_kind(&TokenKind::LBracket) => Expr::Constructor {
                callee: word,
                fields: self.parse_record_fields(TokenStop::RBracket),
            },
            _ => {
                if self.at_stop(stop) {
                    Expr::Path(word)
                } else {
                    Expr::Path(word)
                }
            }
        }
    }

    fn parse_record(&mut self) -> Expr {
        Expr::Record(self.parse_record_fields(TokenStop::RBracket))
    }

    fn parse_record_fields(&mut self, end: TokenStop) -> Vec<RecordField> {
        let mut fields = Vec::new();
        while !self.is_at_end() && !self.at_stop(&[end]) {
            if self.match_kind(&TokenKind::Comma) {
                continue;
            }
            let Some(name) = self.take_word() else {
                self.skip_one("expected record field name");
                continue;
            };
            self.expect(TokenKind::Colon, "expected `:` after record field name");
            let value = self.parse_expr_until(&[end, TokenStop::Comma, TokenStop::FieldStart]);
            fields.push(RecordField { name, value });
            self.match_kind(&TokenKind::Comma);
        }
        self.expect_stop(end, "expected record closing delimiter");
        fields
    }

    fn parse_call_args(&mut self) -> Vec<CallArg> {
        let mut args = Vec::new();
        while !self.is_at_end() && !self.at_stop(&[TokenStop::RParen]) {
            if self.match_kind(&TokenKind::Comma) {
                continue;
            }
            if self.current_word_followed_by_colon() {
                let name = self.take_word().unwrap();
                self.expect(TokenKind::Colon, "expected `:` after argument name");
                let value = self.parse_expr_until(&[
                    TokenStop::RParen,
                    TokenStop::Comma,
                    TokenStop::FieldStart,
                ]);
                args.push(CallArg::Named { name, value });
            } else {
                let value = self.parse_expr_until(&[TokenStop::RParen, TokenStop::Comma]);
                args.push(CallArg::Positional(value));
            }
            self.match_kind(&TokenKind::Comma);
        }
        self.expect(TokenKind::RParen, "expected `)` after call arguments");
        args
    }

    fn parse_expr_list(&mut self, end: TokenStop) -> Vec<Expr> {
        let mut values = Vec::new();
        while !self.is_at_end() && !self.at_stop(&[end]) {
            if self.match_kind(&TokenKind::Comma) {
                continue;
            }
            values.push(self.parse_expr_until(&[end, TokenStop::Comma]));
            self.match_kind(&TokenKind::Comma);
        }
        self.expect_stop(end, "expected closing delimiter");
        values
    }

    fn parse_braced_single_expr(&mut self) -> Expr {
        let expr = self.parse_expr_until(&[TokenStop::RBrace]);
        self.expect(TokenKind::RBrace, "expected `}` after expression");
        expr
    }

    fn parse_match_arms(&mut self) -> Vec<MatchArm> {
        let mut arms = Vec::new();
        while !self.is_at_end() && !self.at_stop(&[TokenStop::RBrace]) {
            let Some(pattern) = self.take_word() else {
                self.skip_one("expected match pattern");
                continue;
            };
            self.expect(TokenKind::FatArrow, "expected `=>` after match pattern");
            let value = self.parse_expr_until(&[TokenStop::RBrace, TokenStop::ArmStart]);
            arms.push(MatchArm { pattern, value });
        }
        self.expect(TokenKind::RBrace, "expected `}` after match arms");
        arms
    }

    fn collect_text(&mut self, opener: &Token) -> String {
        let Some(open_brace) = self.tokens.get(self.index.saturating_sub(1)).cloned() else {
            return String::new();
        };
        let mut depth = 1_i32;
        let content_start = open_brace.end;
        let mut content_end = open_brace.end;
        while let Some(token) = self.advance().cloned() {
            match token.kind {
                TokenKind::LBrace => depth += 1,
                TokenKind::RBrace => {
                    depth -= 1;
                    if depth == 0 {
                        content_end = token.start;
                        break;
                    }
                }
                _ => {}
            }
            content_end = token.end;
        }
        if depth != 0 {
            self.diagnostics.push(SyntaxDiagnostic {
                message: "unterminated TEXT literal".to_owned(),
                span: SourceSpan {
                    start: opener.start,
                    end: opener.end,
                },
            });
        }
        let relative_start = content_start.saturating_sub(self.base_offset);
        let relative_end = content_end.saturating_sub(self.base_offset);
        self.source
            .get(relative_start..relative_end)
            .unwrap_or_default()
            .strip_prefix(' ')
            .and_then(|value| value.strip_suffix(' '))
            .unwrap_or_else(|| {
                self.source
                    .get(relative_start..relative_end)
                    .unwrap_or_default()
            })
            .to_owned()
    }

    fn at_stop(&self, stops: &[TokenStop]) -> bool {
        stops.iter().any(|stop| self.matches_stop(*stop))
    }

    fn matches_stop(&self, stop: TokenStop) -> bool {
        match stop {
            TokenStop::RBracket => self.check(&TokenKind::RBracket),
            TokenStop::RParen => self.check(&TokenKind::RParen),
            TokenStop::RBrace => self.check(&TokenKind::RBrace),
            TokenStop::Comma => self.check(&TokenKind::Comma),
            TokenStop::FieldStart => self.current_word_followed_by_colon(),
            TokenStop::ArmStart => self.current_word_followed_by_fat_arrow(),
        }
    }

    fn current_word_followed_by_colon(&self) -> bool {
        matches!(
            self.peek().map(|token| &token.kind),
            Some(TokenKind::Word(_))
        ) && matches!(
            self.tokens.get(self.index + 1).map(|token| &token.kind),
            Some(TokenKind::Colon)
        )
    }

    fn current_word_followed_by_fat_arrow(&self) -> bool {
        matches!(
            self.peek().map(|token| &token.kind),
            Some(TokenKind::Word(_))
        ) && matches!(
            self.tokens.get(self.index + 1).map(|token| &token.kind),
            Some(TokenKind::FatArrow)
        )
    }

    fn take_word(&mut self) -> Option<String> {
        match self.peek().map(|token| &token.kind) {
            Some(TokenKind::Word(_)) => match self.advance().map(|token| token.kind.clone()) {
                Some(TokenKind::Word(word)) => Some(word),
                _ => None,
            },
            _ => None,
        }
    }

    fn previous(&self) -> Option<&Token> {
        self.index
            .checked_sub(1)
            .and_then(|index| self.tokens.get(index))
    }

    fn current_end(&self) -> Option<usize> {
        self.tokens.get(self.index).map(|token| token.end)
    }

    fn skip_one(&mut self, message: &str) {
        if let Some(token) = self.advance().cloned() {
            self.diagnostics.push(SyntaxDiagnostic {
                message: message.to_owned(),
                span: SourceSpan {
                    start: token.start,
                    end: token.end,
                },
            });
        }
    }

    fn expect_stop(&mut self, stop: TokenStop, message: &str) {
        match stop {
            TokenStop::RBracket => self.expect(TokenKind::RBracket, message),
            TokenStop::RParen => self.expect(TokenKind::RParen, message),
            TokenStop::RBrace => self.expect(TokenKind::RBrace, message),
            _ => {}
        }
    }

    fn expect(&mut self, kind: TokenKind, message: &str) {
        if self.match_kind(&kind) {
            return;
        }
        let span = self
            .peek()
            .map_or(SourceSpan { start: 0, end: 0 }, |token| SourceSpan {
                start: token.start,
                end: token.end,
            });
        self.diagnostics.push(SyntaxDiagnostic {
            message: message.to_owned(),
            span,
        });
    }

    fn match_kind(&mut self, kind: &TokenKind) -> bool {
        if self.check(kind) {
            self.index += 1;
            true
        } else {
            false
        }
    }

    fn check(&self, kind: &TokenKind) -> bool {
        self.peek()
            .is_some_and(|token| same_token_variant(&token.kind, kind))
    }

    fn advance(&mut self) -> Option<&Token> {
        let token = self.tokens.get(self.index);
        if token.is_some() {
            self.index += 1;
        }
        token
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.index)
    }

    fn is_at_end(&self) -> bool {
        self.index >= self.tokens.len()
    }
}

#[derive(Clone, Copy)]
enum TokenStop {
    RBracket,
    RParen,
    RBrace,
    Comma,
    FieldStart,
    ArmStart,
}

fn same_token_variant(left: &TokenKind, right: &TokenKind) -> bool {
    matches!(
        (left, right),
        (TokenKind::Word(_), TokenKind::Word(_))
            | (TokenKind::Number(_), TokenKind::Number(_))
            | (TokenKind::Colon, TokenKind::Colon)
            | (TokenKind::Comma, TokenKind::Comma)
            | (TokenKind::Plus, TokenKind::Plus)
            | (TokenKind::Minus, TokenKind::Minus)
            | (TokenKind::Dot, TokenKind::Dot)
            | (TokenKind::Equals, TokenKind::Equals)
            | (TokenKind::DoubleEquals, TokenKind::DoubleEquals)
            | (TokenKind::Pipe, TokenKind::Pipe)
            | (TokenKind::FatArrow, TokenKind::FatArrow)
            | (TokenKind::LBracket, TokenKind::LBracket)
            | (TokenKind::RBracket, TokenKind::RBracket)
            | (TokenKind::LParen, TokenKind::LParen)
            | (TokenKind::RParen, TokenKind::RParen)
            | (TokenKind::LBrace, TokenKind::LBrace)
            | (TokenKind::RBrace, TokenKind::RBrace)
    )
}

fn leading_spaces(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

struct ParsedDefinitionHeader<'a> {
    name: String,
    parameters: Vec<String>,
    is_function: bool,
    expression: &'a str,
    expression_offset: usize,
}

fn definition_header<'a>(
    block: &'a str,
    block_offset: usize,
    diagnostics: &mut Vec<SyntaxDiagnostic>,
) -> Option<ParsedDefinitionHeader<'a>> {
    let trimmed = block.trim_start();
    let leading = block.len() - trimmed.len();
    if let Some((name, parameters, body, body_offset)) =
        function_header_and_body(trimmed, block_offset + leading, diagnostics)
    {
        return Some(ParsedDefinitionHeader {
            name,
            parameters,
            is_function: true,
            expression: body,
            expression_offset: body_offset,
        });
    }
    let colon = block.find(':')?;
    Some(ParsedDefinitionHeader {
        name: block[..colon].trim().to_owned(),
        parameters: Vec::new(),
        is_function: false,
        expression: &block[colon + 1..],
        expression_offset: block_offset + colon + 1,
    })
}

fn value_definition_label(trimmed: &str) -> Option<String> {
    let colon = trimmed.find(':')?;
    let candidate = trimmed[..colon].trim();
    if candidate.is_empty()
        || candidate
            .chars()
            .any(|ch| !(ch.is_ascii_alphanumeric() || ch == '_'))
    {
        None
    } else {
        Some(candidate.to_owned())
    }
}

fn function_definition_header(trimmed: &str) -> Option<String> {
    let rest = trimmed.strip_prefix("FUNCTION ")?;
    let open = rest.find('(')?;
    let name = rest[..open].trim();
    if name.is_empty()
        || name
            .chars()
            .any(|ch| !(ch.is_ascii_alphanumeric() || ch == '_'))
    {
        None
    } else {
        Some(name.to_owned())
    }
}

fn function_header_and_body<'a>(
    trimmed: &'a str,
    trimmed_offset: usize,
    diagnostics: &mut Vec<SyntaxDiagnostic>,
) -> Option<(String, Vec<String>, &'a str, usize)> {
    let name = function_definition_header(trimmed)?;
    let open_paren = trimmed.find('(')?;
    let close_paren = trimmed[open_paren + 1..].find(')')? + open_paren + 1;
    let parameters = trimmed[open_paren + 1..close_paren]
        .split(',')
        .map(str::trim)
        .filter(|param| !param.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let Some(open_brace) = trimmed[close_paren + 1..]
        .find('{')
        .map(|index| index + close_paren + 1)
    else {
        diagnostics.push(SyntaxDiagnostic {
            message: "expected `{` after FUNCTION header".to_owned(),
            span: SourceSpan {
                start: trimmed_offset,
                end: trimmed_offset + close_paren + 1,
            },
        });
        return None;
    };
    let close_brace = matching_last_brace(trimmed).unwrap_or_else(|| {
        diagnostics.push(SyntaxDiagnostic {
            message: "expected `}` after FUNCTION body".to_owned(),
            span: SourceSpan {
                start: trimmed_offset + open_brace,
                end: trimmed_offset + open_brace + 1,
            },
        });
        trimmed.len()
    });
    let body_start = open_brace + 1;
    let body_end = close_brace.min(trimmed.len());
    Some((
        name,
        parameters,
        &trimmed[body_start..body_end],
        trimmed_offset + body_start,
    ))
}

fn matching_last_brace(text: &str) -> Option<usize> {
    let mut depth = 0_i32;
    let mut last_closed = None;
    for (index, ch) in text.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    last_closed = Some(index);
                }
            }
            _ => {}
        }
    }
    last_closed
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn parses_all_checked_in_examples_without_diagnostics() {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let examples_dir = manifest_dir.join("../../examples");
        let mut parsed = 0_usize;
        for entry in fs::read_dir(&examples_dir).expect("examples dir exists") {
            let entry = entry.expect("example entry");
            let source_path = entry.path().join("source.bn");
            if !source_path.exists() {
                continue;
            }
            let text = fs::read_to_string(&source_path).expect("source readable");
            let module = parse_source(source_path.display().to_string(), text);
            assert!(
                !module.definitions.is_empty(),
                "no definitions parsed for {}",
                source_path.display()
            );
            assert!(
                module.diagnostics.is_empty(),
                "diagnostics for {}: {:#?}",
                source_path.display(),
                module.diagnostics
            );
            parsed += 1;
        }
        assert!(
            parsed >= 22,
            "expected current example corpus, parsed {parsed}"
        );
    }

    #[test]
    fn parses_counter_sources_from_ast_not_raw_module_only() {
        let module = parse_source(
            "examples/counter/source.bn",
            include_str!("../../../examples/counter/source.bn"),
        );
        assert_eq!(module.definitions.len(), 2);
        assert_eq!(module.definitions[0].name, "store");
        assert_eq!(module.definitions[1].name, "document");
        assert!(module.diagnostics.is_empty());
    }

    #[test]
    fn parses_cross_repo_numeric_and_equality_syntax_without_text_scans() {
        let module = parse_source(
            "cross_repo_numeric.bn",
            "-- sibling repos use these operators\nvalue: Number/max(left: score - 1, right: -12)\nvisible: item.id == selected_id\n",
        );
        assert!(module.diagnostics.is_empty(), "{:#?}", module.diagnostics);
        assert_eq!(module.definitions.len(), 2);

        let Expr::Call { args, .. } = &module.definitions[0].expression else {
            panic!("expected Number/max call");
        };
        let Some(CallArg::Named { value, .. }) = args.first() else {
            panic!("expected named left argument");
        };
        assert!(matches!(
            value,
            Expr::Binary {
                op: BinaryOp::Subtract,
                ..
            }
        ));
        let Some(CallArg::Named { value, .. }) = args.get(1) else {
            panic!("expected named right argument");
        };
        assert_eq!(value, &Expr::Number("-12".to_owned()));

        assert!(matches!(
            &module.definitions[1].expression,
            Expr::Binary {
                op: BinaryOp::Equal,
                ..
            }
        ));
    }

    #[test]
    fn preserves_function_link_and_source_target_syntax() {
        let module = parse_source(
            "cross_repo_link_function.bn",
            "FUNCTION greeting(name) {\n    TEXT { Hello {name} }\n}\nstore: [toggle: LINK]\ndocument: greeting(name: TEXT { World }) |> LINK { store.toggle }\ninput: text_input() |> SOURCE { PASSED.store.input }\n",
        );
        assert!(module.diagnostics.is_empty(), "{:#?}", module.diagnostics);
        assert_eq!(module.definitions.len(), 4);
        assert_eq!(module.definitions[0].name, "greeting");
        assert!(module.definitions[0].is_function);
        assert_eq!(module.definitions[0].parameters, ["name"]);

        let Expr::Record(fields) = &module.definitions[1].expression else {
            panic!("expected store record");
        };
        assert!(matches!(
            fields.first().map(|field| &field.value),
            Some(Expr::Link { target: None })
        ));

        let Expr::Pipe { stage, .. } = &module.definitions[2].expression else {
            panic!("expected document link pipe");
        };
        assert!(matches!(stage.as_ref(), Expr::Link { target: Some(_) }));

        let Expr::Pipe { stage, .. } = &module.definitions[3].expression else {
            panic!("expected input source pipe");
        };
        assert!(matches!(stage.as_ref(), Expr::SourceAt { .. }));
    }

    #[test]
    fn parses_postfix_field_access_on_calls() {
        let module = parse_source(
            "field_access.bn",
            "color: Theme/material(of: Danger).color\nicon: Assets/icon().checkbox_completed\n",
        );
        assert!(module.diagnostics.is_empty(), "{:#?}", module.diagnostics);
        assert_eq!(module.definitions.len(), 2);
        let Expr::FieldAccess { base, field } = &module.definitions[0].expression else {
            panic!("expected material field access");
        };
        assert_eq!(field, "color");
        assert!(matches!(base.as_ref(), Expr::Call { callee, .. } if callee == "Theme/material"));
        let Expr::FieldAccess { base, field } = &module.definitions[1].expression else {
            panic!("expected icon field access");
        };
        assert_eq!(field, "checkbox_completed");
        assert!(matches!(base.as_ref(), Expr::Call { callee, .. } if callee == "Assets/icon"));
    }
}
