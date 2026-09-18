use dolphin_source::source::Span;

#[derive(Clone, Debug)]
pub struct Program {
    pub package: Option<PathRef>,
    pub uses: Vec<PathRef>,
    pub functions: Vec<Function>,
    pub structs: Vec<StructDecl>,
    pub enums: Vec<EnumDecl>,
    pub traits: Vec<TraitDecl>,
    pub impls: Vec<ImplBlock>,
}

/// 一个泛型类型参数声明 `T` / `I: Iterator`（M15）。
#[derive(Clone, Debug)]
pub struct TypeParamDecl {
    pub name: String,
    pub name_span: Span,
    /// 单 trait 约束（限定名，模块解析后填入）；首版每参数最多一个。
    pub bound: Option<String>,
}

/// `trait Name<T> { type Item; fn next(self: *Self): Option<Self::Item>; }`。
#[derive(Clone, Debug)]
pub struct TraitDecl {
    pub source_id: usize,
    pub public: bool,
    pub name: String,
    pub name_span: Span,
    pub type_params: Vec<TypeParamDecl>,
    pub associated_types: Vec<AssociatedTypeDecl>,
    pub methods: Vec<Function>,
}

#[derive(Clone, Debug)]
pub struct AssociatedTypeDecl {
    pub name: String,
    pub name_span: Span,
}

/// `impl Type { ... }` 或 `impl Trait for Type { type Item = X; ... }`。
#[derive(Clone, Debug)]
pub struct ImplBlock {
    pub source_id: usize,
    /// 模块解析阶段填入：impl 块所在模块（字段可见性判定用）。
    pub module: String,
    /// `impl Trait for Type` 时的 trait 路径（`None` 表示固有方法）。
    pub trait_name: Option<String>,
    pub trait_span: Span,
    /// trait 的类型实参（泛型 trait 暂不支持，保留用于声明处诊断）。
    pub trait_arguments: Vec<TypeRef>,
    pub type_name: String,
    pub type_span: Span,
    /// impl 目标的类型实参（如 `impl<T> Box<T>` 中的 `T`）；
    /// 本批要求在声明处与 impl 参数按位置一一对应。
    pub type_arguments: Vec<TypeRef>,
    pub type_params: Vec<TypeParamDecl>,
    pub associated_types: Vec<AssociatedTypeBinding>,
    pub methods: Vec<Function>,
}

#[derive(Clone, Debug)]
pub struct AssociatedTypeBinding {
    pub name: String,
    pub name_span: Span,
    pub ty: TypeRef,
}

#[derive(Clone, Debug)]
pub struct StructDecl {
    pub source_id: usize,
    pub public: bool,
    pub name: String,
    pub name_span: Span,
    pub type_params: Vec<TypeParamDecl>,
    pub fields: Vec<FieldDecl>,
    /// `extern struct`：按目标 C 布局，只能通过指针传给 C（M14-E）。
    pub extern_c: bool,
}

#[derive(Clone, Debug)]
pub struct FieldDecl {
    pub name: String,
    pub name_span: Span,
    pub ty: TypeRef,
    /// 字段可见性：默认模块私有（M15）。
    pub public: bool,
}

#[derive(Clone, Debug)]
pub struct EnumDecl {
    pub source_id: usize,
    pub public: bool,
    pub name: String,
    pub name_span: Span,
    pub type_params: Vec<TypeParamDecl>,
    pub variants: Vec<VariantDecl>,
}

#[derive(Clone, Debug)]
pub struct VariantDecl {
    pub name: String,
    pub name_span: Span,
    pub fields: Vec<TypeRef>,
}

#[derive(Clone, Debug)]
pub struct PathRef {
    pub segments: Vec<String>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Function {
    pub source_id: usize,
    pub public: bool,
    pub name: String,
    pub name_span: Span,
    pub type_params: Vec<TypeParamDecl>,
    pub parameters: Vec<Parameter>,
    pub return_type: Option<TypeRef>,
    pub body: Block,
    pub span: Span,
    /// `extern "C"` 声明：无函数体，按 C ABI 调用，链接名不加 Dolphin mangling。
    pub extern_c: bool,
    /// extern 函数的原始 C 链接名（普通函数为 `None`）。
    pub link_name: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Parameter {
    pub name: String,
    pub name_span: Span,
    pub ty: TypeRef,
    /// `self` 接收者（M15 方法）；`self` / `self: *Self` / `self: *const Self`。
    pub receiver: bool,
}

#[derive(Clone, Debug)]
pub struct TypeRef {
    pub kind: TypeRefKind,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum TypeRefKind {
    /// 具名类型，可带泛型实参：`i32`、`mod.Point`、`Vec<i32>`、`Option<Result<T, E>>`。
    Name {
        name: String,
        arguments: Vec<TypeRef>,
    },
    Array {
        element: Box<TypeRef>,
        length: usize,
    },
    Ptr {
        pointee: Box<TypeRef>,
        mutable: bool,
    },
    Slice {
        element: Box<TypeRef>,
        mutable: bool,
    },
}

pub type Block = Vec<Statement>;

#[derive(Clone, Debug)]
pub struct Statement {
    pub kind: StatementKind,
    pub span: Span,
}

#[derive(Clone, Debug)]
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
    FieldAssignment {
        name: String,
        name_span: Span,
        field: String,
        field_span: Span,
        deref: bool,
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
    /// `defer call_expression;`：块退出时逆序执行的清理调用（M14-D）。
    Defer(Expr),
    Return(Option<Expr>),
}

#[derive(Clone, Debug)]
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

#[derive(Clone, Debug)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Clone, Debug)]
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
        type_arguments: Vec<TypeRef>,
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
    AddressOf {
        operand: Box<Expr>,
    },
    Deref {
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
    Match {
        value: Box<Expr>,
        arms: Vec<MatchArm>,
    },
}

#[derive(Clone, Debug)]
pub struct MatchArm {
    pub pattern: MatchPattern,
    pub body: Expr,
}

#[derive(Clone, Debug)]
pub enum MatchPattern {
    Enum {
        name: String,
        name_span: Span,
        /// `Option<i32>.Some(v)` 中的显式类型实参（可省略，由被匹配值推断）。
        type_arguments: Vec<TypeRef>,
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
