//! Program 结构校验（H18-06 第一层）。
//!
//! 检查索引与种类、控制流、位置数量以及表达式/Place 的类型一致性。
//! 错误与后端无关，由 HIR/driver 转成 `Diagnostic`；本模块不依赖任何代码
//! 生成后端，也不调用递归 `layout`。按值布局环用带灰/黑标记的 DFS 单独检测，
//! 经指针/切片的间接递归保持合法。

use std::collections::HashSet;

use crate::ir::{
    Expr, ExprKind, Function, Instruction, LocalId, MatchPattern, Place, PlaceKind, PrintPart,
    Program, Terminator, Type, TypeDef,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyError {
    message: String,
}

impl VerifyError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for VerifyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for VerifyError {}

/// 校验一个完整 `Program`；成功返回 `Ok(())`，失败返回第一条错误。
pub fn verify_program(program: &Program) -> Result<(), VerifyError> {
    Verifier { program }.run()
}

struct Verifier<'a> {
    program: &'a Program,
}

impl Verifier<'_> {
    fn run(&self) -> Result<(), VerifyError> {
        self.check_types()?;
        self.check_type_cycles()?;
        if let Some(main) = self.program.main
            && main.0 >= self.program.functions.len()
        {
            return Err(VerifyError::new(format!(
                "main references unknown function #{}",
                main.0
            )));
        }
        for (index, function) in self.program.functions.iter().enumerate() {
            if function.id.0 != index {
                return Err(VerifyError::new(format!(
                    "function `{}` has id #{} at index {index}",
                    function.name, function.id.0
                )));
            }
            self.check_function(function)?;
        }
        Ok(())
    }

    /// IR-01：所有 `Type::Struct`/`Enum` 引用必须指向对应种类的 `TypeDef`，
    /// 指针/切片递归展开，数组长度非零。
    fn check_types(&self) -> Result<(), VerifyError> {
        let mut visited = HashSet::new();
        for (index, definition) in self.program.types.iter().enumerate() {
            match definition {
                TypeDef::Struct { fields, .. } => {
                    for field in fields {
                        self.check_type(&field.ty, &mut visited)
                            .map_err(|error| self.type_context(index, error))?;
                    }
                }
                TypeDef::Enum { variants } => {
                    for variant in variants {
                        for field in &variant.fields {
                            self.check_type(field, &mut visited)
                                .map_err(|error| self.type_context(index, error))?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn type_context(&self, index: usize, error: VerifyError) -> VerifyError {
        VerifyError::new(format!("type #{index}: {}", error.message))
    }

    fn check_type(&self, ty: &Type, visited: &mut HashSet<usize>) -> Result<(), VerifyError> {
        match ty {
            Type::Unit
            | Type::Null
            | Type::I8
            | Type::I16
            | Type::I32
            | Type::I64
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::U64
            | Type::Usize
            | Type::Isize
            | Type::F32
            | Type::F64
            | Type::Char
            | Type::Bool
            | Type::String => Ok(()),
            Type::Array { length, .. } => {
                if *length == 0 {
                    return Err(VerifyError::new("array length must be greater than zero"));
                }
                Ok(())
            }
            Type::Ptr { pointee, .. } => self.check_type(pointee, visited),
            Type::Slice { element, .. } => self.check_type(element, visited),
            Type::Struct(id) | Type::Enum(id) => {
                let Some(definition) = self.program.types.get(id.0) else {
                    return Err(VerifyError::new(format!(
                        "type reference #{} is out of range",
                        id.0
                    )));
                };
                match (ty, definition) {
                    (Type::Struct(_), TypeDef::Struct { .. }) => {}
                    (Type::Enum(_), TypeDef::Enum { .. }) => {}
                    (Type::Struct(_), TypeDef::Enum { .. }) => {
                        return Err(VerifyError::new(format!(
                            "type #{} is a struct reference but resolves to an enum",
                            id.0
                        )));
                    }
                    (Type::Enum(_), TypeDef::Struct { .. }) => {
                        return Err(VerifyError::new(format!(
                            "type #{} is an enum reference but resolves to a struct",
                            id.0
                        )));
                    }
                    _ => unreachable!("type id match covers both variants"),
                }
                if !visited.insert(id.0) {
                    return Ok(());
                }
                match definition {
                    TypeDef::Struct { fields, .. } => {
                        for field in fields {
                            self.check_type(&field.ty, visited)?;
                        }
                    }
                    TypeDef::Enum { variants } => {
                        for variant in variants {
                            for field in &variant.fields {
                                self.check_type(field, visited)?;
                            }
                        }
                    }
                }
                Ok(())
            }
        }
    }

    /// IR-01：按值布局自环/互环非法；经 `*T`/`[]T` 的递归合法。
    fn check_type_cycles(&self) -> Result<(), VerifyError> {
        let mut color = vec![0u8; self.program.types.len()];
        for index in 0..self.program.types.len() {
            self.visit_type(index, &mut color)?;
        }
        Ok(())
    }

    fn visit_type(&self, index: usize, color: &mut [u8]) -> Result<(), VerifyError> {
        match color[index] {
            2 => return Ok(()),
            1 => {
                return Err(VerifyError::new(format!(
                    "type #{index} contains itself by value"
                )));
            }
            _ => {}
        }
        color[index] = 1;
        for dependency in self.by_value_dependencies(index) {
            self.visit_type(dependency, color)?;
        }
        color[index] = 2;
        Ok(())
    }

    fn by_value_dependencies(&self, index: usize) -> Vec<usize> {
        let mut dependencies = Vec::new();
        match &self.program.types[index] {
            TypeDef::Struct { fields, .. } => {
                for field in fields {
                    if let Some(id) = by_value_type_id(&field.ty) {
                        dependencies.push(id);
                    }
                }
            }
            TypeDef::Enum { variants } => {
                for variant in variants {
                    for field in &variant.fields {
                        if let Some(id) = by_value_type_id(field) {
                            dependencies.push(id);
                        }
                    }
                }
            }
        }
        dependencies
    }

    fn check_function(&self, function: &Function) -> Result<(), VerifyError> {
        let context = |error: VerifyError| {
            VerifyError::new(format!("function `{}`: {}", function.name, error.message))
        };
        if function.external_link_name.is_some() {
            if !function.blocks.is_empty() {
                return Err(context(VerifyError::new(
                    "extern function must not have a body",
                )));
            }
        } else {
            if function.blocks.is_empty() {
                return Err(context(VerifyError::new("function has no basic blocks")));
            }
            if function.entry.0 >= function.blocks.len() {
                return Err(context(VerifyError::new(format!(
                    "entry block #{} is out of range",
                    function.entry.0
                ))));
            }
        }
        self.check_location(function.location.file)
            .map_err(context)?;
        self.check_source(function.source).map_err(context)?;
        let mut visited = HashSet::new();
        self.check_type(&function.return_type, &mut visited)
            .map_err(context)?;
        for local in &function.locals {
            self.check_type(local, &mut visited).map_err(context)?;
        }
        for parameter in &function.parameters {
            if parameter.0 >= function.locals.len() {
                return Err(context(VerifyError::new(format!(
                    "parameter local #{} is out of range",
                    parameter.0
                ))));
            }
        }
        for (index, block) in function.blocks.iter().enumerate() {
            self.check_block(function, index, block).map_err(context)?;
        }
        Ok(())
    }

    fn check_block(
        &self,
        function: &Function,
        block_index: usize,
        block: &crate::ir::BasicBlock,
    ) -> Result<(), VerifyError> {
        if block.instructions.len() != block.locations.len() {
            return Err(VerifyError::new(format!(
                "block #{block_index} has {} instructions but {} locations",
                block.instructions.len(),
                block.locations.len()
            )));
        }
        self.check_location(block.location.file)?;
        self.check_location(block.terminator_location.file)?;
        for location in &block.locations {
            self.check_location(location.file)?;
        }
        for instruction in &block.instructions {
            self.check_instruction(function, block_index, instruction)?;
        }
        self.check_terminator(function, block_index, &block.terminator)
    }

    fn check_location(&self, file: u32) -> Result<(), VerifyError> {
        if file as usize >= self.program.sources.len() {
            return Err(VerifyError::new(format!(
                "location references source #{file} but the program has {} sources",
                self.program.sources.len()
            )));
        }
        Ok(())
    }

    fn check_source(&self, source: u32) -> Result<(), VerifyError> {
        self.check_location(source)
    }

    fn check_instruction(
        &self,
        function: &Function,
        block_index: usize,
        instruction: &Instruction,
    ) -> Result<(), VerifyError> {
        match instruction {
            Instruction::SetLocal { local, value } => {
                self.check_local(function, *local)?;
                self.check_expr(function, value)?;
                if !types_coercible(&value.ty, &function.locals[local.0]) {
                    return Err(VerifyError::new(format!(
                        "block #{block_index}: SetLocal local #{} expects `{}` but value has type `{}`",
                        local.0, function.locals[local.0], value.ty
                    )));
                }
            }
            Instruction::SetFieldAt {
                place,
                field,
                value,
            } => {
                self.check_place(function, place)?;
                let Type::Struct(id) = &place.ty else {
                    return Err(VerifyError::new(format!(
                        "block #{block_index}: SetFieldAt target is `{}`, not a struct",
                        place.ty
                    )));
                };
                self.check_expr(function, value)?;
                let field_type = self.field_type(*id, *field)?;
                if !types_coercible(&value.ty, &field_type) {
                    return Err(VerifyError::new(format!(
                        "block #{block_index}: SetFieldAt field #{field} expects `{field_type}` but value has type `{}`",
                        value.ty
                    )));
                }
            }
            Instruction::SetIndexAt { place, value } => {
                self.check_place(function, place)?;
                self.check_expr(function, value)?;
                if !types_coercible(&value.ty, &place.ty) {
                    return Err(VerifyError::new(format!(
                        "block #{block_index}: SetIndexAt element expects `{}` but value has type `{}`",
                        place.ty, value.ty
                    )));
                }
            }
            Instruction::Evaluate(value) => self.check_expr(function, value)?,
            Instruction::Print(parts) => {
                for part in parts {
                    if let PrintPart::Value(value) = part {
                        self.check_expr(function, value)?;
                        if !is_printable(&value.ty) {
                            return Err(VerifyError::new(format!(
                                "block #{block_index}: cannot print value of type `{}`",
                                value.ty
                            )));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn check_terminator(
        &self,
        function: &Function,
        block_index: usize,
        terminator: &Terminator,
    ) -> Result<(), VerifyError> {
        match terminator {
            Terminator::Jump(target) => {
                self.check_block_id(function, *target)?;
            }
            Terminator::Branch {
                condition,
                then_block,
                else_block,
            } => {
                self.check_expr(function, condition)?;
                if condition.ty != Type::Bool {
                    return Err(VerifyError::new(format!(
                        "block #{block_index}: branch condition has type `{}`, not `bool`",
                        condition.ty
                    )));
                }
                self.check_block_id(function, *then_block)?;
                self.check_block_id(function, *else_block)?;
            }
            Terminator::Return(value) => {
                if let Some(value) = value {
                    self.check_expr(function, value)?;
                    if !types_coercible(&value.ty, &function.return_type) {
                        return Err(VerifyError::new(format!(
                            "block #{block_index}: return type `{}` does not match `{}`",
                            value.ty, function.return_type
                        )));
                    }
                } else if function.return_type != Type::Unit {
                    return Err(VerifyError::new(format!(
                        "block #{block_index}: empty return in function returning `{}`",
                        function.return_type
                    )));
                }
            }
        }
        Ok(())
    }

    fn check_expr(&self, function: &Function, expression: &Expr) -> Result<(), VerifyError> {
        match &expression.kind {
            ExprKind::Integer(_) => {
                if !expression.ty.is_integer() {
                    return Err(VerifyError::new(format!(
                        "integer literal has type `{}`",
                        expression.ty
                    )));
                }
            }
            ExprKind::Float(_) => {
                if !expression.ty.is_float() {
                    return Err(VerifyError::new(format!(
                        "float literal has type `{}`",
                        expression.ty
                    )));
                }
            }
            ExprKind::Char(_) => {
                if expression.ty != Type::Char {
                    return Err(VerifyError::new(format!(
                        "char literal has type `{}`",
                        expression.ty
                    )));
                }
            }
            ExprKind::Bool(_) => {
                if expression.ty != Type::Bool {
                    return Err(VerifyError::new(format!(
                        "bool literal has type `{}`",
                        expression.ty
                    )));
                }
            }
            ExprKind::String(_) => {
                if expression.ty != Type::String {
                    return Err(VerifyError::new(format!(
                        "string literal has type `{}`",
                        expression.ty
                    )));
                }
            }
            ExprKind::StringLength(value) => {
                self.check_expr(function, value)?;
                if value.ty != Type::String || expression.ty != Type::Usize {
                    return Err(VerifyError::new("string length must produce `usize`"));
                }
            }
            ExprKind::Array(elements) => {
                let Type::Array { element, length } = &expression.ty else {
                    return Err(VerifyError::new(format!(
                        "array literal has type `{}`",
                        expression.ty
                    )));
                };
                if elements.len() != *length {
                    return Err(VerifyError::new(format!(
                        "array literal has {} elements but type has length {length}",
                        elements.len()
                    )));
                }
                for item in elements {
                    self.check_expr(function, item)?;
                    if !types_compatible(&item.ty, &element.as_type()) {
                        return Err(VerifyError::new(format!(
                            "array element has type `{}` but element type is `{element}`",
                            item.ty
                        )));
                    }
                }
            }
            ExprKind::RepeatArray { value, length } => {
                self.check_expr(function, value)?;
                let Type::Array {
                    element,
                    length: declared,
                } = &expression.ty
                else {
                    return Err(VerifyError::new(format!(
                        "repeat array has type `{}`",
                        expression.ty
                    )));
                };
                if length != declared {
                    return Err(VerifyError::new(format!(
                        "repeat array length {length} does not match type length {declared}"
                    )));
                }
                if !types_compatible(&value.ty, &element.as_type()) {
                    return Err(VerifyError::new(format!(
                        "repeat array element has type `{}` but element type is `{element}`",
                        value.ty
                    )));
                }
            }
            ExprKind::Index { array, index } => {
                self.check_expr(function, array)?;
                self.check_expr(function, index)?;
                let expected = match &array.ty {
                    Type::Array { element, .. } => {
                        if index.ty != Type::I32 {
                            return Err(VerifyError::new(format!(
                                "array index has type `{}`, expected `i32`",
                                index.ty
                            )));
                        }
                        element.as_type()
                    }
                    Type::Slice { element, .. } => {
                        if index.ty != Type::Usize {
                            return Err(VerifyError::new(format!(
                                "slice index has type `{}`, expected `usize`",
                                index.ty
                            )));
                        }
                        (**element).clone()
                    }
                    other => {
                        return Err(VerifyError::new(format!(
                            "cannot index value of type `{other}`"
                        )));
                    }
                };
                if !types_coercible(&expression.ty, &expected) {
                    return Err(VerifyError::new(format!(
                        "index expression has type `{}` but element type is `{expected}`",
                        expression.ty
                    )));
                }
            }
            ExprKind::Local(local) => {
                self.check_local(function, *local)?;
                if !types_coercible(&expression.ty, &function.locals[local.0]) {
                    return Err(VerifyError::new(format!(
                        "local #{} has type `{}` but expression type is `{}`",
                        local.0, function.locals[local.0], expression.ty
                    )));
                }
            }
            ExprKind::Call {
                function: callee,
                arguments,
            } => {
                if callee.0 >= self.program.functions.len() {
                    return Err(VerifyError::new(format!(
                        "call to unknown function #{}",
                        callee.0
                    )));
                }
                let target = &self.program.functions[callee.0];
                if arguments.len() != target.parameters.len() {
                    return Err(VerifyError::new(format!(
                        "call to `{}` expects {} arguments but {} were provided",
                        target.name,
                        target.parameters.len(),
                        arguments.len()
                    )));
                }
                for (argument, parameter) in arguments.iter().zip(&target.parameters) {
                    self.check_expr(function, argument)?;
                    let expected = &target.locals[parameter.0];
                    if !types_coercible(&argument.ty, expected) {
                        return Err(VerifyError::new(format!(
                            "argument has type `{}` but `{}` expects `{expected}`",
                            argument.ty, target.name
                        )));
                    }
                }
                if !types_coercible(&expression.ty, &target.return_type) {
                    return Err(VerifyError::new(format!(
                        "call result has type `{}` but `{}` returns `{}`",
                        expression.ty, target.name, target.return_type
                    )));
                }
            }
            ExprKind::Unary { operator, operand } => {
                self.check_expr(function, operand)?;
                match operator {
                    dolphin_syntax::ast::UnaryOperator::Negate => {
                        if !operand.ty.is_integer() && !operand.ty.is_float() {
                            return Err(VerifyError::new(format!(
                                "cannot negate value of type `{}`",
                                operand.ty
                            )));
                        }
                        if expression.ty != operand.ty {
                            return Err(VerifyError::new(format!(
                                "negation type `{}` does not match operand `{}`",
                                expression.ty, operand.ty
                            )));
                        }
                    }
                    dolphin_syntax::ast::UnaryOperator::Not => {
                        if operand.ty != Type::Bool || expression.ty != Type::Bool {
                            return Err(VerifyError::new("logical not requires `bool`"));
                        }
                    }
                }
            }
            ExprKind::Binary {
                operator,
                left,
                right,
            } => {
                self.check_expr(function, left)?;
                self.check_expr(function, right)?;
                self.check_binary(expression, operator, left, right)?;
            }
            ExprKind::Cast { value, to } => {
                self.check_expr(function, value)?;
                if expression.ty != *to {
                    return Err(VerifyError::new(format!(
                        "cast result has type `{}` but target is `{to}`",
                        expression.ty
                    )));
                }
                if !is_castable(&value.ty) || !is_castable(to) {
                    return Err(VerifyError::new(format!(
                        "cannot cast `{}` to `{to}`",
                        value.ty
                    )));
                }
            }
            ExprKind::StructInit { fields } => {
                let Type::Struct(id) = &expression.ty else {
                    return Err(VerifyError::new(format!(
                        "struct initializer has type `{}`",
                        expression.ty
                    )));
                };
                let TypeDef::Struct {
                    fields: declared, ..
                } = &self.program.types[id.0]
                else {
                    return Err(VerifyError::new(format!("type #{} is not a struct", id.0)));
                };
                if fields.len() != declared.len() {
                    return Err(VerifyError::new(format!(
                        "struct initializer has {} fields but type has {}",
                        fields.len(),
                        declared.len()
                    )));
                }
                for (value, field) in fields.iter().zip(declared) {
                    self.check_expr(function, value)?;
                    if !types_coercible(&value.ty, &field.ty) {
                        return Err(VerifyError::new(format!(
                            "field `{}` expects `{}` but value has type `{}`",
                            field.name, field.ty, value.ty
                        )));
                    }
                }
            }
            ExprKind::EnumInit { variant, arguments } => {
                let Type::Enum(id) = &expression.ty else {
                    return Err(VerifyError::new(format!(
                        "enum initializer has type `{}`",
                        expression.ty
                    )));
                };
                let TypeDef::Enum { variants } = &self.program.types[id.0] else {
                    return Err(VerifyError::new(format!("type #{} is not an enum", id.0)));
                };
                let Some(declared) = variants.get(*variant) else {
                    return Err(VerifyError::new(format!(
                        "enum #{} has no variant #{variant}",
                        id.0
                    )));
                };
                if arguments.len() != declared.fields.len() {
                    return Err(VerifyError::new(format!(
                        "variant `{}` expects {} arguments but {} were provided",
                        declared.name,
                        declared.fields.len(),
                        arguments.len()
                    )));
                }
                for (value, field) in arguments.iter().zip(&declared.fields) {
                    self.check_expr(function, value)?;
                    if !types_coercible(&value.ty, field) {
                        return Err(VerifyError::new(format!(
                            "variant `{}` field expects `{field}` but value has type `{}`",
                            declared.name, value.ty
                        )));
                    }
                }
            }
            ExprKind::Field { base, field } => {
                self.check_expr(function, base)?;
                let Type::Struct(id) = &base.ty else {
                    return Err(VerifyError::new(format!(
                        "cannot access field on value of type `{}`",
                        base.ty
                    )));
                };
                let field_type = self.field_type(*id, *field)?;
                if !types_coercible(&expression.ty, &field_type) {
                    return Err(VerifyError::new(format!(
                        "field #{field} has type `{field_type}` but expression type is `{}`",
                        expression.ty
                    )));
                }
            }
            ExprKind::AddressOf { place } => {
                self.check_place(function, place)?;
                let expected = Type::Ptr {
                    pointee: Box::new(place.ty.clone()),
                    mutable: place.mutable,
                };
                if !types_coercible(&expression.ty, &expected) {
                    return Err(VerifyError::new(format!(
                        "address-of has type `{}` but expected `{expected}`",
                        expression.ty
                    )));
                }
            }
            ExprKind::Deref { pointer } => {
                self.check_expr(function, pointer)?;
                let Type::Ptr { pointee, .. } = &pointer.ty else {
                    return Err(VerifyError::new(format!(
                        "cannot dereference value of type `{}`",
                        pointer.ty
                    )));
                };
                if !types_coercible(&expression.ty, pointee) {
                    return Err(VerifyError::new(format!(
                        "deref has type `{}` but pointee is `{}`",
                        expression.ty, pointee
                    )));
                }
            }
            ExprKind::Null => {
                if expression.ty != Type::Null && !expression.ty.is_pointer() {
                    return Err(VerifyError::new(format!(
                        "null literal has type `{}`",
                        expression.ty
                    )));
                }
            }
            ExprKind::SlicePtr { base } => {
                self.check_expr(function, base)?;
                let (element, mutable) = match &base.ty {
                    Type::Slice { element, mutable } => ((**element).clone(), *mutable),
                    Type::String => (Type::U8, false),
                    other => {
                        return Err(VerifyError::new(format!(
                            "cannot take `.ptr` of value of type `{other}`"
                        )));
                    }
                };
                let expected = Type::Ptr {
                    pointee: Box::new(element),
                    mutable,
                };
                if !types_coercible(&expression.ty, &expected) {
                    return Err(VerifyError::new(format!(
                        "slice pointer has type `{}` but expected `{expected}`",
                        expression.ty
                    )));
                }
            }
            ExprKind::SliceLen { base } => {
                self.check_expr(function, base)?;
                if !matches!(base.ty, Type::Slice { .. } | Type::String)
                    || expression.ty != Type::Usize
                {
                    return Err(VerifyError::new("slice length must produce `usize`"));
                }
            }
            ExprKind::SliceRange { base, start, end } => {
                self.check_expr(function, base)?;
                self.check_expr(function, start)?;
                self.check_expr(function, end)?;
                if !matches!(base.ty, Type::Slice { .. }) {
                    return Err(VerifyError::new(format!(
                        "cannot slice value of type `{}`",
                        base.ty
                    )));
                }
                if start.ty != Type::Usize || end.ty != Type::Usize {
                    return Err(VerifyError::new("slice range bounds must be `usize`"));
                }
                if !types_coercible(&expression.ty, &base.ty) {
                    return Err(VerifyError::new(format!(
                        "slice result has type `{}` but base is `{}`",
                        expression.ty, base.ty
                    )));
                }
            }
            ExprKind::MemAlloc { element, count } => {
                let mut visited = HashSet::new();
                self.check_type(element, &mut visited)?;
                self.check_expr(function, count)?;
                if count.ty != Type::Usize {
                    return Err(VerifyError::new("allocation count must be `usize`"));
                }
                let expected = Type::Slice {
                    element: Box::new(element.clone()),
                    mutable: true,
                };
                if expression.ty != expected {
                    return Err(VerifyError::new(format!(
                        "allocation has type `{}` but expected `{expected}`",
                        expression.ty
                    )));
                }
            }
            ExprKind::MemFree { element, buffer } => {
                let mut visited = HashSet::new();
                self.check_type(element, &mut visited)?;
                self.check_expr(function, buffer)?;
                if !matches!(buffer.ty, Type::Slice { .. }) || expression.ty != Type::Unit {
                    return Err(VerifyError::new("free expects a slice and produces `Unit`"));
                }
            }
            ExprKind::MemCreate { element, value } => {
                let mut visited = HashSet::new();
                self.check_type(element, &mut visited)?;
                self.check_expr(function, value)?;
                if !types_coercible(&value.ty, element) {
                    return Err(VerifyError::new(format!(
                        "create value has type `{}` but element is `{element}`",
                        value.ty
                    )));
                }
                let expected = Type::Ptr {
                    pointee: Box::new(element.clone()),
                    mutable: true,
                };
                if expression.ty != expected {
                    return Err(VerifyError::new(format!(
                        "create has type `{}` but expected `{expected}`",
                        expression.ty
                    )));
                }
            }
            ExprKind::MemDestroy { element, pointer } => {
                let mut visited = HashSet::new();
                self.check_type(element, &mut visited)?;
                self.check_expr(function, pointer)?;
                if !pointer.ty.is_pointer() || expression.ty != Type::Unit {
                    return Err(VerifyError::new(
                        "destroy expects a pointer and produces `Unit`",
                    ));
                }
            }
            ExprKind::MemCopy { element, dst, src } => {
                let mut visited = HashSet::new();
                self.check_type(element, &mut visited)?;
                self.check_expr(function, dst)?;
                self.check_expr(function, src)?;
                if !matches!(dst.ty, Type::Slice { .. })
                    || !matches!(src.ty, Type::Slice { .. })
                    || expression.ty != Type::Unit
                {
                    return Err(VerifyError::new(
                        "copy expects two slices and produces `Unit`",
                    ));
                }
            }
            ExprKind::MemIsValidUtf8 { bytes } => {
                self.check_expr(function, bytes)?;
                if !matches!(
                    &bytes.ty,
                    Type::Slice { element, .. } if **element == Type::U8
                ) || expression.ty != Type::Bool
                {
                    return Err(VerifyError::new(
                        "is_valid_utf8 expects a `[]const u8` and produces `bool`",
                    ));
                }
            }
            ExprKind::MemView { pointer, len } => {
                self.check_expr(function, pointer)?;
                self.check_expr(function, len)?;
                let pointee = match &pointer.ty {
                    Type::Ptr { pointee, .. } => (**pointee).clone(),
                    // `mem.view<T>(null, len)` 在运行时以 101 失败；类型仍然合法。
                    Type::Null => match &expression.ty {
                        Type::Slice { element, .. } => (**element).clone(),
                        _ => {
                            return Err(VerifyError::new(format!(
                                "view has type `{}` with a null pointer",
                                expression.ty
                            )));
                        }
                    },
                    other => {
                        return Err(VerifyError::new(format!(
                            "view expects a pointer, found `{other}`"
                        )));
                    }
                };
                if len.ty != Type::Usize {
                    return Err(VerifyError::new("view length must be `usize`"));
                }
                // HIR 会用 `mem.view` 生成 `[]const` 只读视图（如数组 `for`），
                // 因此这里只要求元素类型一致，不约束可变性。
                if !matches!(&expression.ty, Type::Slice { element, .. } if **element == pointee) {
                    return Err(VerifyError::new(format!(
                        "view has type `{}` but element is `{pointee}`",
                        expression.ty
                    )));
                }
            }
            ExprKind::MemCast { pointer } => {
                self.check_expr(function, pointer)?;
                if !pointer.ty.is_pointer() && pointer.ty != Type::Null {
                    return Err(VerifyError::new(format!(
                        "cast_ptr expects a pointer, found `{}`",
                        pointer.ty
                    )));
                }
                if !expression.ty.is_pointer() && expression.ty != Type::Null {
                    return Err(VerifyError::new(format!(
                        "cast_ptr result has type `{}`",
                        expression.ty
                    )));
                }
            }
            ExprKind::StringBytes { base } => {
                self.check_expr(function, base)?;
                let expected = Type::Slice {
                    element: Box::new(Type::U8),
                    mutable: false,
                };
                if base.ty != Type::String || expression.ty != expected {
                    return Err(VerifyError::new(
                        "bytes() expects `string` and produces `[]const u8`",
                    ));
                }
            }
            ExprKind::StringFromBytes { bytes } => {
                self.check_expr(function, bytes)?;
                if !matches!(
                    &bytes.ty,
                    Type::Slice { element, .. } if **element == Type::U8
                ) || expression.ty != Type::String
                {
                    return Err(VerifyError::new(
                        "string.from_bytes expects a `[]const u8` and produces `string`",
                    ));
                }
            }
            ExprKind::Match { value, arms } => {
                self.check_expr(function, value)?;
                let Type::Enum(id) = &value.ty else {
                    return Err(VerifyError::new(format!(
                        "match subject has type `{}`, not an enum",
                        value.ty
                    )));
                };
                let TypeDef::Enum { variants } = &self.program.types[id.0] else {
                    return Err(VerifyError::new(format!("type #{} is not an enum", id.0)));
                };
                for arm in arms {
                    if let MatchPattern::Variant { variant, bindings } = &arm.pattern {
                        let Some(declared) = variants.get(*variant) else {
                            return Err(VerifyError::new(format!(
                                "enum #{} has no variant #{variant}",
                                id.0
                            )));
                        };
                        if bindings.len() != declared.fields.len() {
                            return Err(VerifyError::new(format!(
                                "variant `{}` has {} fields but {} bindings were provided",
                                declared.name,
                                declared.fields.len(),
                                bindings.len()
                            )));
                        }
                        for (binding, field) in bindings.iter().zip(&declared.fields) {
                            self.check_local(function, *binding)?;
                            if function.locals[binding.0] != *field {
                                return Err(VerifyError::new(format!(
                                    "variant `{}` binding has type `{}` but field is `{field}`",
                                    declared.name, function.locals[binding.0]
                                )));
                            }
                        }
                    }
                    self.check_expr(function, &arm.body)?;
                    if !types_coercible(&arm.body.ty, &expression.ty) {
                        return Err(VerifyError::new(format!(
                            "match arm has type `{}` but match result is `{}`",
                            arm.body.ty, expression.ty
                        )));
                    }
                }
            }
            ExprKind::EnumIsVariant { value, variant } => {
                self.check_expr(function, value)?;
                let Type::Enum(id) = &value.ty else {
                    return Err(VerifyError::new(format!(
                        "variant check subject has type `{}`",
                        value.ty
                    )));
                };
                if !self.enum_has_variant(*id, *variant) || expression.ty != Type::Bool {
                    return Err(VerifyError::new(format!(
                        "enum #{} has no variant #{variant} or result is not `bool`",
                        id.0
                    )));
                }
            }
            ExprKind::EnumPayload {
                value,
                variant,
                field,
            } => {
                self.check_expr(function, value)?;
                let Type::Enum(id) = &value.ty else {
                    return Err(VerifyError::new(format!(
                        "payload read subject has type `{}`",
                        value.ty
                    )));
                };
                let TypeDef::Enum { variants } = &self.program.types[id.0] else {
                    return Err(VerifyError::new(format!("type #{} is not an enum", id.0)));
                };
                let Some(declared) = variants.get(*variant) else {
                    return Err(VerifyError::new(format!(
                        "enum #{} has no variant #{variant}",
                        id.0
                    )));
                };
                let Some(field_type) = declared.fields.get(*field) else {
                    return Err(VerifyError::new(format!(
                        "variant `{}` has no field #{field}",
                        declared.name
                    )));
                };
                if expression.ty != *field_type {
                    return Err(VerifyError::new(format!(
                        "payload has type `{}` but field is `{field_type}`",
                        expression.ty
                    )));
                }
            }
        }
        Ok(())
    }

    fn check_binary(
        &self,
        expression: &Expr,
        operator: &dolphin_syntax::ast::BinaryOperator,
        left: &Expr,
        right: &Expr,
    ) -> Result<(), VerifyError> {
        use dolphin_syntax::ast::BinaryOperator;
        match operator {
            BinaryOperator::And | BinaryOperator::Or => {
                if left.ty != Type::Bool || right.ty != Type::Bool || expression.ty != Type::Bool {
                    return Err(VerifyError::new(
                        "logical operators require `bool` operands",
                    ));
                }
            }
            BinaryOperator::Equal | BinaryOperator::NotEqual => {
                if !types_compatible(&left.ty, &right.ty) && !types_compatible(&right.ty, &left.ty)
                {
                    return Err(VerifyError::new(format!(
                        "cannot compare `{}` with `{}`",
                        left.ty, right.ty
                    )));
                }
                if expression.ty != Type::Bool {
                    return Err(VerifyError::new("comparison result must be `bool`"));
                }
            }
            BinaryOperator::Less
            | BinaryOperator::LessEqual
            | BinaryOperator::Greater
            | BinaryOperator::GreaterEqual => {
                if left.ty != right.ty {
                    return Err(VerifyError::new(format!(
                        "comparison operands have types `{}` and `{}`",
                        left.ty, right.ty
                    )));
                }
                if expression.ty != Type::Bool {
                    return Err(VerifyError::new("comparison result must be `bool`"));
                }
            }
            BinaryOperator::Add
            | BinaryOperator::Subtract
            | BinaryOperator::Multiply
            | BinaryOperator::Divide
            | BinaryOperator::Remainder => {
                if left.ty != right.ty || expression.ty != left.ty {
                    return Err(VerifyError::new(format!(
                        "arithmetic operands have types `{}`, `{}` and result `{}`",
                        left.ty, right.ty, expression.ty
                    )));
                }
            }
        }
        Ok(())
    }

    fn check_place(&self, function: &Function, place: &Place) -> Result<(), VerifyError> {
        match &place.kind {
            PlaceKind::Local(local) => {
                self.check_local(function, *local)?;
                if place.ty != function.locals[local.0] {
                    return Err(VerifyError::new(format!(
                        "place local #{} has type `{}` but place type is `{}`",
                        local.0, function.locals[local.0], place.ty
                    )));
                }
            }
            PlaceKind::Field { base, field } => {
                self.check_place(function, base)?;
                let Type::Struct(id) = &base.ty else {
                    return Err(VerifyError::new(format!(
                        "field place base has type `{}`, not a struct",
                        base.ty
                    )));
                };
                let field_type = self.field_type(*id, *field)?;
                if place.ty != field_type {
                    return Err(VerifyError::new(format!(
                        "field place has type `{}` but field is `{field_type}`",
                        place.ty
                    )));
                }
            }
            PlaceKind::Index { base, index } => {
                self.check_place(function, base)?;
                self.check_expr(function, index)?;
                let expected = match &base.ty {
                    Type::Array { element, .. } => {
                        if index.ty != Type::I32 {
                            return Err(VerifyError::new(format!(
                                "array place index has type `{}`, expected `i32`",
                                index.ty
                            )));
                        }
                        element.as_type()
                    }
                    Type::Slice { element, .. } => {
                        if index.ty != Type::Usize {
                            return Err(VerifyError::new(format!(
                                "slice place index has type `{}`, expected `usize`",
                                index.ty
                            )));
                        }
                        (**element).clone()
                    }
                    other => {
                        return Err(VerifyError::new(format!(
                            "index place base has type `{other}`"
                        )));
                    }
                };
                if place.ty != expected {
                    return Err(VerifyError::new(format!(
                        "index place has type `{}` but element is `{expected}`",
                        place.ty
                    )));
                }
            }
            PlaceKind::Deref { pointer } => {
                self.check_expr(function, pointer)?;
                let Type::Ptr { pointee, .. } = &pointer.ty else {
                    return Err(VerifyError::new(format!(
                        "deref place pointer has type `{}`",
                        pointer.ty
                    )));
                };
                if place.ty != **pointee {
                    return Err(VerifyError::new(format!(
                        "deref place has type `{}` but pointee is `{}`",
                        place.ty, pointee
                    )));
                }
            }
        }
        Ok(())
    }

    fn check_local(&self, function: &Function, local: LocalId) -> Result<(), VerifyError> {
        if local.0 >= function.locals.len() {
            return Err(VerifyError::new(format!(
                "local reference #{} is out of range",
                local.0
            )));
        }
        Ok(())
    }

    fn check_block_id(
        &self,
        function: &Function,
        block: crate::ir::BlockId,
    ) -> Result<(), VerifyError> {
        if block.0 >= function.blocks.len() {
            return Err(VerifyError::new(format!(
                "block target #{} is out of range",
                block.0
            )));
        }
        Ok(())
    }

    fn field_type(&self, id: crate::ir::TypeId, field: usize) -> Result<Type, VerifyError> {
        let TypeDef::Struct { fields, .. } = &self.program.types[id.0] else {
            return Err(VerifyError::new(format!("type #{} is not a struct", id.0)));
        };
        fields
            .get(field)
            .map(|field| field.ty.clone())
            .ok_or_else(|| VerifyError::new(format!("type #{} has no field #{field}", id.0)))
    }

    fn enum_has_variant(&self, id: crate::ir::TypeId, variant: usize) -> bool {
        matches!(
            &self.program.types[id.0],
            TypeDef::Enum { variants } if variant < variants.len()
        )
    }
}

fn by_value_type_id(ty: &Type) -> Option<usize> {
    match ty {
        Type::Struct(id) | Type::Enum(id) => Some(id.0),
        _ => None,
    }
}

/// 双向容忍 HIR 在变量初始化时写入的只读限定（`value.ty = declared`）：
/// 结构自然类型与表达式类型允许只差一层 const。
fn types_coercible(left: &Type, right: &Type) -> bool {
    types_compatible(left, right) || types_compatible(right, left)
}

fn is_castable(ty: &Type) -> bool {
    ty.is_integer() || ty.is_float() || *ty == Type::Char
}

fn is_printable(ty: &Type) -> bool {
    ty.is_integer() || ty.is_float() || matches!(ty, Type::Bool | Type::Char | Type::String)
}

/// 允许合法只读限定转换（`*T` -> `*const T`、`[]T` -> `[]const T`）与
/// `null` 对指针的匹配；其余要求类型完全一致。
fn types_compatible(actual: &Type, expected: &Type) -> bool {
    if actual == expected {
        return true;
    }
    match (actual, expected) {
        (Type::Null, Type::Ptr { .. }) => true,
        (Type::Ptr { .. }, Type::Null) => true,
        (
            Type::Ptr {
                pointee: actual,
                mutable: true,
            },
            Type::Ptr {
                pointee: expected,
                mutable: false,
            },
        ) => actual == expected,
        (
            Type::Slice {
                element: actual,
                mutable: true,
            },
            Type::Slice {
                element: expected,
                mutable: false,
            },
        ) => actual == expected,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{BasicBlock, BlockId, EnumVariant, FunctionId, Location, StructField, TypeId};
    use std::path::PathBuf;

    fn location() -> Location {
        Location {
            file: 0,
            line: 1,
            column: 1,
        }
    }

    fn block(instructions: Vec<Instruction>, terminator: Terminator) -> BasicBlock {
        let locations = instructions.iter().map(|_| location()).collect();
        BasicBlock {
            instructions,
            terminator,
            location: location(),
            locations,
            terminator_location: location(),
        }
    }

    fn function_with_id(
        id: usize,
        name: &str,
        locals: Vec<Type>,
        parameters: Vec<LocalId>,
        return_type: Type,
        blocks: Vec<BasicBlock>,
    ) -> Function {
        Function {
            id: FunctionId(id),
            name: name.to_string(),
            parameters,
            return_type,
            locals,
            blocks,
            entry: BlockId(0),
            external_link_name: None,
            source: 0,
            location: location(),
        }
    }

    fn main_with(
        locals: Vec<Type>,
        instructions: Vec<Instruction>,
        terminator: Terminator,
    ) -> Program {
        Program {
            functions: vec![function_with_id(
                0,
                "main",
                locals,
                Vec::new(),
                Type::I32,
                vec![block(instructions, terminator)],
            )],
            main: Some(FunctionId(0)),
            types: Vec::new(),
            sources: vec![PathBuf::from("test.do")],
        }
    }

    fn empty_struct() -> TypeDef {
        TypeDef::Struct {
            fields: Vec::new(),
            extern_c: false,
        }
    }

    #[test]
    fn valid_minimal_program_passes() {
        let program = main_with(
            vec![Type::I32],
            vec![Instruction::SetLocal {
                local: LocalId(0),
                value: Expr::i32(1),
            }],
            Terminator::Return(Some(Expr::i32(0))),
        );
        assert_eq!(verify_program(&program), Ok(()));
    }

    #[test]
    fn struct_kind_mismatch_is_rejected() {
        let mut program = main_with(vec![], vec![], Terminator::Return(Some(Expr::i32(0))));
        program.types = vec![empty_struct()];
        program.functions[0].locals = vec![Type::Enum(TypeId(0))];
        let error = verify_program(&program).unwrap_err();
        assert!(error.message().contains("resolves to a struct"), "{error}");
    }

    #[test]
    fn value_self_cycle_is_rejected() {
        let mut program = main_with(vec![], vec![], Terminator::Return(Some(Expr::i32(0))));
        program.types = vec![TypeDef::Struct {
            fields: vec![StructField {
                name: "next".to_string(),
                ty: Type::Struct(TypeId(0)),
                public: true,
            }],
            extern_c: false,
        }];
        let error = verify_program(&program).unwrap_err();
        assert!(
            error.message().contains("contains itself by value"),
            "{error}"
        );
    }

    #[test]
    fn value_mutual_cycle_is_rejected() {
        let mut program = main_with(vec![], vec![], Terminator::Return(Some(Expr::i32(0))));
        program.types = vec![
            TypeDef::Struct {
                fields: vec![StructField {
                    name: "other".to_string(),
                    ty: Type::Struct(TypeId(1)),
                    public: true,
                }],
                extern_c: false,
            },
            TypeDef::Struct {
                fields: vec![StructField {
                    name: "other".to_string(),
                    ty: Type::Struct(TypeId(0)),
                    public: true,
                }],
                extern_c: false,
            },
        ];
        let error = verify_program(&program).unwrap_err();
        assert!(
            error.message().contains("contains itself by value"),
            "{error}"
        );
    }

    #[test]
    fn pointer_recursion_is_allowed() {
        let mut program = main_with(vec![], vec![], Terminator::Return(Some(Expr::i32(0))));
        program.types = vec![TypeDef::Struct {
            fields: vec![StructField {
                name: "next".to_string(),
                ty: Type::Ptr {
                    pointee: Box::new(Type::Struct(TypeId(0))),
                    mutable: true,
                },
                public: true,
            }],
            extern_c: false,
        }];
        assert_eq!(verify_program(&program), Ok(()));
    }

    #[test]
    fn invalid_local_index_is_rejected() {
        let program = main_with(
            vec![Type::I32],
            vec![Instruction::SetLocal {
                local: LocalId(3),
                value: Expr::i32(1),
            }],
            Terminator::Return(Some(Expr::i32(0))),
        );
        let error = verify_program(&program).unwrap_err();
        assert!(error.message().contains("local reference #3"), "{error}");
    }

    #[test]
    fn invalid_block_target_is_rejected() {
        let program = main_with(vec![], vec![], Terminator::Jump(BlockId(5)));
        let error = verify_program(&program).unwrap_err();
        assert!(error.message().contains("block target #5"), "{error}");
    }

    #[test]
    fn location_count_mismatch_is_rejected() {
        let mut program = main_with(
            vec![Type::I32],
            vec![
                Instruction::SetLocal {
                    local: LocalId(0),
                    value: Expr::i32(1),
                },
                Instruction::SetLocal {
                    local: LocalId(0),
                    value: Expr::i32(2),
                },
            ],
            Terminator::Return(Some(Expr::i32(0))),
        );
        program.functions[0].blocks[0].locations.pop();
        let error = verify_program(&program).unwrap_err();
        assert!(
            error.message().contains("2 instructions but 1 locations"),
            "{error}"
        );
    }

    #[test]
    fn branch_condition_must_be_bool() {
        let program = main_with(
            vec![],
            vec![],
            Terminator::Branch {
                condition: Expr::i32(1),
                then_block: BlockId(0),
                else_block: BlockId(0),
            },
        );
        let error = verify_program(&program).unwrap_err();
        assert!(error.message().contains("branch condition"), "{error}");
    }

    #[test]
    fn set_local_type_mismatch_is_rejected() {
        let program = main_with(
            vec![Type::I32],
            vec![Instruction::SetLocal {
                local: LocalId(0),
                value: Expr {
                    kind: ExprKind::Float(1.0),
                    ty: Type::F64,
                },
            }],
            Terminator::Return(Some(Expr::i32(0))),
        );
        let error = verify_program(&program).unwrap_err();
        assert!(error.message().contains("SetLocal"), "{error}");
    }

    #[test]
    fn call_arity_mismatch_is_rejected() {
        let callee = function_with_id(
            1,
            "callee",
            vec![],
            vec![],
            Type::I32,
            vec![block(vec![], Terminator::Return(Some(Expr::i32(0))))],
        );
        let mut program = main_with(
            vec![],
            vec![Instruction::Evaluate(Expr {
                kind: ExprKind::Call {
                    function: FunctionId(1),
                    arguments: vec![Expr::i32(1)],
                },
                ty: Type::I32,
            })],
            Terminator::Return(Some(Expr::i32(0))),
        );
        program.functions.push(callee);
        let error = verify_program(&program).unwrap_err();
        assert!(error.message().contains("expects 0 arguments"), "{error}");
    }

    #[test]
    fn extern_without_body_is_allowed() {
        let mut program = main_with(vec![], vec![], Terminator::Return(Some(Expr::i32(0))));
        let mut external = function_with_id(1, "c_func", vec![], vec![], Type::I32, Vec::new());
        external.external_link_name = Some("c_func".to_string());
        program.functions.push(external);
        assert_eq!(verify_program(&program), Ok(()));
    }

    #[test]
    fn library_without_main_is_allowed() {
        let mut program = main_with(vec![], vec![], Terminator::Return(Some(Expr::i32(0))));
        program.main = None;
        assert_eq!(verify_program(&program), Ok(()));
    }

    #[test]
    fn invalid_source_is_rejected() {
        let mut program = main_with(vec![], vec![], Terminator::Return(Some(Expr::i32(0))));
        program.functions[0].source = 5;
        let error = verify_program(&program).unwrap_err();
        assert!(error.message().contains("source #5"), "{error}");
    }

    #[test]
    fn enum_variant_out_of_range_is_rejected() {
        let mut program = main_with(
            vec![],
            vec![Instruction::Evaluate(Expr {
                kind: ExprKind::EnumInit {
                    variant: 3,
                    arguments: Vec::new(),
                },
                ty: Type::Enum(TypeId(0)),
            })],
            Terminator::Return(Some(Expr::i32(0))),
        );
        program.types = vec![TypeDef::Enum {
            variants: vec![EnumVariant {
                name: "Only".to_string(),
                fields: Vec::new(),
            }],
        }];
        let error = verify_program(&program).unwrap_err();
        assert!(error.message().contains("no variant #3"), "{error}");
    }

    #[test]
    fn return_type_mismatch_is_rejected() {
        let mut program = main_with(vec![], vec![], Terminator::Return(Some(Expr::i32(0))));
        program.functions[0].return_type = Type::Bool;
        let error = verify_program(&program).unwrap_err();
        assert!(error.message().contains("return type"), "{error}");
    }

    #[test]
    fn empty_return_in_non_unit_is_rejected() {
        let program = main_with(vec![], vec![], Terminator::Return(None));
        let error = verify_program(&program).unwrap_err();
        assert!(error.message().contains("empty return"), "{error}");
    }

    #[test]
    fn print_non_printable_is_rejected() {
        let mut program = main_with(
            vec![Type::Struct(TypeId(0))],
            vec![Instruction::Print(vec![PrintPart::Value(Expr {
                kind: ExprKind::Local(LocalId(0)),
                ty: Type::Struct(TypeId(0)),
            })])],
            Terminator::Return(Some(Expr::i32(0))),
        );
        program.types = vec![empty_struct()];
        let error = verify_program(&program).unwrap_err();
        assert!(error.message().contains("cannot print"), "{error}");
    }
}
