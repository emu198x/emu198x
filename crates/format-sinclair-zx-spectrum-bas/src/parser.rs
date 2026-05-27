//! Recursive descent parser: plain-text BASIC → AST.

use crate::ast::*;
use crate::tokens::{KeywordRole, KEYWORDS};

pub fn parse_program(source: &str) -> Result<Program, String> {
    let mut lines = Vec::new();
    for (line_idx, raw_line) in source.lines().enumerate() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let line = parse_line(trimmed, line_idx + 1)?;
        lines.push(line);
    }
    Ok(Program { lines })
}

fn parse_line(text: &str, source_line: usize) -> Result<Line, String> {
    let bytes = text.as_bytes();
    let mut pos = 0;

    while pos < bytes.len() && bytes[pos] == b' ' {
        pos += 1;
    }
    let num_start = pos;
    while pos < bytes.len() && bytes[pos].is_ascii_digit() {
        pos += 1;
    }
    if pos == num_start {
        return Err(format!("line {source_line}: missing line number"));
    }
    let num_str = &text[num_start..pos];
    let number: u32 = num_str
        .parse()
        .map_err(|_| format!("line {source_line}: invalid line number"))?;
    if number == 0 || number > 9999 {
        return Err(format!(
            "line {source_line}: line number {number} out of range 1–9999"
        ));
    }

    while pos < bytes.len() && bytes[pos] == b' ' {
        pos += 1;
    }

    let mut parser = Parser::new(&text[pos..]);
    let statements = parser.parse_statement_list();
    Ok(Line {
        number: number as u16,
        statements,
    })
}

struct Parser<'a> {
    src: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            src: text.as_bytes(),
            pos: 0,
        }
    }

    fn at_end(&self) -> bool {
        self.pos >= self.src.len()
    }

    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    fn advance(&mut self) -> u8 {
        let ch = self.src[self.pos];
        self.pos += 1;
        ch
    }

    fn skip_spaces(&mut self) {
        while self.peek() == Some(b' ') {
            self.pos += 1;
        }
    }

    fn try_consume(&mut self, ch: u8) -> bool {
        if self.peek() == Some(ch) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    // ── Keyword matching ────────────────────────────────────────────

    fn match_keyword(&self, roles: &[KeywordRole]) -> Option<(u8, usize)> {
        let text = &self.src[self.pos..];
        for &(keyword, token, role) in KEYWORDS {
            if !roles.contains(&role) {
                continue;
            }
            let kw = keyword.as_bytes();
            let has_trailing_space = kw.last() == Some(&b' ');
            let core_len = if has_trailing_space {
                kw.len() - 1
            } else {
                kw.len()
            };
            if text.len() < core_len {
                continue;
            }
            if !text[..core_len].eq_ignore_ascii_case(&kw[..core_len]) {
                continue;
            }
            let last_core = kw[core_len - 1];
            if last_core.is_ascii_alphabetic() {
                if let Some(&next) = text.get(core_len) {
                    if next.is_ascii_alphanumeric() || next == b'$' {
                        continue;
                    }
                }
            }
            let consume = if has_trailing_space
                && text.len() > core_len
                && text[core_len] == b' '
            {
                core_len + 1
            } else {
                core_len
            };
            return Some((token, consume));
        }
        None
    }

    fn try_match_token(&mut self, token: u8) -> bool {
        let text = &self.src[self.pos..];
        for &(keyword, t, _) in KEYWORDS {
            if t != token {
                continue;
            }
            let kw = keyword.as_bytes();
            let has_trailing_space = kw.last() == Some(&b' ');
            let core_len = if has_trailing_space {
                kw.len() - 1
            } else {
                kw.len()
            };
            if text.len() < core_len {
                return false;
            }
            if !text[..core_len].eq_ignore_ascii_case(&kw[..core_len]) {
                return false;
            }
            let last_core = kw[core_len - 1];
            if last_core.is_ascii_alphabetic() {
                if let Some(&next) = text.get(core_len) {
                    if next.is_ascii_alphanumeric() || next == b'$' {
                        return false;
                    }
                }
            }
            let consume = if has_trailing_space
                && text.len() > core_len
                && text[core_len] == b' '
            {
                core_len + 1
            } else {
                core_len
            };
            self.pos += consume;
            return true;
        }
        false
    }

    /// Match a keyword that is valid at a statement boundary.
    fn match_statement_keyword(&self) -> Option<(u8, usize)> {
        use KeywordRole::*;
        self.match_keyword(&[Statement, PrintItem, RestOfLine, Expression, SubKeyword, Symbol])
    }

    fn match_expression_keyword(&self) -> Option<(u8, usize)> {
        self.match_keyword(&[KeywordRole::Expression, KeywordRole::Symbol])
    }

    fn match_print_item_keyword(&self) -> Option<(u8, usize)> {
        self.match_keyword(&[KeywordRole::PrintItem])
    }

    // ── Statement parsing ───────────────────────────────────────────

    fn parse_statement_list(&mut self) -> Vec<Statement> {
        let mut stmts = Vec::new();
        stmts.push(self.parse_statement());
        while self.try_consume(b':') {
            stmts.push(self.parse_statement());
        }
        stmts
    }

    fn parse_statement(&mut self) -> Statement {
        self.skip_spaces();
        if self.at_end() {
            return Statement::Expression(Expr::Number(0.0));
        }

        if let Some((token, len)) = self.match_statement_keyword() {
            self.pos += len;
            match token {
                0xF1 => self.parse_let(),
                0xEB => self.parse_for(),
                0xF3 => self.parse_next(),
                0xFA => self.parse_if(),
                0xE9 => self.parse_dim(),
                0xE3 => self.parse_read(),
                0xF5 => Statement::Print { items: self.parse_print_list() },
                0xE0 => Statement::LPrint { items: self.parse_print_list() },
                0xEE => Statement::Input { items: self.parse_print_list() },
                0xE4 => self.parse_data(),
                0xEA => self.parse_rem(),
                0xEC => Statement::GoTo(self.parse_expr(0)),
                0xED => Statement::GoSub(self.parse_expr(0)),
                0xFE => Statement::Return,
                0xE2 => Statement::Stop,
                0xFB => Statement::Cls,
                0xE6 => Statement::New,
                0xFF => Statement::Copy,
                0xE8 => Statement::Continue,
                0xF2 => Statement::Pause(self.parse_expr(0)),
                0xE7 => Statement::Border(self.parse_expr(0)),
                0xD9 => Statement::Ink(self.parse_expr(0)),
                0xDA => Statement::Paper(self.parse_expr(0)),
                0xDC => Statement::Bright(self.parse_expr(0)),
                0xDB => Statement::Flash(self.parse_expr(0)),
                0xDE => Statement::Over(self.parse_expr(0)),
                0xDD => Statement::Inverse(self.parse_expr(0)),
                0xD7 => self.parse_beep(),
                0xF6 => self.parse_plot(),
                0xFC => self.parse_draw(),
                0xD8 => self.parse_circle(),
                0xF4 => self.parse_poke(),
                0xDF => self.parse_out(),
                0xF8 => self.parse_save(),
                0xEF => self.parse_load(),
                0xD5 => Statement::Merge(self.parse_expr(0)),
                0xD6 => self.parse_verify(),
                0xFD => self.parse_optional_expr(Statement::Clear),
                0xF7 => self.parse_optional_expr(Statement::Run),
                0xE5 => self.parse_optional_expr(Statement::Restore),
                0xF9 => self.parse_optional_expr(Statement::Randomize),
                0xCF => self.parse_optional_expr(Statement::Cat),
                0xF0 => self.parse_optional_expr(Statement::List),
                0xE1 => self.parse_optional_expr(Statement::LList),
                0xD0 => Statement::Format(self.parse_expr(0)),
                0xD1 => Statement::Move(self.parse_expr(0)),
                0xD2 => Statement::Erase(self.parse_expr(0)),
                0xCE => self.parse_def_fn(),
                0xD3 => self.parse_open_hash(),
                0xD4 => Statement::CloseHash(self.parse_expr(0)),
                _ => Statement::Expression(self.parse_expr(0)),
            }
        } else {
            Statement::Expression(self.parse_expr(0))
        }
    }

    fn parse_let(&mut self) -> Statement {
        let target = self.parse_variable_expr();
        self.skip_spaces();
        self.try_consume(b'=');
        Statement::Let {
            target,
            value: self.parse_expr(0),
        }
    }

    fn parse_for(&mut self) -> Statement {
        let variable = self.parse_variable();
        self.skip_spaces();
        self.try_consume(b'=');
        let from = self.parse_expr_until_token(0xCC); // TO
        self.try_match_token(0xCC);
        let to = self.parse_expr_until_tokens(&[0xCD]); // STEP
        let step = if self.try_match_token(0xCD) {
            Some(self.parse_expr(0))
        } else {
            None
        };
        Statement::For {
            variable,
            from,
            to,
            step,
        }
    }

    fn parse_next(&mut self) -> Statement {
        self.skip_spaces();
        let variable = if !self.at_end() && self.peek() != Some(b':') {
            Some(self.parse_variable())
        } else {
            None
        };
        Statement::Next { variable }
    }

    fn parse_if(&mut self) -> Statement {
        let condition = self.parse_expr_until_token(0xCB); // THEN
        self.try_match_token(0xCB);
        let then_body = self.parse_statement_list();
        Statement::If {
            condition,
            then_body,
        }
    }

    fn parse_dim(&mut self) -> Statement {
        let variable = self.parse_variable();
        self.skip_spaces();
        let mut dimensions = Vec::new();
        if self.try_consume(b'(') {
            dimensions.push(self.parse_expr(0));
            while self.try_consume(b',') {
                dimensions.push(self.parse_expr(0));
            }
            self.try_consume(b')');
        }
        Statement::Dim {
            variable,
            dimensions,
        }
    }

    fn parse_read(&mut self) -> Statement {
        let mut targets = Vec::new();
        targets.push(self.parse_variable_expr());
        while self.try_consume(b',') {
            targets.push(self.parse_variable_expr());
        }
        Statement::Read { targets }
    }

    fn parse_data(&mut self) -> Statement {
        let mut items = Vec::new();
        loop {
            self.skip_spaces();
            if self.at_end() || self.peek() == Some(b':') {
                break;
            }
            if self.peek() == Some(b'"') {
                items.push(DataItem::String(self.consume_string_contents()));
            } else {
                let start = self.pos;
                while !self.at_end()
                    && self.peek() != Some(b',')
                    && self.peek() != Some(b':')
                {
                    self.pos += 1;
                }
                let text = String::from_utf8_lossy(&self.src[start..self.pos])
                    .trim()
                    .to_string();
                if let Ok(n) = text.parse::<f64>() {
                    items.push(DataItem::Number(n));
                } else {
                    items.push(DataItem::Raw(text));
                }
            }
            if !self.try_consume(b',') {
                break;
            }
        }
        Statement::Data { items }
    }

    fn parse_rem(&mut self) -> Statement {
        let start = self.pos;
        self.pos = self.src.len();
        Statement::Rem(
            String::from_utf8_lossy(&self.src[start..]).to_string(),
        )
    }

    fn parse_beep(&mut self) -> Statement {
        let duration = self.parse_expr(0);
        self.skip_spaces();
        self.try_consume(b',');
        let pitch = self.parse_expr(0);
        Statement::Beep { duration, pitch }
    }

    fn parse_plot(&mut self) -> Statement {
        let x = self.parse_expr(0);
        self.skip_spaces();
        self.try_consume(b',');
        let y = self.parse_expr(0);
        Statement::Plot { x, y }
    }

    fn parse_draw(&mut self) -> Statement {
        let dx = self.parse_expr(0);
        self.skip_spaces();
        self.try_consume(b',');
        let dy = self.parse_expr(0);
        self.skip_spaces();
        let angle = if self.try_consume(b',') {
            Some(self.parse_expr(0))
        } else {
            None
        };
        Statement::Draw { dx, dy, angle }
    }

    fn parse_circle(&mut self) -> Statement {
        let x = self.parse_expr(0);
        self.skip_spaces();
        self.try_consume(b',');
        let y = self.parse_expr(0);
        self.skip_spaces();
        self.try_consume(b',');
        let radius = self.parse_expr(0);
        Statement::Circle { x, y, radius }
    }

    fn parse_poke(&mut self) -> Statement {
        let address = self.parse_expr(0);
        self.skip_spaces();
        self.try_consume(b',');
        let value = self.parse_expr(0);
        Statement::Poke { address, value }
    }

    fn parse_out(&mut self) -> Statement {
        let port = self.parse_expr(0);
        self.skip_spaces();
        self.try_consume(b',');
        let value = self.parse_expr(0);
        Statement::Out { port, value }
    }

    fn parse_save(&mut self) -> Statement {
        let filename = self.parse_expr(0);
        let qualifier = self.parse_tape_qualifier();
        Statement::Save {
            filename,
            qualifier,
        }
    }

    fn parse_load(&mut self) -> Statement {
        let filename = self.parse_expr(0);
        let qualifier = self.parse_tape_qualifier();
        Statement::Load {
            filename,
            qualifier,
        }
    }

    fn parse_verify(&mut self) -> Statement {
        let filename = self.parse_expr(0);
        let qualifier = self.parse_tape_qualifier();
        Statement::Verify {
            filename,
            qualifier,
        }
    }

    fn parse_tape_qualifier(&mut self) -> Option<TapeQualifier> {
        self.skip_spaces();
        if self.try_match_token(0xCA) {
            // LINE
            return Some(TapeQualifier::Line(self.parse_expr(0)));
        }
        if self.try_match_token(0xE4) {
            // DATA
            let var = self.parse_variable();
            self.skip_spaces();
            self.try_consume(b'(');
            self.try_consume(b')');
            return Some(TapeQualifier::Data(var));
        }
        if self.try_match_token(0xAF) {
            // CODE
            let start = self.parse_expr(0);
            self.skip_spaces();
            self.try_consume(b',');
            let length = self.parse_expr(0);
            return Some(TapeQualifier::Code { start, length });
        }
        if self.try_match_token(0xAA) {
            // SCREEN$
            return Some(TapeQualifier::Screen);
        }
        None
    }

    fn parse_def_fn(&mut self) -> Statement {
        self.skip_spaces();
        let name = if !self.at_end() && self.peek().unwrap_or(0).is_ascii_alphabetic() {
            (self.advance() as char).to_ascii_lowercase()
        } else {
            'a'
        };
        self.skip_spaces();
        let mut params = Vec::new();
        if self.try_consume(b'(') {
            if self.peek() != Some(b')') {
                params.push(self.parse_variable());
                while self.try_consume(b',') {
                    params.push(self.parse_variable());
                }
            }
            self.try_consume(b')');
        }
        self.skip_spaces();
        self.try_consume(b'=');
        let body = self.parse_expr(0);
        Statement::DefFn { name, params, body }
    }

    fn parse_open_hash(&mut self) -> Statement {
        let channel = self.parse_expr(0);
        self.skip_spaces();
        self.try_consume(b',');
        let args = self.parse_expr(0);
        Statement::OpenHash { channel, args }
    }

    fn parse_optional_expr<F>(&mut self, ctor: F) -> Statement
    where
        F: FnOnce(Option<Expr>) -> Statement,
    {
        self.skip_spaces();
        if self.at_end() || self.peek() == Some(b':') {
            ctor(None)
        } else {
            ctor(Some(self.parse_expr(0)))
        }
    }

    // ── PRINT list ──────────────────────────────────────────────────

    fn parse_print_list(&mut self) -> Vec<PrintItem> {
        let mut items = Vec::new();
        loop {
            self.skip_spaces();
            if self.at_end() || self.peek() == Some(b':') {
                break;
            }

            if self.try_consume(b';') {
                items.push(PrintItem::Separator(PrintSep::Semicolon));
                continue;
            }
            if self.try_consume(b',') {
                items.push(PrintItem::Separator(PrintSep::Comma));
                continue;
            }
            if self.try_consume(b'\'') {
                items.push(PrintItem::Separator(PrintSep::Apostrophe));
                continue;
            }

            if let Some((token, len)) = self.match_print_item_keyword() {
                self.pos += len;
                let item = match token {
                    0xD9 => PrintItem::Ink(self.parse_expr(0)),
                    0xDA => PrintItem::Paper(self.parse_expr(0)),
                    0xDC => PrintItem::Bright(self.parse_expr(0)),
                    0xDB => PrintItem::Flash(self.parse_expr(0)),
                    0xDE => PrintItem::Over(self.parse_expr(0)),
                    0xDD => PrintItem::Inverse(self.parse_expr(0)),
                    0xAC => {
                        let row = self.parse_expr(0);
                        self.skip_spaces();
                        self.try_consume(b',');
                        let col = self.parse_expr(0);
                        PrintItem::At { row, col }
                    }
                    0xAD => PrintItem::Tab(self.parse_expr(0)),
                    _ => PrintItem::Expr(self.parse_expr(0)),
                };
                items.push(item);
            } else {
                items.push(PrintItem::Expr(self.parse_expr_for_print()));
            }
        }
        items
    }

    /// Parse an expression in PRINT context — stops at `;`, `,`, `'`,
    /// and before PrintItem keywords.
    fn parse_expr_for_print(&mut self) -> Expr {
        self.parse_expr_with_stops(&[b';', b',', b'\''], true)
    }

    // ── Variable parsing ────────────────────────────────────────────

    fn parse_variable(&mut self) -> Variable {
        self.skip_spaces();
        let start = self.pos;
        while !self.at_end() {
            let ch = self.src[self.pos];
            if ch.is_ascii_alphanumeric() || ch == b'_' {
                self.pos += 1;
            } else {
                break;
            }
        }
        let is_string = self.try_consume(b'$');
        let name = String::from_utf8_lossy(&self.src[start..self.pos - if is_string { 0 } else { 0 }])
            .to_string();
        Variable {
            name: if is_string { name.trim_end_matches('$').to_string() } else { name },
            is_string,
        }
    }

    // ── Expression parsing (Pratt) ──────────────────────────────────

    fn parse_expr(&mut self, min_bp: u8) -> Expr {
        self.parse_expr_inner(min_bp, &[], false)
    }

    fn parse_expr_with_stops(
        &mut self,
        stop_bytes: &[u8],
        stop_at_print_items: bool,
    ) -> Expr {
        self.parse_expr_inner(0, stop_bytes, stop_at_print_items)
    }

    fn parse_expr_until_token(&mut self, token: u8) -> Expr {
        let save = self.pos;
        let expr = self.parse_expr_inner_with_stop_token(0, token);
        if let Some(e) = expr {
            e
        } else {
            self.pos = save;
            self.parse_expr(0)
        }
    }

    fn parse_expr_until_tokens(&mut self, tokens: &[u8]) -> Expr {
        // Parse expression, stopping before any of the given keyword tokens
        self.parse_expr_inner_with_stop_tokens(0, tokens)
    }

    fn parse_expr_inner(
        &mut self,
        min_bp: u8,
        stop_bytes: &[u8],
        stop_at_print_items: bool,
    ) -> Expr {
        self.skip_spaces();

        if stop_at_print_items {
            if let Some(_) = self.match_print_item_keyword() {
                return Expr::Number(0.0);
            }
        }

        let mut left = self.parse_prefix(stop_bytes, stop_at_print_items);

        loop {
            self.skip_spaces();
            if self.at_end() {
                break;
            }
            if let Some(&ch) = self.src.get(self.pos) {
                if ch == b':' || stop_bytes.contains(&ch) {
                    break;
                }
            }
            if stop_at_print_items {
                if let Some(_) = self.match_print_item_keyword() {
                    break;
                }
            }

            if let Some((op, bp)) = self.peek_infix_op() {
                if bp < min_bp {
                    break;
                }
                self.consume_op(&op);
                let right_bp = if op == BinOp::Pow { bp } else { bp + 1 };
                let right = self.parse_expr_inner(right_bp, stop_bytes, stop_at_print_items);
                left = Expr::BinaryOp {
                    left: Box::new(left),
                    op,
                    right: Box::new(right),
                };
            } else {
                break;
            }
        }

        left
    }

    fn parse_expr_inner_with_stop_token(&mut self, min_bp: u8, stop_token: u8) -> Option<Expr> {
        self.skip_spaces();
        if self.at_end() {
            return Some(Expr::Number(0.0));
        }

        let mut left = self.parse_prefix(&[], false);

        loop {
            self.skip_spaces();
            if self.at_end() || self.peek() == Some(b':') {
                break;
            }
            // Check stop token
            let saved = self.pos;
            if self.try_match_token(stop_token) {
                self.pos = saved; // don't consume — caller handles it
                break;
            }

            if let Some((op, bp)) = self.peek_infix_op() {
                if bp < min_bp {
                    break;
                }
                self.consume_op(&op);
                let right_bp = if op == BinOp::Pow { bp } else { bp + 1 };
                let right = self
                    .parse_expr_inner_with_stop_token(right_bp, stop_token)
                    .unwrap_or(Expr::Number(0.0));
                left = Expr::BinaryOp {
                    left: Box::new(left),
                    op,
                    right: Box::new(right),
                };
            } else {
                break;
            }
        }

        Some(left)
    }

    fn parse_expr_inner_with_stop_tokens(&mut self, min_bp: u8, stop_tokens: &[u8]) -> Expr {
        self.skip_spaces();
        if self.at_end() {
            return Expr::Number(0.0);
        }

        let mut left = self.parse_prefix(&[], false);

        loop {
            self.skip_spaces();
            if self.at_end() || self.peek() == Some(b':') {
                break;
            }
            let saved = self.pos;
            let mut hit_stop = false;
            for &tok in stop_tokens {
                if self.try_match_token(tok) {
                    self.pos = saved;
                    hit_stop = true;
                    break;
                }
            }
            if hit_stop {
                break;
            }

            if let Some((op, bp)) = self.peek_infix_op() {
                if bp < min_bp {
                    break;
                }
                self.consume_op(&op);
                let right_bp = if op == BinOp::Pow { bp } else { bp + 1 };
                let right = self.parse_expr_inner_with_stop_tokens(right_bp, stop_tokens);
                left = Expr::BinaryOp {
                    left: Box::new(left),
                    op,
                    right: Box::new(right),
                };
            } else {
                break;
            }
        }

        left
    }

    fn parse_prefix(&mut self, stop_bytes: &[u8], stop_at_print_items: bool) -> Expr {
        self.skip_spaces();
        if self.at_end() {
            return Expr::Number(0.0);
        }
        let ch = self.src[self.pos];

        // Unary minus
        if ch == b'-' {
            self.pos += 1;
            let operand = self.parse_expr_inner(14, stop_bytes, stop_at_print_items);
            return Expr::UnaryOp {
                op: UnaryOp::Neg,
                operand: Box::new(operand),
            };
        }

        // NOT
        if let Some((0xC3, len)) = self.match_expression_keyword() {
            self.pos += len;
            let operand = self.parse_expr_inner(5, stop_bytes, stop_at_print_items);
            return Expr::UnaryOp {
                op: UnaryOp::Not,
                operand: Box::new(operand),
            };
        }

        // Parenthesised expression
        if ch == b'(' {
            self.pos += 1;
            let inner = self.parse_expr(0);
            self.skip_spaces();
            self.try_consume(b')');
            return Expr::Paren(Box::new(inner));
        }

        // String literal
        if ch == b'"' {
            return Expr::StringLiteral(self.consume_string_contents());
        }

        // Number
        if ch.is_ascii_digit()
            || (ch == b'.' && self.pos + 1 < self.src.len() && self.src[self.pos + 1].is_ascii_digit())
        {
            return self.parse_number_literal();
        }

        // Expression-level keyword (function or constant)
        if let Some((token, len)) = self.match_expression_keyword() {
            self.pos += len;
            return self.parse_function_or_constant(token);
        }

        // Variable (possibly with array index or string slice)
        if ch.is_ascii_alphabetic() {
            return self.parse_variable_expr();
        }

        // Unknown — skip and return a placeholder
        self.pos += 1;
        Expr::Number(0.0)
    }

    fn parse_function_or_constant(&mut self, token: u8) -> Expr {
        match token {
            0xA7 => Expr::Pi,
            0xA5 => Expr::Rnd,
            0xA6 => Expr::Inkey,
            0xA8 => return self.parse_fn_call(), // FN
            0xC4 => {
                // BIN — consume binary digits
                self.skip_spaces();
                let start = self.pos;
                while !self.at_end() && (self.src[self.pos] == b'0' || self.src[self.pos] == b'1') {
                    self.pos += 1;
                }
                let bits = &self.src[start..self.pos];
                let val = bits.iter().fold(0u32, |acc, &b| acc * 2 + (b - b'0') as u32);
                Expr::Number(val as f64)
            }
            _ => {
                let func = match token {
                    0xBD => BuiltinFn::Abs,
                    0xB6 => BuiltinFn::Acs,
                    0xB5 => BuiltinFn::Asn,
                    0xB7 => BuiltinFn::Atn,
                    0xAB => BuiltinFn::Attr,
                    0xC2 => BuiltinFn::Chr,
                    0xAF => BuiltinFn::Code,
                    0xB3 => BuiltinFn::Cos,
                    0xB9 => BuiltinFn::Exp,
                    0xBF => BuiltinFn::In,
                    0xBA => BuiltinFn::Int,
                    0xB1 => BuiltinFn::Len,
                    0xB8 => BuiltinFn::Ln,
                    0xBE => BuiltinFn::Peek,
                    0xA9 => BuiltinFn::Point,
                    0xAA => BuiltinFn::Screen,
                    0xBC => BuiltinFn::Sgn,
                    0xB2 => BuiltinFn::Sin,
                    0xBB => BuiltinFn::Sqr,
                    0xC1 => BuiltinFn::Str,
                    0xB4 => BuiltinFn::Tan,
                    0xC0 => BuiltinFn::Usr,
                    0xB0 => BuiltinFn::Val,
                    0xAE => BuiltinFn::ValDollar,
                    _ => BuiltinFn::Int,
                };
                // ATTR, POINT, and SCREEN$ take two comma-separated
                // arguments; all others take one.
                let two_arg = matches!(
                    func,
                    BuiltinFn::Attr | BuiltinFn::Point | BuiltinFn::Screen
                );
                let mut args = vec![self.parse_expr(13)];
                if two_arg && self.try_consume(b',') {
                    args.push(self.parse_expr(13));
                }
                Expr::Function { func, args }
            }
        }
    }

    fn parse_variable_expr(&mut self) -> Expr {
        let var = self.parse_variable();
        self.skip_spaces();
        if self.peek() == Some(b'(') {
            self.pos += 1;
            let mut indices = Vec::new();

            // Could be array index or string slice (a$(1 TO 3))
            let first = self.parse_expr_until_tokens(&[0xCC]); // TO
            if self.try_match_token(0xCC) {
                // String slice: var$(from TO to)
                let to = self.parse_expr(0);
                self.skip_spaces();
                self.try_consume(b')');
                return Expr::Slice {
                    string: Box::new(Expr::Variable(var)),
                    from: Box::new(first),
                    to: Box::new(to),
                };
            }
            indices.push(first);
            while self.try_consume(b',') {
                indices.push(self.parse_expr(0));
            }
            self.skip_spaces();
            self.try_consume(b')');
            Expr::ArrayIndex { name: var, indices }
        } else {
            Expr::Variable(var)
        }
    }

    fn parse_number_literal(&mut self) -> Expr {
        let start = self.pos;
        while !self.at_end() && (self.src[self.pos].is_ascii_digit() || self.src[self.pos] == b'.') {
            self.pos += 1;
        }
        if !self.at_end()
            && (self.src[self.pos] == b'e' || self.src[self.pos] == b'E')
        {
            self.pos += 1;
            if !self.at_end() && (self.src[self.pos] == b'+' || self.src[self.pos] == b'-') {
                self.pos += 1;
            }
            while !self.at_end() && self.src[self.pos].is_ascii_digit() {
                self.pos += 1;
            }
        }
        let num_str = std::str::from_utf8(&self.src[start..self.pos]).unwrap_or("0");
        let value = num_str.parse::<f64>().unwrap_or(0.0);
        Expr::Number(value)
    }

    fn consume_string_contents(&mut self) -> String {
        self.try_consume(b'"');
        let start = self.pos;
        while !self.at_end() && self.src[self.pos] != b'"' {
            self.pos += 1;
        }
        let s = String::from_utf8_lossy(&self.src[start..self.pos]).to_string();
        self.try_consume(b'"');
        s
    }

    // ── Operator helpers ────────────────────────────────────────────

    fn peek_infix_op(&self) -> Option<(BinOp, u8)> {
        if self.at_end() {
            return None;
        }

        // Check for keyword operators (AND, OR)
        if let Some((token, _)) = self.match_expression_keyword() {
            return match token {
                0xC5 => Some((BinOp::Or, 2)),
                0xC6 => Some((BinOp::And, 4)),
                _ => None,
            };
        }

        // Check for symbol operators
        let ch = self.src[self.pos];
        match ch {
            b'+' => Some((BinOp::Add, 8)),
            b'-' => Some((BinOp::Sub, 8)),
            b'*' => Some((BinOp::Mul, 10)),
            b'/' => Some((BinOp::Div, 10)),
            b'^' => Some((BinOp::Pow, 12)),
            b'=' => Some((BinOp::Eq, 6)),
            b'<' => {
                if self.pos + 1 < self.src.len() {
                    match self.src[self.pos + 1] {
                        b'>' => Some((BinOp::Ne, 6)),
                        b'=' => Some((BinOp::Le, 6)),
                        _ => Some((BinOp::Lt, 6)),
                    }
                } else {
                    Some((BinOp::Lt, 6))
                }
            }
            b'>' => {
                if self.pos + 1 < self.src.len() && self.src[self.pos + 1] == b'=' {
                    Some((BinOp::Ge, 6))
                } else {
                    Some((BinOp::Gt, 6))
                }
            }
            _ => None,
        }
    }

    fn consume_op(&mut self, op: &BinOp) {
        match op {
            BinOp::And | BinOp::Or => {
                // Keyword operator — use match to consume
                if let Some((_, len)) = self.match_expression_keyword() {
                    self.pos += len;
                }
            }
            BinOp::Ne | BinOp::Le | BinOp::Ge => {
                self.pos += 2;
            }
            _ => {
                self.pos += 1;
            }
        }
    }
}

// ── FN call parsing ─────────────────────────────────────────────────

impl<'a> Parser<'a> {
    fn parse_fn_call(&mut self) -> Expr {
        self.skip_spaces();
        let name = if !self.at_end() && self.src[self.pos].is_ascii_alphabetic() {
            let ch = (self.advance() as char).to_ascii_lowercase();
            ch
        } else {
            'a'
        };
        self.skip_spaces();
        let mut args = Vec::new();
        if self.try_consume(b'(') {
            if self.peek() != Some(b')') {
                args.push(self.parse_expr(0));
                while self.try_consume(b',') {
                    args.push(self.parse_expr(0));
                }
            }
            self.try_consume(b')');
        }
        Expr::FnCall { name, args }
    }
}
