//! Serialize an AST into the Spectrum's in-memory tokenized format.

use crate::ast::*;

/// Serialize a parsed program into Spectrum tokenized bytes.
pub fn serialize(program: &Program) -> Vec<u8> {
    let mut output = Vec::new();
    for line in &program.lines {
        serialize_line(line, &mut output);
    }
    output
}

fn serialize_line(line: &Line, output: &mut Vec<u8>) {
    // Line number: 2 bytes big-endian
    output.push((line.number >> 8) as u8);
    output.push(line.number as u8);

    // Placeholder for length (2 bytes little-endian) — patched after content
    let len_pos = output.len();
    output.push(0);
    output.push(0);

    // Serialize statements separated by ':'
    for (i, stmt) in line.statements.iter().enumerate() {
        if i > 0 {
            output.push(b':');
        }
        serialize_statement(stmt, output);
    }

    // Terminator
    output.push(0x0D);

    // Patch length (content + terminator, excluding the length field itself)
    let content_len = (output.len() - len_pos - 2) as u16;
    output[len_pos] = content_len as u8;
    output[len_pos + 1] = (content_len >> 8) as u8;
}

fn serialize_statement(stmt: &Statement, out: &mut Vec<u8>) {
    match stmt {
        Statement::Let { target, value } => {
            out.push(0xF1); // LET
            serialize_expr(target, out);
            out.push(b'=');
            serialize_expr(value, out);
        }
        Statement::For { variable, from, to, step } => {
            out.push(0xEB); // FOR
            serialize_variable(variable, out);
            out.push(b'=');
            serialize_expr(from, out);
            out.push(0xCC); // TO
            serialize_expr(to, out);
            if let Some(s) = step {
                out.push(0xCD); // STEP
                serialize_expr(s, out);
            }
        }
        Statement::Next { variable } => {
            out.push(0xF3); // NEXT
            if let Some(v) = variable {
                serialize_variable(v, out);
            }
        }
        Statement::If { condition, then_body } => {
            out.push(0xFA); // IF
            serialize_expr(condition, out);
            out.push(0xCB); // THEN
            for (i, s) in then_body.iter().enumerate() {
                if i > 0 {
                    out.push(b':');
                }
                serialize_statement(s, out);
            }
        }
        Statement::GoTo(expr) => {
            out.push(0xEC); // GO TO
            serialize_expr(expr, out);
        }
        Statement::GoSub(expr) => {
            out.push(0xED); // GO SUB
            serialize_expr(expr, out);
        }
        Statement::Return => out.push(0xFE),
        Statement::Stop => out.push(0xE2),
        Statement::Print { items } => {
            out.push(0xF5); // PRINT
            serialize_print_items(items, out);
        }
        Statement::LPrint { items } => {
            out.push(0xE0); // LPRINT
            serialize_print_items(items, out);
        }
        Statement::Input { items } => {
            out.push(0xEE); // INPUT
            serialize_print_items(items, out);
        }
        Statement::Read { targets } => {
            out.push(0xE3); // READ
            for (i, t) in targets.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                serialize_expr(t, out);
            }
        }
        Statement::Data { items } => {
            out.push(0xE4); // DATA
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                match item {
                    DataItem::String(s) => {
                        out.push(b'"');
                        out.extend_from_slice(s.as_bytes());
                        out.push(b'"');
                    }
                    DataItem::Number(n) => serialize_number(*n, out),
                    DataItem::Raw(s) => out.extend_from_slice(s.as_bytes()),
                }
            }
        }
        Statement::Dim { variable, dimensions } => {
            out.push(0xE9); // DIM
            serialize_variable(variable, out);
            out.push(b'(');
            for (i, d) in dimensions.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                serialize_expr(d, out);
            }
            out.push(b')');
        }
        Statement::Ink(e) => { out.push(0xD9); serialize_expr(e, out); }
        Statement::Paper(e) => { out.push(0xDA); serialize_expr(e, out); }
        Statement::Border(e) => { out.push(0xE7); serialize_expr(e, out); }
        Statement::Bright(e) => { out.push(0xDC); serialize_expr(e, out); }
        Statement::Flash(e) => { out.push(0xDB); serialize_expr(e, out); }
        Statement::Over(e) => { out.push(0xDE); serialize_expr(e, out); }
        Statement::Inverse(e) => { out.push(0xDD); serialize_expr(e, out); }
        Statement::Plot { x, y } => {
            out.push(0xF6); // PLOT
            serialize_expr(x, out);
            out.push(b',');
            serialize_expr(y, out);
        }
        Statement::Draw { dx, dy, angle } => {
            out.push(0xFC); // DRAW
            serialize_expr(dx, out);
            out.push(b',');
            serialize_expr(dy, out);
            if let Some(a) = angle {
                out.push(b',');
                serialize_expr(a, out);
            }
        }
        Statement::Circle { x, y, radius } => {
            out.push(0xD8); // CIRCLE
            serialize_expr(x, out);
            out.push(b',');
            serialize_expr(y, out);
            out.push(b',');
            serialize_expr(radius, out);
        }
        Statement::Beep { duration, pitch } => {
            out.push(0xD7); // BEEP
            serialize_expr(duration, out);
            out.push(b',');
            serialize_expr(pitch, out);
        }
        Statement::Poke { address, value } => {
            out.push(0xF4); // POKE
            serialize_expr(address, out);
            out.push(b',');
            serialize_expr(value, out);
        }
        Statement::Out { port, value } => {
            out.push(0xDF); // OUT
            serialize_expr(port, out);
            out.push(b',');
            serialize_expr(value, out);
        }
        Statement::Cls => out.push(0xFB),
        Statement::Clear(e) => {
            out.push(0xFD);
            if let Some(expr) = e { serialize_expr(expr, out); }
        }
        Statement::Pause(e) => { out.push(0xF2); serialize_expr(e, out); }
        Statement::Run(e) => {
            out.push(0xF7);
            if let Some(expr) = e { serialize_expr(expr, out); }
        }
        Statement::Restore(e) => {
            out.push(0xE5);
            if let Some(expr) = e { serialize_expr(expr, out); }
        }
        Statement::Randomize(e) => {
            out.push(0xF9);
            if let Some(expr) = e { serialize_expr(expr, out); }
        }
        Statement::Continue => out.push(0xE8),
        Statement::New => out.push(0xE6),
        Statement::Copy => out.push(0xFF),
        Statement::Save { filename, qualifier } => {
            out.push(0xF8);
            serialize_expr(filename, out);
            serialize_tape_qualifier(qualifier, out);
        }
        Statement::Load { filename, qualifier } => {
            out.push(0xEF);
            serialize_expr(filename, out);
            serialize_tape_qualifier(qualifier, out);
        }
        Statement::Merge(e) => { out.push(0xD5); serialize_expr(e, out); }
        Statement::Verify { filename, qualifier } => {
            out.push(0xD6);
            serialize_expr(filename, out);
            serialize_tape_qualifier(qualifier, out);
        }
        Statement::OpenHash { channel, args } => {
            out.push(0xD3);
            serialize_expr(channel, out);
            out.push(b',');
            serialize_expr(args, out);
        }
        Statement::CloseHash(e) => { out.push(0xD4); serialize_expr(e, out); }
        Statement::Rem(text) => {
            out.push(0xEA);
            out.extend_from_slice(text.as_bytes());
        }
        Statement::DefFn { name, params, body } => {
            out.push(0xCE); // DEF FN
            out.push(*name as u8);
            out.push(b'(');
            for (i, p) in params.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                serialize_variable(p, out);
            }
            out.push(b')');
            out.push(b'=');
            serialize_expr(body, out);
        }
        Statement::Cat(e) => {
            out.push(0xCF);
            if let Some(expr) = e { serialize_expr(expr, out); }
        }
        Statement::List(e) => {
            out.push(0xF0);
            if let Some(expr) = e { serialize_expr(expr, out); }
        }
        Statement::LList(e) => {
            out.push(0xE1);
            if let Some(expr) = e { serialize_expr(expr, out); }
        }
        Statement::Format(e) => { out.push(0xD0); serialize_expr(e, out); }
        Statement::Move(e) => { out.push(0xD1); serialize_expr(e, out); }
        Statement::Erase(e) => { out.push(0xD2); serialize_expr(e, out); }
        Statement::Expression(e) => serialize_expr(e, out),
    }
}

fn serialize_print_items(items: &[PrintItem], out: &mut Vec<u8>) {
    for item in items {
        match item {
            PrintItem::Expr(e) => serialize_expr(e, out),
            PrintItem::Ink(e) => { out.push(0xD9); serialize_expr(e, out); }
            PrintItem::Paper(e) => { out.push(0xDA); serialize_expr(e, out); }
            PrintItem::Bright(e) => { out.push(0xDC); serialize_expr(e, out); }
            PrintItem::Flash(e) => { out.push(0xDB); serialize_expr(e, out); }
            PrintItem::Over(e) => { out.push(0xDE); serialize_expr(e, out); }
            PrintItem::Inverse(e) => { out.push(0xDD); serialize_expr(e, out); }
            PrintItem::At { row, col } => {
                out.push(0xAC); // AT
                serialize_expr(row, out);
                out.push(b',');
                serialize_expr(col, out);
            }
            PrintItem::Tab(e) => { out.push(0xAD); serialize_expr(e, out); }
            PrintItem::Separator(sep) => {
                out.push(match sep {
                    PrintSep::Semicolon => b';',
                    PrintSep::Comma => b',',
                    PrintSep::Apostrophe => b'\'',
                });
            }
        }
    }
}

fn serialize_tape_qualifier(qualifier: &Option<TapeQualifier>, out: &mut Vec<u8>) {
    if let Some(q) = qualifier {
        match q {
            TapeQualifier::Line(e) => {
                out.push(0xCA); // LINE
                serialize_expr(e, out);
            }
            TapeQualifier::Data(v) => {
                out.push(0xE4); // DATA
                serialize_variable(v, out);
                out.push(b'(');
                out.push(b')');
            }
            TapeQualifier::Code { start, length } => {
                out.push(0xAF); // CODE
                serialize_expr(start, out);
                out.push(b',');
                serialize_expr(length, out);
            }
            TapeQualifier::Screen => {
                out.push(0xAA); // SCREEN$
            }
        }
    }
}

fn serialize_variable(var: &Variable, out: &mut Vec<u8>) {
    out.extend_from_slice(var.name.as_bytes());
    if var.is_string {
        out.push(b'$');
    }
}

fn serialize_expr(expr: &Expr, out: &mut Vec<u8>) {
    match expr {
        Expr::Number(n) => serialize_number(*n, out),
        Expr::StringLiteral(s) => {
            out.push(b'"');
            out.extend_from_slice(s.as_bytes());
            out.push(b'"');
        }
        Expr::Variable(v) => serialize_variable(v, out),
        Expr::ArrayIndex { name, indices } => {
            serialize_variable(name, out);
            out.push(b'(');
            for (i, idx) in indices.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                serialize_expr(idx, out);
            }
            out.push(b')');
        }
        Expr::BinaryOp { left, op, right } => {
            serialize_expr(left, out);
            match op {
                BinOp::Add => out.push(b'+'),
                BinOp::Sub => out.push(b'-'),
                BinOp::Mul => out.push(b'*'),
                BinOp::Div => out.push(b'/'),
                BinOp::Pow => out.push(b'^'),
                BinOp::Eq => out.push(b'='),
                BinOp::Lt => out.push(b'<'),
                BinOp::Gt => out.push(b'>'),
                BinOp::Le => out.push(0xC7),
                BinOp::Ge => out.push(0xC8),
                BinOp::Ne => out.push(0xC9),
                BinOp::And => out.push(0xC6),
                BinOp::Or => out.push(0xC5),
            }
            serialize_expr(right, out);
        }
        Expr::UnaryOp { op, operand } => {
            match op {
                UnaryOp::Neg => out.push(b'-'),
                UnaryOp::Not => out.push(0xC3),
            }
            serialize_expr(operand, out);
        }
        Expr::Function { func, arg } => {
            out.push(builtin_fn_token(*func));
            serialize_expr(arg, out);
        }
        Expr::Slice { string, from, to } => {
            serialize_expr(string, out);
            out.push(b'(');
            serialize_expr(from, out);
            out.push(0xCC); // TO
            serialize_expr(to, out);
            out.push(b')');
        }
        Expr::FnCall { name, args } => {
            out.push(0xA8); // FN
            out.push(*name as u8);
            if !args.is_empty() {
                out.push(b'(');
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        out.push(b',');
                    }
                    serialize_expr(a, out);
                }
                out.push(b')');
            }
        }
        Expr::Paren(inner) => {
            out.push(b'(');
            serialize_expr(inner, out);
            out.push(b')');
        }
        Expr::Inkey => out.push(0xA6),
        Expr::Rnd => out.push(0xA5),
        Expr::Pi => out.push(0xA7),
    }
}

fn builtin_fn_token(func: BuiltinFn) -> u8 {
    match func {
        BuiltinFn::Abs => 0xBD,
        BuiltinFn::Acs => 0xB6,
        BuiltinFn::Asn => 0xB5,
        BuiltinFn::Atn => 0xB7,
        BuiltinFn::Attr => 0xAB,
        BuiltinFn::Bin => 0xC4,
        BuiltinFn::Chr => 0xC2,
        BuiltinFn::Code => 0xAF,
        BuiltinFn::Cos => 0xB3,
        BuiltinFn::Exp => 0xB9,
        BuiltinFn::In => 0xBF,
        BuiltinFn::Int => 0xBA,
        BuiltinFn::Len => 0xB1,
        BuiltinFn::Ln => 0xB8,
        BuiltinFn::Peek => 0xBE,
        BuiltinFn::Point => 0xA9,
        BuiltinFn::Screen => 0xAA,
        BuiltinFn::Sgn => 0xBC,
        BuiltinFn::Sin => 0xB2,
        BuiltinFn::Sqr => 0xBB,
        BuiltinFn::Str => 0xC1,
        BuiltinFn::Tan => 0xB4,
        BuiltinFn::Usr => 0xC0,
        BuiltinFn::Val => 0xB0,
        BuiltinFn::ValDollar => 0xAE,
    }
}

/// Serialize a number: ASCII representation + 0x0E + 5-byte float.
fn serialize_number(val: f64, out: &mut Vec<u8>) {
    let s = format_number(val);
    out.extend_from_slice(s.as_bytes());
    out.push(0x0E);
    out.extend_from_slice(&number_to_float5(val));
}

fn format_number(val: f64) -> String {
    let int_val = val as i64;
    #[allow(clippy::float_cmp)]
    if val == int_val as f64 && val.fract() == 0.0 {
        return int_val.to_string();
    }
    format!("{val}")
}

fn number_to_float5(val: f64) -> [u8; 5] {
    let int_val = val as i64;
    #[allow(clippy::float_cmp)]
    if val == int_val as f64 && (-65535..=65535).contains(&int_val) {
        let sign = if int_val < 0 { 0xFF } else { 0x00 };
        let abs_val = int_val.unsigned_abs() as u16;
        return [0x00, sign, abs_val as u8, (abs_val >> 8) as u8, 0x00];
    }
    float_to_spectrum5(val)
}

fn float_to_spectrum5(val: f64) -> [u8; 5] {
    if val == 0.0 {
        return [0x00; 5];
    }

    let negative = val < 0.0;
    let val = val.abs();

    let mut exp = val.log2().floor() as i32 + 1;
    let mut mantissa = val / 2.0_f64.powi(exp);

    while mantissa >= 1.0 {
        mantissa /= 2.0;
        exp += 1;
    }
    while mantissa < 0.5 && mantissa > 0.0 {
        mantissa *= 2.0;
        exp -= 1;
    }

    let exp_byte = (exp + 0x80) as u8;
    let m = ((mantissa * 2.0 - 1.0) * (1u64 << 31) as f64) as u32;
    let m_bytes = m.to_be_bytes();

    let mut result = [exp_byte, m_bytes[0], m_bytes[1], m_bytes[2], m_bytes[3]];
    if negative {
        result[1] |= 0x80;
    } else {
        result[1] &= 0x7F;
    }
    result
}
