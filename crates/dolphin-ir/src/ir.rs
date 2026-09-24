use dolphin_syntax::ast::{BinaryOperator, UnaryOperator};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
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
    Usize,
    Isize,
    F32,
    F64,
    Char,
    Bool,
    String,
    /// `null` 字面量的占位类型：尚未绑定具体指针类型，只在类型检查中出现。
    Null,
    Array {
        element: ScalarType,
        length: usize,
    },
    Struct(TypeId),
    Enum(TypeId),
    Ptr {
        pointee: Box<Type>,
        mutable: bool,
    },
    Slice {
        element: Box<Type>,
        mutable: bool,
    },
}

/// 用户自定义类型的全局编号（M13）。
///
/// `Ord`/`PartialOrd` 供 M20 分析 side table 的 `BTreeMap` 使用；不影响布局或 ABI。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypeId(pub usize);

/// 用户自定义类型定义（M13）。
#[derive(Debug)]
pub enum TypeDef {
    Struct {
        fields: Vec<StructField>,
        /// `extern struct`：按目标 C 布局，只能通过指针传给 C。
        extern_c: bool,
    },
    Enum {
        variants: Vec<EnumVariant>,
    },
}

#[derive(Debug)]
pub struct StructField {
    pub name: String,
    pub ty: Type,
    /// 字段可见性：默认模块私有（M15）。
    pub public: bool,
}

#[derive(Debug)]
pub struct EnumVariant {
    pub name: String,
    pub fields: Vec<Type>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ScalarType {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    Usize,
    Isize,
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
            Self::Usize => Type::Usize,
            Self::Isize => Type::Isize,
            Self::F32 => Type::F32,
            Self::F64 => Type::F64,
            Self::Char => Type::Char,
            Self::Bool => Type::Bool,
            Self::String => Type::String,
        }
    }
}

impl Type {
    pub fn as_scalar(&self) -> Option<ScalarType> {
        match self {
            Self::I8 => Some(ScalarType::I8),
            Self::I16 => Some(ScalarType::I16),
            Self::I32 => Some(ScalarType::I32),
            Self::I64 => Some(ScalarType::I64),
            Self::U8 => Some(ScalarType::U8),
            Self::U16 => Some(ScalarType::U16),
            Self::U32 => Some(ScalarType::U32),
            Self::U64 => Some(ScalarType::U64),
            Self::Usize => Some(ScalarType::Usize),
            Self::Isize => Some(ScalarType::Isize),
            Self::F32 => Some(ScalarType::F32),
            Self::F64 => Some(ScalarType::F64),
            Self::Char => Some(ScalarType::Char),
            Self::Bool => Some(ScalarType::Bool),
            Self::String => Some(ScalarType::String),
            Self::Unit | Self::Array { .. } | Self::Struct(_) | Self::Enum(_) => None,
            Self::Ptr { .. } | Self::Slice { .. } | Self::Null => None,
        }
    }

    pub fn is_integer(&self) -> bool {
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
                | Self::Usize
                | Self::Isize
        )
    }

    pub fn is_signed_integer(&self) -> bool {
        matches!(
            self,
            Self::I8 | Self::I16 | Self::I32 | Self::I64 | Self::Isize
        )
    }

    pub fn is_float(&self) -> bool {
        matches!(self, Self::F32 | Self::F64)
    }

    pub fn bits(&self) -> Option<u16> {
        match self {
            Self::I8 | Self::U8 => Some(8),
            Self::I16 | Self::U16 => Some(16),
            Self::I32 | Self::U32 | Self::F32 | Self::Char => Some(32),
            Self::I64 | Self::U64 | Self::Usize | Self::Isize | Self::F64 => Some(64),
            _ => None,
        }
    }

    /// 是否为指针类型（`*T` / `*const T`）。
    pub fn is_pointer(&self) -> bool {
        matches!(self, Self::Ptr { .. })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FunctionId(pub usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalId(pub usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockId(pub usize);

/// 源码位置（1 基行列），后端无关。`file` 为 `Program.sources` 的下标。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Location {
    pub file: u32,
    pub line: u32,
    pub column: u32,
}

#[derive(Debug)]
pub struct Program {
    pub functions: Vec<Function>,
    /// 可执行入口；库构建（无 `main`）为 `None`。
    pub main: Option<FunctionId>,
    pub types: Vec<TypeDef>,
    /// 源码路径，按 `source_id` 索引。
    pub sources: Vec<std::path::PathBuf>,
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
    /// `extern "C"` 函数的原始链接名；普通函数为 `None`。
    pub external_link_name: Option<String>,
    /// 声明所在源码的 `source_id`。
    pub source: u32,
    /// 声明起始位置。
    pub location: Location,
}

#[derive(Debug)]
pub struct BasicBlock {
    pub instructions: Vec<Instruction>,
    pub terminator: Terminator,
    /// 块起始位置。
    pub location: Location,
    /// 与 `instructions` 一一对应的位置。
    pub locations: Vec<Location>,
    /// 终结指令的位置。
    pub terminator_location: Location,
}

#[derive(Debug)]
pub enum Instruction {
    SetLocal {
        local: LocalId,
        value: Expr,
    },
    SetFieldAt {
        place: Box<Place>,
        field: usize,
        value: Expr,
    },
    SetIndexAt {
        place: Box<Place>,
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

#[derive(Clone, Debug)]
pub struct Expr {
    pub kind: ExprKind,
    pub ty: Type,
}

#[derive(Clone, Debug)]
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
    AddressOf {
        place: Box<Place>,
    },
    Deref {
        pointer: Box<Expr>,
    },
    Null,
    SlicePtr {
        base: Box<Expr>,
    },
    SliceLen {
        base: Box<Expr>,
    },
    SliceRange {
        base: Box<Expr>,
        start: Box<Expr>,
        end: Box<Expr>,
    },
    /// `mem.alloc<T>(count)`：分配 `count` 个 `T` 的连续存储。
    MemAlloc {
        element: Type,
        count: Box<Expr>,
    },
    /// `mem.free<T>(buffer)`：释放完整原始切片。
    MemFree {
        element: Type,
        buffer: Box<Expr>,
    },
    /// `mem.create<T>(value)`：分配一个已初始化的 `T`，返回指向它的指针。
    MemCreate {
        element: Type,
        value: Box<Expr>,
    },
    /// `mem.destroy<T>(ptr)`：释放单个对象。
    MemDestroy {
        element: Type,
        pointer: Box<Expr>,
    },
    /// `mem.copy<T>(dst, src)`：按值复制等长连续存储。
    MemCopy {
        element: Type,
        dst: Box<Expr>,
        src: Box<Expr>,
    },
    /// `mem.is_valid_utf8(bytes)`：只校验编码，不 trap。
    MemIsValidUtf8 {
        bytes: Box<Expr>,
    },
    /// `mem.view<T>` / `mem.view_const<T>`：从指针与长度构造视图。
    MemView {
        pointer: Box<Expr>,
        len: Box<Expr>,
    },
    /// `mem.cast_ptr<T>` / `mem.cast_const_ptr<T>`：显式指针转换并校验对齐。
    MemCast {
        pointer: Box<Expr>,
    },
    /// `s.bytes()`：字符串的只读字节视图（零分配）。
    StringBytes {
        base: Box<Expr>,
    },
    /// `string.from_bytes(bytes)`：校验 UTF-8 后返回字符串视图（零分配）。
    StringFromBytes {
        bytes: Box<Expr>,
    },
    Match {
        value: Box<Expr>,
        arms: Vec<MatchArm>,
    },
    /// `for` 迭代协议内部：判断枚举值是否为指定 variant（只看 tag，返回 `bool`）。
    EnumIsVariant {
        value: Box<Expr>,
        variant: usize,
    },
    /// `for` 迭代协议内部：读取枚举值指定 variant 的第 `field` 个 payload。
    EnumPayload {
        value: Box<Expr>,
        variant: usize,
        field: usize,
    },
}

/// 可寻址位置（place）：取址 `&place` 与写入 `place.field = ...` 的目标。
#[derive(Clone, Debug)]
pub struct Place {
    pub kind: PlaceKind,
    pub ty: Type,
    /// 该位置是否可写（`&var` 得到 `*T`，`&val` 得到 `*const T`）。
    pub mutable: bool,
}

#[derive(Clone, Debug)]
pub enum PlaceKind {
    Local(LocalId),
    Field { base: Box<Place>, field: usize },
    Index { base: Box<Place>, index: Box<Expr> },
    Deref { pointer: Box<Expr> },
}

#[derive(Clone, Debug)]
pub struct MatchArm {
    pub pattern: MatchPattern,
    pub body: Expr,
}

#[derive(Clone, Debug)]
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
            Type::Usize => formatter.write_str("usize"),
            Type::Isize => formatter.write_str("isize"),
            Type::F32 => formatter.write_str("f32"),
            Type::F64 => formatter.write_str("f64"),
            Type::Char => formatter.write_str("char"),
            Type::Bool => formatter.write_str("bool"),
            Type::String => formatter.write_str("string"),
            Type::Null => formatter.write_str("null"),
            Type::Array { element, length } => write!(formatter, "[{element}; {length}]"),
            Type::Struct(id) => write!(formatter, "struct@{}", id.0),
            Type::Enum(id) => write!(formatter, "enum@{}", id.0),
            Type::Ptr { pointee, mutable } => {
                if *mutable {
                    write!(formatter, "*{pointee}")
                } else {
                    write!(formatter, "*const {pointee}")
                }
            }
            Type::Slice { element, mutable } => {
                if *mutable {
                    write!(formatter, "[]{element}")
                } else {
                    write!(formatter, "[]const {element}")
                }
            }
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
            ScalarType::Usize => "usize",
            ScalarType::Isize => "isize",
            ScalarType::F32 => "f32",
            ScalarType::F64 => "f64",
            ScalarType::Char => "char",
            ScalarType::Bool => "bool",
            ScalarType::String => "string",
        })
    }
}
