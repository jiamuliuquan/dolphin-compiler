use crate::ast::{BinaryOperator, UnaryOperator};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Type {
    Unit,
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    F32,
    F64,
    Char,
    Bool,
    String,
    Array { element: ScalarType, length: usize },
    Struct(TypeId),
    Enum(TypeId),
}

/// 用户自定义类型的全局编号（M13）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TypeId(pub usize);

/// 用户自定义类型定义（M13）。
#[derive(Debug)]
pub enum TypeDef {
    Struct { fields: Vec<StructField> },
    Enum { variants: Vec<EnumVariant> },
}

#[derive(Debug)]
pub struct StructField {
    pub name: String,
    pub ty: Type,
}

#[derive(Debug)]
pub struct EnumVariant {
    pub name: String,
    pub fields: Vec<Type>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScalarType {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    F32,
    F64,
    Char,
    Bool,
    String,
}

impl ScalarType {
    pub fn as_type(self) -> Type {
        match self {
            Self::I8 => Type::I8,
            Self::I16 => Type::I16,
            Self::I32 => Type::I32,
            Self::I64 => Type::I64,
            Self::U8 => Type::U8,
            Self::U16 => Type::U16,
            Self::U32 => Type::U32,
            Self::U64 => Type::U64,
            Self::F32 => Type::F32,
            Self::F64 => Type::F64,
            Self::Char => Type::Char,
            Self::Bool => Type::Bool,
            Self::String => Type::String,
        }
    }
}

impl Type {
    pub fn as_scalar(self) -> Option<ScalarType> {
        match self {
            Self::I8 => Some(ScalarType::I8),
            Self::I16 => Some(ScalarType::I16),
            Self::I32 => Some(ScalarType::I32),
            Self::I64 => Some(ScalarType::I64),
            Self::U8 => Some(ScalarType::U8),
            Self::U16 => Some(ScalarType::U16),
            Self::U32 => Some(ScalarType::U32),
            Self::U64 => Some(ScalarType::U64),
            Self::F32 => Some(ScalarType::F32),
            Self::F64 => Some(ScalarType::F64),
            Self::Char => Some(ScalarType::Char),
            Self::Bool => Some(ScalarType::Bool),
            Self::String => Some(ScalarType::String),
            Self::Unit | Self::Array { .. } | Self::Struct(_) | Self::Enum(_) => None,
        }
    }

    pub fn is_integer(self) -> bool {
        matches!(
            self,
            Self::I8
                | Self::I16
                | Self::I32
                | Self::I64
                | Self::U8
                | Self::U16
                | Self::U32
                | Self::U64
        )
    }

    pub fn is_signed_integer(self) -> bool {
        matches!(self, Self::I8 | Self::I16 | Self::I32 | Self::I64)
    }

    pub fn is_float(self) -> bool {
        matches!(self, Self::F32 | Self::F64)
    }

    pub fn bits(self) -> Option<u16> {
        match self {
            Self::I8 | Self::U8 => Some(8),
            Self::I16 | Self::U16 => Some(16),
            Self::I32 | Self::U32 | Self::F32 | Self::Char => Some(32),
            Self::I64 | Self::U64 | Self::F64 => Some(64),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FunctionId(pub usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalId(pub usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockId(pub usize);

#[derive(Debug)]
pub struct Program {
    pub functions: Vec<Function>,
    pub main: FunctionId,
    pub types: Vec<TypeDef>,
}

#[derive(Debug)]
pub struct Function {
    pub id: FunctionId,
    pub name: String,
    pub parameters: Vec<LocalId>,
    pub return_type: Type,
    pub locals: Vec<Type>,
    pub blocks: Vec<BasicBlock>,
    pub entry: BlockId,
}

#[derive(Debug)]
pub struct BasicBlock {
    pub instructions: Vec<Instruction>,
    pub terminator: Terminator,
}

#[derive(Debug)]
pub enum Instruction {
    SetLocal {
        local: LocalId,
        value: Expr,
    },
    SetIndex {
        local: LocalId,
        index: Expr,
        value: Expr,
    },
    Evaluate(Expr),
    Print(Vec<PrintPart>),
}

#[derive(Debug)]
pub enum PrintPart {
    Text(String),
    Value(Expr),
}

#[derive(Debug)]
pub enum Terminator {
    Jump(BlockId),
    Branch {
        condition: Expr,
        then_block: BlockId,
        else_block: BlockId,
    },
    Return(Option<Expr>),
}

#[derive(Debug)]
pub struct Expr {
    pub kind: ExprKind,
    pub ty: Type,
}

#[derive(Debug)]
pub enum ExprKind {
    Integer(u64),
    Float(f64),
    Char(char),
    Bool(bool),
    String(String),
    StringLength(Box<Expr>),
    Array(Vec<Expr>),
    RepeatArray {
        value: Box<Expr>,
        length: usize,
    },
    Index {
        array: Box<Expr>,
        index: Box<Expr>,
    },
    Local(LocalId),
    Call {
        function: FunctionId,
        arguments: Vec<Expr>,
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
    Cast {
        value: Box<Expr>,
        to: Type,
    },
    StructInit {
        fields: Vec<Expr>,
    },
    EnumInit {
        variant: usize,
        arguments: Vec<Expr>,
    },
    Field {
        base: Box<Expr>,
        field: usize,
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
    Variant {
        variant: usize,
        bindings: Vec<LocalId>,
    },
    Wildcard,
}

impl Expr {
    pub fn i32(value: i32) -> Self {
        Self {
            kind: ExprKind::Integer(value as u32 as u64),
            ty: Type::I32,
        }
    }
}

impl std::fmt::Display for Type {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::Unit => formatter.write_str("Unit"),
            Type::I8 => formatter.write_str("i8"),
            Type::I16 => formatter.write_str("i16"),
            Type::I32 => formatter.write_str("i32"),
            Type::I64 => formatter.write_str("i64"),
            Type::U8 => formatter.write_str("u8"),
            Type::U16 => formatter.write_str("u16"),
            Type::U32 => formatter.write_str("u32"),
            Type::U64 => formatter.write_str("u64"),
            Type::F32 => formatter.write_str("f32"),
            Type::F64 => formatter.write_str("f64"),
            Type::Char => formatter.write_str("char"),
            Type::Bool => formatter.write_str("bool"),
            Type::String => formatter.write_str("string"),
            Type::Array { element, length } => write!(formatter, "[{element}; {length}]"),
            Type::Struct(id) => write!(formatter, "struct@{}", id.0),
            Type::Enum(id) => write!(formatter, "enum@{}", id.0),
        }
    }
}

impl std::fmt::Display for ScalarType {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            ScalarType::I8 => "i8",
            ScalarType::I16 => "i16",
            ScalarType::I32 => "i32",
            ScalarType::I64 => "i64",
            ScalarType::U8 => "u8",
            ScalarType::U16 => "u16",
            ScalarType::U32 => "u32",
            ScalarType::U64 => "u64",
            ScalarType::F32 => "f32",
            ScalarType::F64 => "f64",
            ScalarType::Char => "char",
            ScalarType::Bool => "bool",
            ScalarType::String => "string",
        })
    }
}
