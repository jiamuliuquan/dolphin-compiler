use crate::source::Span;

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

#[derive(Clone, Debug)]
pub struct StructDecl {
    pub source_id: usize,
    pub public: bool,
    pub name: String,
    pub name_span: Span,
    /// 类型参数列表 `<T, U>`（M15，阶段 1 解析、阶段 2 使用）。
    pub type_params: Vec<String>,
    pub fields: Vec<FieldDecl>,
}

#[derive(Clone, Debug)]
pub struct FieldDecl {
    pub name: String,
    pub name_span: Span,
    pub ty: TypeRef,
}

#[derive(Clone, Debug)]
pub struct EnumDecl {
    pub source_id: usize,
    pub public: bool,
    pub name: String,
    pub name_span: Span,
    /// 类型参数列表 `<T, U>`（M15，阶段 1 解析、阶段 2 使用）。
    pub type_params: Vec<String>,
    pub variants: Vec<VariantDecl>,
}

#[derive(Clone, Debug)]
pub struct VariantDecl {
    pub name: String,
    pub name_span: Span,
    pub fields: Vec<TypeRef>,
}

/// 契约声明 `trait 名 { 方法签名 }`（M15）。
#[derive(Clone, Debug)]
#[allow(dead_code)] // 阶段 3 使用。
pub struct TraitDecl {
    pub source_id: usize,
    pub public: bool,
    pub name: String,
    pub name_span: Span,
    pub methods: Vec<MethodSignature>,
}

/// 契约方法签名（M15）：只声明、不写函数体。
#[derive(Clone, Debug)]
#[allow(dead_code)] // 阶段 3 使用。
pub struct MethodSignature {
    pub name: String,
    pub name_span: Span,
    pub parameters: Vec<Parameter>,
    pub return_type: Option<TypeRef>,
}

/// 实现块（M15）：`impl Type { ... }` 或 `impl Trait for Type { ... }`。
#[derive(Clone, Debug)]
#[allow(dead_code)] // 阶段 3 使用。
pub struct ImplBlock {
    pub source_id: usize,
    /// `None` 表示固有方法（`impl Type`）；`Some(name)` 表示契约实现（`impl Trait for Type`）。
    pub trait_name: Option<String>,
    pub type_name: String,
    pub methods: Vec<Function>,
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
    /// 类型参数列表 `<T, U>`（M15，阶段 1 解析、阶段 2 使用）。
    pub type_params: Vec<String>,
    pub parameters: Vec<Parameter>,
    pub return_type: Option<TypeRef>,
    pub body: Block,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Parameter {
    pub name: String,
    pub name_span: Span,
    pub ty: TypeRef,
}

#[derive(Clone, Debug)]
pub struct TypeRef {
    pub kind: TypeRefKind,
    pub span: Span,
}

/// 类型引用的等价与哈希：忽略 `span`，只比较结构（用于单态化去重）。
impl PartialEq for TypeRef {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
    }
}
impl Eq for TypeRef {}
impl std::hash::Hash for TypeRef {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.kind.hash(state);
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
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
    /// 泛型实例 `Vec<i32>`（M15）。
    Generic {
        name: String,
        args: Vec<TypeRef>,
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

#[derive(Clone, Debug)]
pub enum ForIterable {
    Range {
        start: Expr,
        end: Expr,
        inclusive: bool,
    },
    Array(Expr),
}

/// `try(...)` 括号内声明的资源（M14）：`var name = initializer`。
#[derive(Clone, Debug)]
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
        /// 显式泛型实参 `allocate<T>(n)`（M15）。普通调用为 `None`。
        type_args: Option<Vec<TypeRef>>,
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
