//! Parser de statements fiel ao compilador Pawn (sc1.c: compound() + statement()).
//!
//! Consome um [`TokenStream`] e produz uma [`StmtTree`] — uma sequência de
//! [`Stmt`] com escopo, indentação e contexto de bloco. Essa estrutura é a
//! fundação para diagnósticos que dependem de fluxo sintático real:
//! PP0017 (loose indentation), variáveis não inicializadas, fluxo de controle, etc.
//!
//! A diferença do compilador real é que aqui não geramos código — apenas
//! analisamos a estrutura para fins de diagnóstico.

#![allow(dead_code)]

use super::token_lexer::{Token, TokenKind, TokenStream};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StmtKind {
    If, Else, While, Do, For, Switch, Case, Default,
    Return, Break, Continue, Exit, Goto,
    FuncDecl, VarDecl, ConstDecl, EnumDecl,
    Label, Pragma, Include, Define, Expr,
    BlockOpen, BlockClose,
}

#[derive(Debug, Clone)]
pub struct Stmt {
    pub kind: StmtKind,
    pub line: u32,
    pub col: u32,
    /// como sc2.c calcula stmtindent
    pub stmt_indent: u32,
    pub depth: u32,
    /// > 0 = dentro de argumentos de chamada; não checado para indentação
    pub paren_depth: u32,
    pub value: String,
}

#[derive(Debug, Default)]
pub struct StmtTree {
    pub stmts: Vec<Stmt>,
    /// valor final de sc_tabsize após #pragma tabsize
    pub tabsize: u32,
}

pub fn parse_stmts(stream: TokenStream) -> StmtTree {
    Parser::new(stream).run()
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    depth: u32,
    paren_depth: u32,
    tabsize: u32,
    stmts: Vec<Stmt>,
}

impl Parser {
    fn new(stream: TokenStream) -> Self {
        Self {
            tabsize: stream.tabsize,
            tokens: stream.tokens,
            pos: 0,
            depth: 0,
            paren_depth: 0,
            stmts: Vec::new(),
        }
    }

    fn run(mut self) -> StmtTree {
        while !self.at_eof() {
            self.parse_one();
        }
        StmtTree { stmts: self.stmts, tabsize: self.tabsize }
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn at_eof(&self) -> bool {
        matches!(self.peek().kind, TokenKind::Eof)
    }

    fn advance(&mut self) -> &Token {
        let t = &self.tokens[self.pos];
        if !matches!(t.kind, TokenKind::Eof) {
            self.pos += 1;
        }
        t
    }

    /// Consome tokens até o próximo ';' ou newline implícito (fim de statement),
    /// respeitando parênteses e colchetes aninhados.
    fn skip_to_end_of_stmt(&mut self) {
        let mut paren = 0i32;
        loop {
            let kind = self.peek().kind.clone();
            match kind {
                TokenKind::Eof => break,
                TokenKind::Semicolon if paren == 0 => { self.advance(); break; }
                TokenKind::LParen | TokenKind::LBracket => { paren += 1; self.advance(); }
                TokenKind::RParen | TokenKind::RBracket => {
                    paren -= 1;
                    self.advance();
                    // após ')' sem ';', se paren==0 e o próximo é '{' ou nova linha, para
                    if paren == 0 {
                        let next = self.peek().kind.clone();
                        if matches!(next, TokenKind::LBrace | TokenKind::Eof) {
                            break;
                        }
                    }
                }
                // { e } só terminam o statement quando não há parênteses abertos.
                // Dentro de parênteses, { } são indexadores Pawn (ex: arr{i}).
                TokenKind::LBrace | TokenKind::RBrace if paren == 0 => break,
                _ => { self.advance(); }
            }
        }
    }

    /// Consome uma lista de tokens que compõem a condição/header de um controle
    /// (o `(...)` após if/while/for/switch), retornando ao caller após o `)`.
    fn skip_paren_expr(&mut self) {
        if !matches!(self.peek().kind, TokenKind::LParen) { return; }
        let mut depth = 0i32;
        loop {
            match self.peek().kind.clone() {
                TokenKind::Eof => break,
                TokenKind::LParen => { depth += 1; self.advance(); }
                TokenKind::RParen => {
                    depth -= 1;
                    self.advance();
                    if depth == 0 { break; }
                }
                _ => { self.advance(); }
            }
        }
    }

    fn push(&mut self, kind: StmtKind, tok: &Token) {
        self.stmts.push(Stmt {
            kind,
            line: tok.line,
            col: tok.col,
            stmt_indent: tok.stmt_indent,
            depth: self.depth,
            paren_depth: self.paren_depth,
            value: tok.value.clone(),
        });
    }

    fn parse_one(&mut self) {
        if matches!(self.peek().kind, TokenKind::LineContinuation) {
            self.advance();
            return;
        }
        if matches!(self.peek().kind, TokenKind::Semicolon) {
            self.advance();
            return;
        }
        if matches!(self.peek().kind, TokenKind::LBrace) {
            let tok = self.advance().clone();
            self.push(StmtKind::BlockOpen, &tok);
            self.depth += 1;
            return;
        }
        if matches!(self.peek().kind, TokenKind::RBrace) {
            if self.depth > 0 { self.depth -= 1; }
            let tok = self.advance().clone();
            self.push(StmtKind::BlockClose, &tok);
            return;
        }
        if matches!(self.peek().kind, TokenKind::Hash) {
            self.parse_directive();
            return;
        }
        if matches!(self.peek().kind, TokenKind::Ident) {
            self.parse_ident_stmt();
            return;
        }
        let tok = self.advance().clone();
        self.push(StmtKind::Expr, &tok);
        self.skip_to_end_of_stmt();
    }

    fn parse_directive(&mut self) {
        let hash_tok = self.advance().clone();
        if !matches!(self.peek().kind, TokenKind::Directive) {
            return;
        }
        let dir = self.advance().clone();
        let kind = match dir.value.as_str() {
            "include" | "tryinclude" => StmtKind::Include,
            "define"                 => StmtKind::Define,
            _                        => StmtKind::Pragma,
        };
        self.push(kind, &hash_tok);
    }

    fn parse_ident_stmt(&mut self) {
        let tok = self.peek().clone();
        let kw = tok.value.as_str();

        match kw {
            "if" => {
                let t = self.advance().clone();
                self.push(StmtKind::If, &t);
                self.skip_paren_expr();
            }
            "else" => {
                let t = self.advance().clone();
                self.push(StmtKind::Else, &t);
            }
            "while" => {
                let t = self.advance().clone();
                self.push(StmtKind::While, &t);
                self.skip_paren_expr();
            }
            "do" => {
                let t = self.advance().clone();
                self.push(StmtKind::Do, &t);
            }
            "for" => {
                let t = self.advance().clone();
                self.push(StmtKind::For, &t);
                self.skip_paren_expr();
            }
            "switch" => {
                let t = self.advance().clone();
                self.push(StmtKind::Switch, &t);
                self.skip_paren_expr();
            }
            "case" => {
                let t = self.advance().clone();
                self.push(StmtKind::Case, &t);
                self.skip_to_end_of_stmt();
            }
            "default" => {
                let t = self.advance().clone();
                self.push(StmtKind::Default, &t);
            }
            "return" => {
                let t = self.advance().clone();
                self.push(StmtKind::Return, &t);
                self.skip_to_end_of_stmt();
            }
            "break" => {
                let t = self.advance().clone();
                self.push(StmtKind::Break, &t);
                self.skip_to_end_of_stmt();
            }
            "continue" => {
                let t = self.advance().clone();
                self.push(StmtKind::Continue, &t);
                self.skip_to_end_of_stmt();
            }
            "exit" => {
                let t = self.advance().clone();
                self.push(StmtKind::Exit, &t);
                self.skip_to_end_of_stmt();
            }
            "goto" => {
                let t = self.advance().clone();
                self.push(StmtKind::Goto, &t);
                self.skip_to_end_of_stmt();
            }

            "native" | "forward" | "public" | "stock" | "static" => {
                let t = self.advance().clone();
                // Consome qualificadores adicionais (ex: "static stock", "public stock")
                while matches!(self.peek().kind, TokenKind::Ident)
                    && matches!(
                        self.peek().value.as_str(),
                        "native" | "forward" | "public" | "stock" | "static" | "const"
                    )
                {
                    self.advance();
                }
                self.push(StmtKind::FuncDecl, &t);
                self.skip_to_end_of_stmt();
            }
            "new" => {
                let t = self.advance().clone();
                self.push(StmtKind::VarDecl, &t);
                self.skip_to_end_of_stmt();
            }
            "const" => {
                let t = self.advance().clone();
                self.push(StmtKind::ConstDecl, &t);
                self.skip_to_end_of_stmt();
            }
            "enum" => {
                let t = self.advance().clone();
                self.push(StmtKind::EnumDecl, &t);
                self.skip_to_end_of_stmt();
            }

            _ => {
                let t = self.advance().clone();

                // Label: ':' simples, não '::' (namespace)
                if matches!(self.peek().kind, TokenKind::Colon) {
                    self.advance();
                    self.push(StmtKind::Label, &t);
                    return;
                }
                if matches!(self.peek().kind, TokenKind::ColonColon) {
                    self.push(StmtKind::Expr, &t);
                    self.skip_to_end_of_stmt();
                    return;
                }

                self.push(StmtKind::Expr, &t);
                self.skip_to_end_of_stmt();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::token_lexer::tokenize;

    fn parse(src: &str) -> Vec<(StmtKind, u32, u32)> {
        let stream = tokenize(src);
        let tree = parse_stmts(stream);
        tree.stmts.into_iter().map(|s| (s.kind, s.stmt_indent, s.depth)).collect()
    }

    #[test]
    fn basic_function_body() {
        let src = "public OnInit()\n{\n    foo();\n    bar();\n}";
        let stmts = parse(src);
        assert!(stmts.iter().any(|(k, _, _)| *k == StmtKind::FuncDecl));
        assert!(stmts.iter().any(|(k, _, _)| *k == StmtKind::BlockOpen));
        assert_eq!(stmts.iter().filter(|(k, _, _)| *k == StmtKind::Expr).count(), 2);
        assert!(stmts.iter().any(|(k, _, _)| *k == StmtKind::BlockClose));
    }

    #[test]
    fn if_else_depth() {
        let src = "if (x)\n{\n    foo();\n}\nelse\n{\n    bar();\n}";
        let stmts = parse(src);
        let if_stmt = stmts.iter().find(|(k, _, _)| *k == StmtKind::If).unwrap();
        let else_stmt = stmts.iter().find(|(k, _, _)| *k == StmtKind::Else).unwrap();
        assert_eq!(if_stmt.2, 0);  // depth 0
        assert_eq!(else_stmt.2, 0); // depth 0
    }

    #[test]
    fn stmt_indent_inside_block() {
        let src = "main()\n{\n    foo();\n    bar();\n}";
        let stmts = parse(src);
        // Expr dentro do bloco (depth=1): foo e bar
        let exprs: Vec<_> = stmts.iter()
            .filter(|(k, _, d)| *k == StmtKind::Expr && *d == 1)
            .collect();
        assert_eq!(exprs.len(), 2);
        assert!(exprs.iter().all(|(_, indent, _)| *indent == 4));
    }

    #[test]
    fn loose_indent_detected() {
        // foo() com indent=4, bar() com indent=8 — indentação solta
        let src = "main()\n{\n    foo();\n        bar();\n}";
        let stmts = parse(src);
        let exprs: Vec<_> = stmts.iter()
            .filter(|(k, _, d)| *k == StmtKind::Expr && *d == 1)
            .collect();
        assert_eq!(exprs.len(), 2);
        assert_eq!(exprs[0].1, 4);
        assert_eq!(exprs[1].1, 8);
        assert!(exprs.iter().all(|(_, _, d)| *d == 1));
    }

    #[test]
    fn label_detected() {
        let src = "main()\n{\n    myLabel:\n    foo();\n}";
        let stream = tokenize(src);
        let tree = parse_stmts(stream);
        // debug: mostra os statements produzidos
        for s in &tree.stmts {
            eprintln!("kind={:?} val={:?} indent={} depth={}", s.kind, s.value, s.stmt_indent, s.depth);
        }
        assert!(tree.stmts.iter().any(|s| s.kind == StmtKind::Label));
    }

    #[test]
    fn include_define_pragma() {
        let src = "#include <a_samp>\n#define X 1\n#pragma tabsize 4\n";
        let stmts = parse(src);
        assert!(stmts.iter().any(|(k, _, _)| *k == StmtKind::Include));
        assert!(stmts.iter().any(|(k, _, _)| *k == StmtKind::Define));
        assert!(stmts.iter().any(|(k, _, _)| *k == StmtKind::Pragma));
    }

    #[test]
    fn nested_blocks_depth() {
        let src = "main()\n{\n    if (x)\n    {\n        foo();\n    }\n}";
        let stmts = parse(src);
        // foo() está em depth=2: main{} + if{}
        let foo = stmts.iter().find(|(k, _, d)| *k == StmtKind::Expr && *d == 2).unwrap();
        assert_eq!(foo.2, 2);
    }

    #[test]
    fn return_stmt() {
        let src = "main()\n{\n    return 1;\n}";
        let stmts = parse(src);
        assert!(stmts.iter().any(|(k, _, _)| *k == StmtKind::Return));
    }

    #[test]
    fn pragma_tabsize_propagated() {
        let src = "#pragma tabsize 4\nmain()\n{\n\tfoo();\n}";
        let stream = tokenize(src);
        let tree = parse_stmts(stream);
        assert_eq!(tree.tabsize, 4);
        // foo() está em depth=1 (dentro de main{})
        let foo = tree.stmts.iter().find(|s| s.kind == StmtKind::Expr && s.depth == 1).unwrap();
        assert_eq!(foo.stmt_indent, 4);
    }
}
