//! Abstract syntax tree for ZX Spectrum BASIC.

/// A complete BASIC program.
#[derive(Debug, Clone)]
pub struct Program {
    pub lines: Vec<Line>,
}

/// A single numbered line.
#[derive(Debug, Clone)]
pub struct Line {
    pub number: u16,
    pub statements: Vec<Statement>,
}

/// A BASIC statement.
#[derive(Debug, Clone)]
pub enum Statement {
    // -- Assignment --
    Let {
        target: Expr,
        value: Expr,
    },

    // -- Control flow --
    For {
        variable: Variable,
        from: Expr,
        to: Expr,
        step: Option<Expr>,
    },
    Next {
        variable: Option<Variable>,
    },
    If {
        condition: Expr,
        then_body: Vec<Statement>,
    },
    GoTo(Expr),
    GoSub(Expr),
    Return,
    Stop,

    // -- I/O --
    Print {
        items: Vec<PrintItem>,
    },
    LPrint {
        items: Vec<PrintItem>,
    },
    Input {
        items: Vec<PrintItem>,
    },
    Read {
        targets: Vec<Expr>,
    },
    Data {
        items: Vec<DataItem>,
    },

    // -- Variables --
    Dim {
        variable: Variable,
        dimensions: Vec<Expr>,
    },

    // -- Colour --
    Ink(Expr),
    Paper(Expr),
    Border(Expr),
    Bright(Expr),
    Flash(Expr),
    Over(Expr),
    Inverse(Expr),

    // -- Graphics --
    Plot {
        x: Expr,
        y: Expr,
    },
    Draw {
        dx: Expr,
        dy: Expr,
        angle: Option<Expr>,
    },
    Circle {
        x: Expr,
        y: Expr,
        radius: Expr,
    },

    // -- Sound --
    Beep {
        duration: Expr,
        pitch: Expr,
    },

    // -- Memory --
    Poke {
        address: Expr,
        value: Expr,
    },
    Out {
        port: Expr,
        value: Expr,
    },

    // -- System --
    Cls,
    Clear(Option<Expr>),
    Pause(Expr),
    Run(Option<Expr>),
    Restore(Option<Expr>),
    Randomize(Option<Expr>),
    Continue,
    New,
    Copy,

    // -- Tape --
    Save {
        filename: Expr,
        qualifier: Option<TapeQualifier>,
    },
    Load {
        filename: Expr,
        qualifier: Option<TapeQualifier>,
    },
    Merge(Expr),
    Verify {
        filename: Expr,
        qualifier: Option<TapeQualifier>,
    },

    // -- I/O channels --
    OpenHash {
        channel: Expr,
        args: Expr,
    },
    CloseHash(Expr),

    // -- Misc --
    Rem(String),
    DefFn {
        name: char,
        params: Vec<Variable>,
        body: Expr,
    },
    Cat(Option<Expr>),
    List(Option<Expr>),
    LList(Option<Expr>),
    Format(Expr),
    Move(Expr),
    Erase(Expr),

    // Bare expression (implicit LET or standalone value).
    Expression(Expr),
}

/// PRINT/INPUT list item.
#[derive(Debug, Clone)]
pub enum PrintItem {
    Expr(Expr),
    Ink(Expr),
    Paper(Expr),
    Bright(Expr),
    Flash(Expr),
    Over(Expr),
    Inverse(Expr),
    At { row: Expr, col: Expr },
    Tab(Expr),
    Separator(PrintSep),
}

/// PRINT separator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrintSep {
    Semicolon,
    Comma,
    Apostrophe,
}

/// DATA item — either a quoted string or raw text/number.
#[derive(Debug, Clone)]
pub enum DataItem {
    String(String),
    Number(f64),
    Raw(String),
}

/// SAVE/LOAD/VERIFY qualifier.
#[derive(Debug, Clone)]
pub enum TapeQualifier {
    Line(Expr),
    Data(Variable),
    Code { start: Expr, length: Expr },
    Screen,
}

/// An expression.
#[derive(Debug, Clone)]
pub enum Expr {
    Number(f64),
    StringLiteral(String),
    Variable(Variable),
    ArrayIndex {
        name: Variable,
        indices: Vec<Expr>,
    },
    BinaryOp {
        left: Box<Expr>,
        op: BinOp,
        right: Box<Expr>,
    },
    UnaryOp {
        op: UnaryOp,
        operand: Box<Expr>,
    },
    Function {
        func: BuiltinFn,
        arg: Box<Expr>,
    },
    Slice {
        string: Box<Expr>,
        from: Box<Expr>,
        to: Box<Expr>,
    },
    FnCall {
        name: char,
        args: Vec<Expr>,
    },
    Paren(Box<Expr>),
    Inkey,
    Rnd,
    Pi,
}

/// A variable reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variable {
    pub name: String,
    pub is_string: bool,
}

/// Binary operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Eq,
    Lt,
    Gt,
    Le,
    Ge,
    Ne,
    And,
    Or,
}

/// Unary operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Not,
}

/// Built-in function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinFn {
    Abs,
    Acs,
    Asn,
    Atn,
    Attr,
    Bin,
    Chr,
    Code,
    Cos,
    Exp,
    In,
    Int,
    Len,
    Ln,
    Peek,
    Point,
    Screen,
    Sgn,
    Sin,
    Sqr,
    Str,
    Tan,
    Usr,
    Val,
    ValDollar,
}
