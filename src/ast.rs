use crate::source::Span;

#[derive(Debug)]
pub struct Program {
    pub package: Option<PathRef>,
    pub uses: Vec<PathRef>,
    pub functions: Vec<Function>,
}

#[derive(Debug)]
pub struct PathRef {
    pub segments: Vec<String>,
    pub span: Span,
}

#[derive(Debug)]
pub struct Function {
    pub source_id: usize,
    pub public: bool,
    pub name: String,
    pub name_span: Span,
    pub parameters: Vec<Parameter>,
    pub return_type: Option<TypeRef>,
    pub body: Block,
    pub span: Span,
}

#[derive(Debug)]
pub struct Parameter {
    pub name: String,
    pub name_span: Span,
    pub ty: TypeRef,
}

#[derive(Debug)]
pub struct TypeRef {
    pub kind: TypeRefKind,
    pub span: Span,
}

#[derive(Debug)]
pub enum TypeRefKind {
    Name(String),
    Array {
        element: Box<TypeRef>,
        length: usize,
    },
}

pub type Block = Vec<Statement>;

#[derive(Debug)]
pub struct Statement {
    pub kind: StatementKind,
    pub span: Span,
}

#[derive(Debug)]
pub enum StatementKind {
    Variable {
        mutable: bool,
        name: String,
        name_span: Span,
        type_name: Option<TypeRef>,
        initializer: Expr,
    },
    Assignment {
        name: String,
        name_span: Span,
        operator: AssignmentOperator,
        value: Expr,
    },
    IndexAssignment {
        name: String,
        name_span: Span,
        index: Expr,
        operator: AssignmentOperator,
        value: Expr,
    },
    Expression(Expr),
    If {
        condition: Expr,
        then_block: Block,
        else_block: Option<Block>,
    },
    Loop(Block),
    While {
        condition: Expr,
        body: Block,
    },
    For {
        name: String,
        name_span: Span,
        iterable: ForIterable,
        body: Block,
    },
    Break,
    Continue,
    Return(Option<Expr>),
}

#[derive(Debug)]
pub enum ForIterable {
    Range {
        start: Expr,
        end: Expr,
        inclusive: bool,
    },
    Array(Expr),
}

#[derive(Clone, Copy, Debug)]
pub enum AssignmentOperator {
    Assign,
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
}

#[derive(Debug)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Debug)]
pub enum ExprKind {
    Number(String),
    Character(char),
    Boolean(bool),
    String(String),
    Array(Vec<Expr>),
    RepeatArray {
        value: Box<Expr>,
        length: usize,
    },
    Name(String),
    Call {
        callee: String,
        callee_span: Span,
        arguments: Vec<Expr>,
    },
    Index {
        array: Box<Expr>,
        index: Box<Expr>,
    },
    Cast {
        value: Box<Expr>,
        ty: TypeRef,
    },
    Unary {
        operator: UnaryOperator,
        operand: Box<Expr>,
    },
    Binary {
        operator: BinaryOperator,
        left: Box<Expr>,
        right: Box<Expr>,
    },
}

#[derive(Clone, Copy, Debug)]
pub enum UnaryOperator {
    Negate,
    Not,
}

#[derive(Clone, Copy, Debug)]
pub enum BinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    And,
    Or,
}
