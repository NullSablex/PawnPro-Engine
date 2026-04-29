#![allow(dead_code)]

// Lexer fiel ao compilador Pawn (sc2.c). Produz Token com `stmt_indent` — coluna do
// primeiro token de cada linha calculada com expansão de tabs via `sc_tabsize`, necessário
// para PP0017 (loose indentation / warning 217).

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    // Literais
    Ident,         // identificador ou keyword
    Integer,       // literal inteiro (decimal, hex 0x, binário 0b)
    Float,         // literal float
    StringLit,     // "..."
    CharLit,       // '...'

    // Preprocessor
    Hash,          // # (início de diretiva)
    Directive,     // palavra após #: include, define, pragma, if, etc.

    // Delimitadores
    LParen,        // (
    RParen,        // )
    LBracket,      // [
    RBracket,      // ]
    LBrace,        // {
    RBrace,        // }
    Semicolon,     // ;
    Comma,         // ,
    Colon,         // :
    Ellipsis,      // ...

    // Operadores
    Equals,        // =
    PlusEq,        // +=
    MinusEq,       // -=
    StarEq,        // *=
    SlashEq,       // /=
    PercentEq,     // %=
    AmpEq,         // &=
    PipeEq,        // |=
    CaretEq,       // ^=
    ShrEq,         // >>=
    ShlEq,         // <<=
    Plus,          // +
    Minus,         // -
    Star,          // *
    Slash,         // /
    Percent,       // %
    Amp,           // &
    Pipe,          // |
    Caret,         // ^
    Tilde,         // ~
    Bang,          // !
    Lt,            // <
    Gt,            // >
    Shl,           // <<
    Shr,           // >>
    EqEq,          // ==
    BangEq,        // !=
    LtEq,          // <=
    GtEq,          // >=
    AmpAmp,        // &&
    PipePipe,      // ||
    PlusPlus,      // ++
    MinusMinus,    // --
    Arrow,         // ->
    ColonColon,    // ::
    Dot,           // .
    Question,      // ?

    // Continuação de linha
    LineContinuation,

    // Fim de arquivo
    Eof,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub value: String,
    pub line: u32,
    pub col: u32,
    /// colunas; calculado só no 1º token de cada linha lógica, expandindo tabs via tabsize
    pub stmt_indent: u32,
}

pub struct TokenStream {
    pub tokens: Vec<Token>,
    /// Valor final de `sc_tabsize` após processar todos os `#pragma tabsize N`.
    pub tabsize: u32,
}

pub fn tokenize(source: &str) -> TokenStream {
    Lexer::new(source).run()
}

struct Lexer<'s> {
    src: &'s [u8],
    pos: usize,
    line: u32,
    col: u32,

    in_block_comment: bool,
    tabsize: u32,
    current_line_indent: u32,
    at_line_start: bool,

    tokens: Vec<Token>,
}

impl<'s> Lexer<'s> {
    fn new(source: &'s str) -> Self {
        Self {
            src: source.as_bytes(),
            pos: 0,
            line: 0,
            col: 0,
            in_block_comment: false,
            tabsize: 8,
            current_line_indent: 0,
            at_line_start: true,
            tokens: Vec::new(),
        }
    }

    fn run(mut self) -> TokenStream {
        while self.pos < self.src.len() {
            self.step();
        }
        self.tokens.push(Token {
            kind: TokenKind::Eof,
            value: String::new(),
            line: self.line,
            col: self.col,
            stmt_indent: self.current_line_indent,
        });
        let tabsize = self.tabsize;
        TokenStream { tokens: self.tokens, tabsize }
    }

    fn peek(&self) -> u8 {
        if self.pos < self.src.len() { self.src[self.pos] } else { 0 }
    }

    fn peek2(&self) -> u8 {
        if self.pos + 1 < self.src.len() { self.src[self.pos + 1] } else { 0 }
    }

    fn advance(&mut self) -> u8 {
        let ch = self.src[self.pos];
        self.pos += 1;
        if ch == b'\n' {
            self.line += 1;
            self.col = 0;
            self.at_line_start = true;
            self.current_line_indent = 0;
        } else {
            self.col += 1;
        }
        ch
    }

    /// Calcula a contribuição de um caractere de whitespace para `stmt_indent`,
    /// exatamente como `sc2.c:2340-2347`.
    fn add_indent_char(&mut self, ch: u8) {
        if ch == b'\t' && self.tabsize > 0 {
            let ts = self.tabsize;
            self.current_line_indent += ts - (self.current_line_indent + ts) % ts;
        } else {
            self.current_line_indent += 1;
        }
    }

    fn step(&mut self) {
        if self.in_block_comment {
            self.consume_block_comment_tail();
            return;
        }

        let ch = self.peek();

        if ch == b'\r' { self.advance(); return; }
        if ch == b'\n' { self.advance(); return; }

        if ch == b' ' || ch == b'\t' {
            if self.at_line_start { self.add_indent_char(ch); }
            self.advance();
            return;
        }

        self.at_line_start = false;
        let tok_indent = self.current_line_indent;
        let tok_line = self.line;
        let tok_col = self.col;

        if ch == b'\\' && (self.peek2() == b'\n' || self.peek2() == b'\r') {
            self.advance();
            // próxima linha é continuação lógica — não resetar at_line_start
            self.consume_newline();
            self.at_line_start = false;
            self.push(TokenKind::LineContinuation, "\\", tok_line, tok_col, tok_indent);
            return;
        }

        if ch == b'/' && self.peek2() == b'/' {
            self.consume_line_comment();
            return;
        }

        if ch == b'/' && self.peek2() == b'*' {
            self.advance(); self.advance();
            self.in_block_comment = true;
            self.consume_block_comment_tail();
            return;
        }

        if ch == b'#' && tok_col == 0 {
            self.lex_directive(tok_line, tok_indent);
            return;
        }

        if ch == b'"' { self.lex_string(tok_line, tok_col, tok_indent); return; }
        if ch == b'\'' { self.lex_char(tok_line, tok_col, tok_indent); return; }
        if ch.is_ascii_digit() { self.lex_number(tok_line, tok_col, tok_indent); return; }
        if ch.is_ascii_alphabetic() || ch == b'_' || ch == b'@' {
            self.lex_ident(tok_line, tok_col, tok_indent);
            return;
        }

        self.lex_punct(tok_line, tok_col, tok_indent);
    }

    fn push(&mut self, kind: TokenKind, value: &str, line: u32, col: u32, indent: u32) {
        self.tokens.push(Token {
            kind,
            value: value.to_string(),
            line,
            col,
            stmt_indent: indent,
        });
    }

    fn consume_newline(&mut self) {
        if self.peek() == b'\r' { self.advance(); }
        if self.peek() == b'\n' { self.advance(); }
    }

    fn consume_line_comment(&mut self) {
        while self.pos < self.src.len() && self.peek() != b'\n' {
            self.advance();
        }
    }

    fn consume_block_comment_tail(&mut self) {
        while self.pos < self.src.len() {
            if self.peek() == b'*' && self.peek2() == b'/' {
                self.advance(); self.advance();
                self.in_block_comment = false;
                return;
            }
            self.advance();
        }
    }

    fn lex_string(&mut self, line: u32, col: u32, indent: u32) {
        self.advance(); // '"'
        let mut val = String::from('"');
        loop {
            if self.pos >= self.src.len() { break; }
            let ch = self.advance();
            val.push(ch as char);
            if ch == b'\\' && self.pos < self.src.len() {
                let esc = self.peek();
                if esc == b'\n' || esc == b'\r' {
                    // \<newline>: continuação de linha dentro de string.
                    // Consome o newline e acumula o whitespace da próxima linha em
                    // current_line_indent, para que o token após o fechamento da string
                    // tenha o indent correto.
                    let nl = self.advance();
                    val.push(nl as char);
                    // Se \r\n, consome o \n também
                    if nl == b'\r' && self.peek() == b'\n' {
                        let lf = self.advance();
                        val.push(lf as char);
                    }
                    // Cada linha de continuação tem seu próprio indent — resetar antes de acumular.
                    self.current_line_indent = 0;
                    while self.pos < self.src.len() && (self.src[self.pos] == b' ' || self.src[self.pos] == b'\t') {
                        let ws = self.src[self.pos];
                        let wsc = self.advance();
                        val.push(wsc as char);
                        self.add_indent_char(ws);
                    }
                    self.at_line_start = false;
                } else {
                    let e = self.advance();
                    val.push(e as char);
                }
                continue;
            }
            if ch == b'"' { break; }
            if ch == b'\n' { break; } // string não fechada na linha
        }
        self.at_line_start = false;
        self.push(TokenKind::StringLit, &val, line, col, indent);
    }

    fn lex_char(&mut self, line: u32, col: u32, indent: u32) {
        self.advance(); // '\''
        let mut val = String::from('\'');
        loop {
            if self.pos >= self.src.len() { break; }
            let ch = self.advance();
            val.push(ch as char);
            if ch == b'\\' && self.pos < self.src.len() {
                let esc = self.advance();
                val.push(esc as char);
                if esc == b'\n' {
                    self.at_line_start = false;
                }
                continue;
            }
            if ch == b'\'' { break; }
            if ch == b'\n' { break; }
        }
        self.at_line_start = false;
        self.push(TokenKind::CharLit, &val, line, col, indent);
    }

    fn lex_number(&mut self, line: u32, col: u32, indent: u32) {
        let start = self.pos;
        let mut is_float = false;

        if self.peek() == b'0' && (self.peek2() == b'x' || self.peek2() == b'X') {
            self.advance(); self.advance();
            while self.pos < self.src.len() && self.src[self.pos].is_ascii_hexdigit() {
                self.advance();
            }
        } else if self.peek() == b'0' && (self.peek2() == b'b' || self.peek2() == b'B') {
            self.advance(); self.advance();
            while self.pos < self.src.len() && (self.src[self.pos] == b'0' || self.src[self.pos] == b'1') {
                self.advance();
            }
        } else {
            while self.pos < self.src.len() && self.src[self.pos].is_ascii_digit() {
                self.advance();
            }
            if self.pos < self.src.len() && self.src[self.pos] == b'.' {
                let next = if self.pos + 1 < self.src.len() { self.src[self.pos + 1] } else { 0 };
                if next.is_ascii_digit() {
                    is_float = true;
                    self.advance(); // '.'
                    while self.pos < self.src.len() && self.src[self.pos].is_ascii_digit() {
                        self.advance();
                    }
                }
            }
        }

        let val = std::str::from_utf8(&self.src[start..self.pos]).unwrap_or("").to_string();
        let kind = if is_float { TokenKind::Float } else { TokenKind::Integer };
        self.push(kind, &val, line, col, indent);
    }

    fn lex_ident(&mut self, line: u32, col: u32, indent: u32) {
        let start = self.pos;
        while self.pos < self.src.len() {
            let ch = self.src[self.pos];
            if ch.is_ascii_alphanumeric() || ch == b'_' || ch == b'@' {
                self.advance();
            } else {
                break;
            }
        }
        let val = std::str::from_utf8(&self.src[start..self.pos]).unwrap_or("").to_string();
        self.push(TokenKind::Ident, &val, line, col, indent);
    }

    fn lex_directive(&mut self, line: u32, indent: u32) {
        self.advance(); // '#'
        self.push(TokenKind::Hash, "#", line, 0, indent);

        while self.pos < self.src.len() && self.src[self.pos] == b' ' {
            self.advance();
        }

        let dir_col = self.col;
        let start = self.pos;
        while self.pos < self.src.len() && self.src[self.pos].is_ascii_alphabetic() {
            self.advance();
        }
        let word = std::str::from_utf8(&self.src[start..self.pos]).unwrap_or("").to_string();
        if !word.is_empty() {
            self.push(TokenKind::Directive, &word, line, dir_col, indent);
        }

        if word == "pragma" {
            self.skip_spaces();
            let pragma_start = self.pos;
            while self.pos < self.src.len() && self.src[self.pos].is_ascii_alphabetic() {
                self.advance();
            }
            let pragma_word = std::str::from_utf8(&self.src[pragma_start..self.pos])
                .unwrap_or("")
                .to_string();
            if pragma_word == "tabsize" {
                self.skip_spaces();
                let num_start = self.pos;
                while self.pos < self.src.len() && self.src[self.pos].is_ascii_digit() {
                    self.advance();
                }
                if let Ok(n) = std::str::from_utf8(&self.src[num_start..self.pos])
                    .unwrap_or("0")
                    .parse::<u32>()
                {
                    self.tabsize = n;
                }
            }
        }

        loop {
            while self.pos < self.src.len() && self.src[self.pos] != b'\n' && self.src[self.pos] != b'\\' {
                self.advance();
            }
            if self.pos < self.src.len() && self.src[self.pos] == b'\\' {
                self.advance();
                self.consume_newline();
            } else {
                break;
            }
        }
    }

    fn skip_spaces(&mut self) {
        while self.pos < self.src.len() && self.src[self.pos] == b' ' {
            self.advance();
        }
    }

    fn lex_punct(&mut self, line: u32, col: u32, indent: u32) {
        let ch = self.advance();
        let next = self.peek();

        let (kind, value): (TokenKind, &str) = match (ch, next) {
            (b'+', b'+') => { self.advance(); (TokenKind::PlusPlus,   "++") }
            (b'+', b'=') => { self.advance(); (TokenKind::PlusEq,     "+=") }
            (b'+', _)    =>                   (TokenKind::Plus,         "+"),
            (b'-', b'-') => { self.advance(); (TokenKind::MinusMinus,  "--") }
            (b'-', b'=') => { self.advance(); (TokenKind::MinusEq,     "-=") }
            (b'-', b'>') => { self.advance(); (TokenKind::Arrow,       "->") }
            (b'-', _)    =>                   (TokenKind::Minus,         "-"),
            (b'*', b'=') => { self.advance(); (TokenKind::StarEq,      "*=") }
            (b'*', _)    =>                   (TokenKind::Star,          "*"),
            (b'/', b'=') => { self.advance(); (TokenKind::SlashEq,     "/=") }
            (b'/', _)    =>                   (TokenKind::Slash,         "/"),
            (b'%', b'=') => { self.advance(); (TokenKind::PercentEq,   "%=") }
            (b'%', _)    =>                   (TokenKind::Percent,       "%"),
            (b'&', b'&') => { self.advance(); (TokenKind::AmpAmp,      "&&") }
            (b'&', b'=') => { self.advance(); (TokenKind::AmpEq,       "&=") }
            (b'&', _)    =>                   (TokenKind::Amp,           "&"),
            (b'|', b'|') => { self.advance(); (TokenKind::PipePipe,    "||") }
            (b'|', b'=') => { self.advance(); (TokenKind::PipeEq,      "|=") }
            (b'|', _)    =>                   (TokenKind::Pipe,          "|"),
            (b'^', b'=') => { self.advance(); (TokenKind::CaretEq,     "^=") }
            (b'^', _)    =>                   (TokenKind::Caret,         "^"),
            (b'<', b'<') => {
                self.advance();
                if self.peek() == b'=' { self.advance(); (TokenKind::ShlEq, "<<=") }
                else { (TokenKind::Shl, "<<") }
            }
            (b'<', b'=') => { self.advance(); (TokenKind::LtEq,       "<=") }
            (b'<', _)    =>                   (TokenKind::Lt,            "<"),
            (b'>', b'>') => {
                self.advance();
                if self.peek() == b'=' { self.advance(); (TokenKind::ShrEq, ">>=") }
                else { (TokenKind::Shr, ">>") }
            }
            (b'>', b'=') => { self.advance(); (TokenKind::GtEq,       ">=") }
            (b'>', _)    =>                   (TokenKind::Gt,            ">"),
            (b'=', b'=') => { self.advance(); (TokenKind::EqEq,       "==") }
            (b'=', _)    =>                   (TokenKind::Equals,        "="),
            (b'!', b'=') => { self.advance(); (TokenKind::BangEq,     "!=") }
            (b'!', _)    =>                   (TokenKind::Bang,          "!"),
            (b':', b':') => { self.advance(); (TokenKind::ColonColon,  "::") }
            (b':', _)    =>                   (TokenKind::Colon,         ":"),
            (b'.', b'.') if self.peek2() == b'.' => {
                self.advance(); self.advance(); (TokenKind::Ellipsis, "...")
            }
            (b'.', _)    =>                   (TokenKind::Dot,           "."),
            (b'(', _)    =>                   (TokenKind::LParen,        "("),
            (b')', _)    =>                   (TokenKind::RParen,        ")"),
            (b'[', _)    =>                   (TokenKind::LBracket,      "["),
            (b']', _)    =>                   (TokenKind::RBracket,      "]"),
            (b'{', _)    =>                   (TokenKind::LBrace,        "{"),
            (b'}', _)    =>                   (TokenKind::RBrace,        "}"),
            (b';', _)    =>                   (TokenKind::Semicolon,     ";"),
            (b',', _)    =>                   (TokenKind::Comma,         ","),
            (b'~', _)    =>                   (TokenKind::Tilde,         "~"),
            (b'?', _)    =>                   (TokenKind::Question,      "?"),
            _            => return,
        };

        self.push(kind, value, line, col, indent);
    }
}

pub fn tokenize_with_tabsize(source: &str, tabsize: u32) -> TokenStream {
    let mut lex = Lexer::new(source);
    lex.tabsize = tabsize;
    lex.run()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<TokenKind> {
        tokenize(src).tokens.into_iter().map(|t| t.kind).collect()
    }

    #[test]
    fn basic_ident_and_punct() {
        let ts = tokenize("foo(bar);");
        let k: Vec<_> = ts.tokens.iter().map(|t| &t.kind).collect();
        assert!(matches!(k[0], TokenKind::Ident));
        assert!(matches!(k[1], TokenKind::LParen));
        assert!(matches!(k[2], TokenKind::Ident));
        assert!(matches!(k[3], TokenKind::RParen));
        assert!(matches!(k[4], TokenKind::Semicolon));
    }

    #[test]
    fn stmt_indent_spaces() {
        let src = "    foo();";
        let ts = tokenize(src);
        assert_eq!(ts.tokens[0].stmt_indent, 4);
    }

    #[test]
    fn stmt_indent_inside_block() {
        let src = "main()\n{\n    foo();\n    bar();\n}";
        let ts = tokenize(src);
        for t in &ts.tokens {
            eprintln!("L{} indent={} kind={:?} val={:?}", t.line, t.stmt_indent, t.kind, t.value);
        }
        let foo = ts.tokens.iter().find(|t| t.value == "foo").unwrap();
        let bar = ts.tokens.iter().find(|t| t.value == "bar").unwrap();
        assert_eq!(foo.stmt_indent, 4, "foo deve ter stmt_indent=4");
        assert_eq!(bar.stmt_indent, 4, "bar deve ter stmt_indent=4");
    }

    #[test]
    fn stmt_indent_tab_tabsize8() {
        // um tab com tabsize=8 deve valer 8
        let src = "\tfoo();";
        let ts = tokenize(src);
        assert_eq!(ts.tokens[0].stmt_indent, 8);
    }

    #[test]
    fn pragma_tabsize_updates_lexer() {
        let src = "#pragma tabsize 4\n\tfoo();";
        let ts = tokenize(src);
        assert_eq!(ts.tabsize, 4);
        // tab com tabsize=4 vale 4
        let foo = ts.tokens.iter().find(|t| t.value == "foo").unwrap();
        assert_eq!(foo.stmt_indent, 4);
    }

    #[test]
    fn string_with_color_codes_no_brace_tokens() {
        let src = r#"Create3DTextLabel("{FF0000}texto", -1, 0.0, 0.0, 0.0, 10.0, 0);"#;
        let ts = tokenize(src);
        // não deve haver LBrace/RBrace — estão dentro da string
        assert!(!ts.tokens.iter().any(|t| matches!(t.kind, TokenKind::LBrace | TokenKind::RBrace)));
    }

    #[test]
    fn block_comment_skipped() {
        let src = "/* comentário */ foo();";
        let ts = tokenize(src);
        assert_eq!(ts.tokens[0].value, "foo");
    }

    #[test]
    fn multiline_block_comment() {
        let src = "/* linha1\nlinha2 */ bar();";
        let ts = tokenize(src);
        assert_eq!(ts.tokens[0].value, "bar");
    }

    #[test]
    fn line_comment_skipped() {
        let src = "foo(); // comentário\nbar();";
        let ts = tokenize(src);
        let idents: Vec<_> = ts.tokens.iter()
            .filter(|t| matches!(t.kind, TokenKind::Ident))
            .map(|t| t.value.as_str())
            .collect();
        assert_eq!(idents, vec!["foo", "bar"]);
    }

    #[test]
    fn operators() {
        let k = kinds("a += b == c && d != e");
        assert!(k.contains(&TokenKind::PlusEq));
        assert!(k.contains(&TokenKind::EqEq));
        assert!(k.contains(&TokenKind::AmpAmp));
        assert!(k.contains(&TokenKind::BangEq));
    }

    #[test]
    fn hex_number() {
        let ts = tokenize("0xFF");
        assert!(matches!(ts.tokens[0].kind, TokenKind::Integer));
        assert_eq!(ts.tokens[0].value, "0xFF");
    }

    #[test]
    fn float_number() {
        let ts = tokenize("3.14");
        assert!(matches!(ts.tokens[0].kind, TokenKind::Float));
    }

    #[test]
    fn tab_then_spaces_indent() {
        // \t + 4 espaços com tabsize=4: tab vai para col 4, +4 espaços = 8
        // Deve ser igual a 8 espaços puros
        let src_tab_spaces = "\t    Kick(playerid);";
        let src_spaces = "        Kick(playerid);";
        let ts1 = tokenize_with_tabsize(src_tab_spaces, 4);
        let ts2 = tokenize_with_tabsize(src_spaces, 4);
        let indent1 = ts1.tokens[0].stmt_indent;
        let indent2 = ts2.tokens[0].stmt_indent;
        eprintln!("tab+spaces indent={}, 8spaces indent={}", indent1, indent2);
        assert_eq!(indent1, indent2, "\\t+4 espaços deve = 8 espaços com tabsize=4");
    }

    #[test]
    fn multiline_string_continuation_indent() {
        // String com \n\ (continuação física de linha) — o token após a string
        // deve herdar o indent correto da linha onde a string fecha, não zero.
        let src = concat!(
            "    ShowPlayerDialog(playerid, 0, 0,\n",
            "        \"Armas:\",\n",
            "        \"Knife $200\\n\\\n",
            "        Desert $1000\",\n",
            "        \"Ok\", \"Sair\");\n",
            "    return 1;\n",
        );
        let ts = tokenize(src);
        // A vírgula após a string multilinha deve ter indent=8 (não 0)
        let comma_after_str = ts.tokens.iter()
            .skip_while(|t| !(t.kind == TokenKind::StringLit && t.value.contains("Desert")))
            .find(|t| t.kind == TokenKind::Comma)
            .unwrap();
        assert_eq!(comma_after_str.stmt_indent, 8, "vírgula após string multilinha deve ter indent=8");
        // return deve ter indent=4
        let ret = ts.tokens.iter().find(|t| t.kind == TokenKind::Ident && t.value == "return").unwrap();
        assert_eq!(ret.stmt_indent, 4, "return deve ter stmt_indent=4");
    }
}
