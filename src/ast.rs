use crate::source::Span;

#[derive(Debug)]
pub struct Program {
    pub package: Option<PathRef>,
    pub uses: Vec<PathRef>,
    pub functions: Vec<Function>,
    pub structs: Vec<StructDecl>,
    pub enums: Vec<EnumDecl>,
}

#[derive(Debug)]
pub struct StructDecl {
    pub source_id: usize,
    pub public: bool,
    pub name: String,
    pub name_span: Span,
    pub fields: Vec<FieldDecl>,
}

#[derive(Debug)]
pub struct FieldDecl {
    pub name: String,
    pub name_span: Span,
    pub ty: TypeRef,
}

#[derive(Debug)]
pub struct EnumDecl {
    pub source_id: usize,
    pub public: bool,
    pub name: String,
    pub name_span: Span,
    pub variants: Vec<VariantDecl>,
}

#[derive(Debug)]
pub struct VariantDecl {
    pub name: String,
    pub name_span: Span,
    pub fields: Vec<TypeRef>,
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
    /// 动态切片 `[]T`（M14）。
    Slice {
        element: Box<TypeRef>,
    },
    /// 显式指针 `*T`（M14）。
    Pointer {
        inner: Box<TypeRef>,
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
    /// 指针解引用赋值 `*p = v`（M14）。
    DerefAssignment {
        target: Expr,
        operator: AssignmentOperator,
        value: Expr,
    },
    /// 指针字段赋值 `q->x = v`（M14）。
    PtrFieldAssignment {
        base: Expr,
        field: String,
        field_span: Span,
        operator: AssignmentOperator,
        value: Expr,
    },
    /// `defer <expr>;`（M14）：把语句推迟到块作用域退出时执行。
    Defer {
        value: Expr,
    },
    /// `try (var x = alloc(...), ...) { ... }`（M14）：`defer free` + 块的语法糖。
    Try {
        resources: Vec<TryResource>,
        body: Block,
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

/// `try(...)` 括号内声明的资源（M14）：`var name = initializer`。
#[derive(Debug)]
pub struct TryResource {
    pub name: String,
    pub name_span: Span,
    pub initializer: Expr,
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
    Field {
        base: Box<Expr>,
        field: String,
        field_span: Span,
    },
    /// 取址 `&e`（M14）。
    AddressOf {
        operand: Box<Expr>,
    },
    /// 解引用 `*e`（M14）。
    Deref {
        operand: Box<Expr>,
    },
    /// 指针字段访问 `q->x`（M14）。
    PtrField {
        base: Box<Expr>,
        field: String,
        field_span: Span,
    },
    Match {
        value: Box<Expr>,
        arms: Vec<MatchArm>,
    },
}

#[derive(Debug)]
pub struct MatchArm {
    pub pattern: MatchPattern,
    pub body: Expr,
}

#[derive(Debug)]
pub enum MatchPattern {
    Enum {
        name: String,
        name_span: Span,
        bindings: Vec<String>,
    },
    Wildcard,
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
