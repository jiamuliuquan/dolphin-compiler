use std::collections::{HashMap, HashSet};

use crate::ast::{
    self, AssignmentOperator, BinaryOperator, ExprKind, ForIterable, Statement, StatementKind,
    TryResource, TypeRefKind, UnaryOperator,
};
use crate::diagnostic::Diagnostic;
use crate::ir::{
    self, BasicBlock, BlockId, EnumVariant, Expr, FunctionId, Instruction, LocalId, Pointer,
    PrintPart, StructField, Terminator, Type, TypeDef, TypeId,
};
use crate::source::{SourceFile, Span};

pub fn lower(source: &SourceFile, program: &ast::Program) -> Result<ir::Program, Diagnostic> {
    lower_sources(std::slice::from_ref(source), program)
}

pub fn lower_sources(
    sources: &[SourceFile],
    program: &ast::Program,
) -> Result<ir::Program, Diagnostic> {
    ProgramLowerer::new(sources, program).lower()
}

#[derive(Clone)]
struct FunctionSignature {
    id: FunctionId,
    parameters: Vec<Type>,
    return_type: Type,
}

struct ProgramLowerer<'a> {
    sources: &'a [SourceFile],
    ast: &'a ast::Program,
    signatures: HashMap<String, FunctionSignature>,
    main: Option<FunctionId>,
    types: Vec<TypeDef>,
    type_ids: HashMap<String, TypeId>,
}

impl<'a> ProgramLowerer<'a> {
    fn new(sources: &'a [SourceFile], ast: &'a ast::Program) -> Self {
        Self {
            sources,
            ast,
            signatures: HashMap::new(),
            main: None,
            types: Vec::new(),
            type_ids: HashMap::new(),
        }
    }

    fn lower(mut self) -> Result<ir::Program, Diagnostic> {
        self.collect_types()?;
        self.collect_signatures()?;
        let mut functions = Vec::with_capacity(self.ast.functions.len());
        for (index, function) in self.ast.functions.iter().enumerate() {
            let signature = self.signatures[&function.name].clone();
            debug_assert_eq!(signature.id, FunctionId(index));
            functions.push(
                FunctionLowerer::new(
                    &self.sources[function.source_id],
                    function,
                    signature,
                    &self.signatures,
                    &self.types,
                    &self.type_ids,
                )
                .lower()?,
            );
        }
        Ok(ir::Program {
            functions,
            main: self.main.expect("signature collection checks main"),
            types: self.types,
        })
    }

    /// 收集 struct/enum 声明，构建类型表与全限定名到 TypeId 的映射。
    fn collect_types(&mut self) -> Result<(), Diagnostic> {
        for structure in &self.ast.structs {
            let id = TypeId(self.types.len());
            self.type_ids.insert(structure.name.clone(), id);
            self.types.push(TypeDef::Struct { fields: Vec::new() });
        }
        for enumeration in &self.ast.enums {
            let id = TypeId(self.types.len());
            self.type_ids.insert(enumeration.name.clone(), id);
            self.types.push(TypeDef::Enum {
                variants: Vec::new(),
            });
        }
        // 第二轮：填充字段与 variant 类型（此时所有类型名都已注册，可解析相互引用）。
        for structure in &self.ast.structs {
            let id = self.type_ids[&structure.name];
            // 检查字段重复。
            let mut seen = std::collections::HashSet::new();
            for field in &structure.fields {
                if !seen.insert(field.name.clone()) {
                    let source = &self.sources[structure.source_id];
                    return Err(Diagnostic::at(
                        source,
                        field.name_span,
                        format!("field `{}` is already defined", field.name),
                    ));
                }
            }
            let fields = structure
                .fields
                .iter()
                .map(|field| {
                    let source = &self.sources[structure.source_id];
                    let ty = resolve_type(source, &field.ty, &self.types, &self.type_ids)?;
                    Ok(StructField {
                        name: field.name.clone(),
                        ty,
                    })
                })
                .collect::<Result<Vec<_>, Diagnostic>>()?;
            if let TypeDef::Struct { fields: slot, .. } = &mut self.types[id.0] {
                *slot = fields;
            }
        }
        for enumeration in &self.ast.enums {
            let id = self.type_ids[&enumeration.name];
            // 检查 variant 重复。
            let mut seen = std::collections::HashSet::new();
            for variant in &enumeration.variants {
                if !seen.insert(variant.name.clone()) {
                    let source = &self.sources[enumeration.source_id];
                    return Err(Diagnostic::at(
                        source,
                        variant.name_span,
                        format!("variant `{}` is already defined", variant.name),
                    ));
                }
            }
            let variants = enumeration
                .variants
                .iter()
                .map(|variant| {
                    let fields = variant
                        .fields
                        .iter()
                        .map(|field| {
                            let source = &self.sources[enumeration.source_id];
                            resolve_type(source, field, &self.types, &self.type_ids)
                        })
                        .collect::<Result<Vec<_>, Diagnostic>>()?;
                    Ok(EnumVariant {
                        name: variant.name.clone(),
                        fields,
                    })
                })
                .collect::<Result<Vec<_>, Diagnostic>>()?;
            if let TypeDef::Enum { variants: slot, .. } = &mut self.types[id.0] {
                *slot = variants;
            }
        }
        Ok(())
    }

    fn collect_signatures(&mut self) -> Result<(), Diagnostic> {
        for (index, function) in self.ast.functions.iter().enumerate() {
            let source = &self.sources[function.source_id];
            if matches!(function.name.as_str(), "print" | "println" | "length") {
                return Err(Diagnostic::at(
                    source,
                    function.name_span,
                    format!("`{}` is a reserved built-in function", function.name),
                ));
            }
            if self.signatures.contains_key(&function.name) {
                return Err(Diagnostic::at(
                    source,
                    function.name_span,
                    format!("function `{}` is already defined", function.name),
                ));
            }

            let mut parameters = Vec::with_capacity(function.parameters.len());
            for parameter in &function.parameters {
                let ty = resolve_type(source, &parameter.ty, &self.types, &self.type_ids)?;
                parameters.push(ty);
            }
            let is_main = function.name == "main";
            let return_type = match &function.return_type {
                Some(ty) => resolve_type(source, ty, &self.types, &self.type_ids)?,
                None if is_main => Type::I32,
                None => Type::Unit,
            };
            let id = FunctionId(index);
            if is_main {
                if self.main.replace(id).is_some() {
                    return Err(Diagnostic::at(
                        source,
                        function.name_span,
                        "program contains more than one `main` function",
                    ));
                }
                if !parameters.is_empty() {
                    return Err(Diagnostic::at(
                        source,
                        function.name_span,
                        "`main` cannot have parameters yet",
                    ));
                }
                if return_type != Type::I32 {
                    return Err(Diagnostic::at(
                        source,
                        function.name_span,
                        "`main` must return `i32` or omit its return type",
                    ));
                }
            }
            self.signatures.insert(
                function.name.clone(),
                FunctionSignature {
                    id,
                    parameters,
                    return_type,
                },
            );
        }

        if self.main.is_none() {
            return Err(Diagnostic::plain("program does not define `main`"));
        }
        Ok(())
    }
}

#[derive(Clone)]
struct Binding {
    local: LocalId,
    ty: Type,
    mutable: bool,
}

#[derive(Clone, Copy)]
struct LoopTargets {
    break_block: BlockId,
    continue_block: BlockId,
}

struct WorkingBlock {
    instructions: Vec<Instruction>,
    terminator: Option<Terminator>,
    reachable: bool,
}

struct FunctionLowerer<'a> {
    source: &'a SourceFile,
    function: &'a ast::Function,
    signature: FunctionSignature,
    signatures: &'a HashMap<String, FunctionSignature>,
    types: &'a [TypeDef],
    type_ids: &'a HashMap<String, TypeId>,
    parameters: Vec<LocalId>,
    locals: Vec<Type>,
    scopes: Vec<HashMap<String, Binding>>,
    /// 词法块作用域的 `defer` 动作栈（每层一个逆序待执行列表，见 §12.8）。
    deferred: Vec<Vec<Expr>>,
    /// 词法块作用域的 `try` 资源名栈（用于逃逸检查，见 §4.2/§12.2）。
    try_resources: Vec<HashSet<String>>,
    blocks: Vec<WorkingBlock>,
    current: BlockId,
    loops: Vec<LoopTargets>,
}

impl<'a> FunctionLowerer<'a> {
    fn new(
        source: &'a SourceFile,
        function: &'a ast::Function,
        signature: FunctionSignature,
        signatures: &'a HashMap<String, FunctionSignature>,
        types: &'a [TypeDef],
        type_ids: &'a HashMap<String, TypeId>,
    ) -> Self {
        Self {
            source,
            function,
            signature,
            signatures,
            types,
            type_ids,
            parameters: Vec::new(),
            locals: Vec::new(),
            scopes: vec![HashMap::new()],
            deferred: vec![Vec::new()],
            try_resources: vec![HashSet::new()],
            blocks: vec![WorkingBlock {
                instructions: Vec::new(),
                terminator: None,
                reachable: true,
            }],
            current: BlockId(0),
            loops: Vec::new(),
        }
    }

    fn lower(mut self) -> Result<ir::Function, Diagnostic> {
        for (parameter, ty) in self
            .function
            .parameters
            .iter()
            .zip(self.signature.parameters.iter().copied())
        {
            if self.scopes[0].contains_key(&parameter.name) {
                return Err(Diagnostic::at(
                    self.source,
                    parameter.name_span,
                    format!("parameter `{}` is already declared", parameter.name),
                ));
            }
            let local = LocalId(self.locals.len());
            self.locals.push(ty);
            self.parameters.push(local);
            self.scopes[0].insert(
                parameter.name.clone(),
                Binding {
                    local,
                    ty,
                    mutable: false,
                },
            );
        }

        self.lower_statements(&self.function.body)?;
        if !self.is_terminated(self.current) {
            let reachable = self.blocks[self.current.0].reachable;
            match self.signature.return_type {
                Type::Unit => self.terminate(Terminator::Return(None)),
                Type::I32 if self.function.name == "main" => {
                    self.terminate(Terminator::Return(Some(Expr::i32(0))))
                }
                _ if reachable => {
                    return Err(Diagnostic::at(
                        self.source,
                        self.function.span,
                        format!(
                            "function `{}` may exit without returning `{}`",
                            self.function.name, self.signature.return_type
                        ),
                    ));
                }
                ty => self.terminate(Terminator::Return(Some(default_expr(ty)))),
            }
        }

        let blocks = self
            .blocks
            .into_iter()
            .map(|block| BasicBlock {
                instructions: block.instructions,
                terminator: block
                    .terminator
                    .expect("all IR blocks must have a terminator"),
            })
            .collect();
        Ok(ir::Function {
            id: self.signature.id,
            name: self.function.name.clone(),
            parameters: self.parameters,
            return_type: self.signature.return_type,
            locals: self.locals,
            blocks,
            entry: BlockId(0),
        })
    }

    fn lower_statements(&mut self, statements: &[Statement]) -> Result<(), Diagnostic> {
        for statement in statements {
            if self.is_terminated(self.current) || !self.blocks[self.current.0].reachable {
                return Err(Diagnostic::at(
                    self.source,
                    statement.span,
                    "unreachable statement",
                ));
            }
            self.lower_statement(statement)?;
        }
        Ok(())
    }

    fn lower_statement(&mut self, statement: &Statement) -> Result<(), Diagnostic> {
        match &statement.kind {
            StatementKind::Variable {
                mutable,
                name,
                name_span,
                type_name,
                initializer,
            } => {
                self.check_try_escape(initializer, *name_span)?;
                let value = self.lower_expr(initializer)?;
                if value.ty == Type::Unit {
                    return Err(Diagnostic::at(
                        self.source,
                        initializer.span,
                        "cannot store a value of type `Unit`",
                    ));
                }
                let ty = match type_name {
                    Some(ty) => {
                        let declared = resolve_type(self.source, ty, self.types, self.type_ids)?;
                        self.require_type(value.ty, declared, initializer.span)?;
                        declared
                    }
                    None => value.ty,
                };
                if self.scopes.last().unwrap().contains_key(name) {
                    return Err(Diagnostic::at(
                        self.source,
                        *name_span,
                        format!("`{name}` is already declared in this scope"),
                    ));
                }
                let local = LocalId(self.locals.len());
                self.locals.push(ty);
                self.scopes.last_mut().unwrap().insert(
                    name.clone(),
                    Binding {
                        local,
                        ty,
                        mutable: *mutable,
                    },
                );
                self.emit(Instruction::SetLocal { local, value });
            }
            StatementKind::Assignment {
                name,
                name_span,
                operator,
                value,
            } => self.lower_assignment(name, *name_span, *operator, value)?,
            StatementKind::IndexAssignment {
                name,
                name_span,
                index,
                operator,
                value,
            } => self.lower_index_assignment(name, *name_span, index, *operator, value)?,
            StatementKind::Expression(expression) => self.lower_expression_statement(expression)?,
            StatementKind::If {
                condition,
                then_block,
                else_block,
            } => self.lower_if(condition, then_block, else_block.as_deref())?,
            StatementKind::Loop(body) => self.lower_loop(body)?,
            StatementKind::While { condition, body } => self.lower_while(condition, body)?,
            StatementKind::For {
                name,
                name_span,
                iterable,
                body,
            } => self.lower_for(name, *name_span, iterable, body)?,
            StatementKind::Break => {
                let targets = self.loops.last().copied().ok_or_else(|| {
                    Diagnostic::at(self.source, statement.span, "`break` used outside a loop")
                })?;
                self.emit_pending_defers();
                self.terminate(Terminator::Jump(targets.break_block));
            }
            StatementKind::Continue => {
                let targets = self.loops.last().copied().ok_or_else(|| {
                    Diagnostic::at(
                        self.source,
                        statement.span,
                        "`continue` used outside a loop",
                    )
                })?;
                self.emit_pending_defers();
                self.terminate(Terminator::Jump(targets.continue_block));
            }
            StatementKind::Return(value) => self.lower_return(value.as_ref(), statement.span)?,
            StatementKind::Defer { value } => self.lower_defer(value, statement.span)?,
            StatementKind::Try { resources, body } => self.lower_try(resources, body)?,
            StatementKind::DerefAssignment {
                target,
                operator,
                value,
            } => self.lower_deref_assignment(target, *operator, value)?,
            StatementKind::PtrFieldAssignment {
                base,
                field,
                field_span,
                operator,
                value,
            } => self.lower_ptr_field_assignment(base, field, *field_span, *operator, value)?,
        }
        Ok(())
    }

    fn lower_assignment(
        &mut self,
        name: &str,
        name_span: Span,
        operator: AssignmentOperator,
        value: &ast::Expr,
    ) -> Result<(), Diagnostic> {
        self.check_try_escape(value, name_span)?;
        let binding = self.lookup(name, name_span)?.clone();
        if !binding.mutable {
            return Err(Diagnostic::at(
                self.source,
                name_span,
                format!("cannot assign to immutable variable `{name}`"),
            ));
        }
        let right = self.lower_expr(value)?;
        let assigned = match operator {
            AssignmentOperator::Assign => {
                self.require_type(right.ty, binding.ty, value.span)?;
                right
            }
            operator => {
                if !(binding.ty.is_integer() || binding.ty.is_float()) {
                    return Err(Diagnostic::at(
                        self.source,
                        name_span,
                        "compound assignment requires a numeric value",
                    ));
                }
                self.require_type(right.ty, binding.ty, value.span)?;
                Expr {
                    kind: ir::ExprKind::Binary {
                        operator: assignment_binary(operator),
                        left: Box::new(Expr {
                            kind: ir::ExprKind::Local(binding.local),
                            ty: binding.ty,
                        }),
                        right: Box::new(right),
                    },
                    ty: binding.ty,
                }
            }
        };
        self.emit(Instruction::SetLocal {
            local: binding.local,
            value: assigned,
        });
        Ok(())
    }

    fn lower_index_assignment(
        &mut self,
        name: &str,
        name_span: Span,
        index: &ast::Expr,
        operator: AssignmentOperator,
        value: &ast::Expr,
    ) -> Result<(), Diagnostic> {
        let binding = self.lookup(name, name_span)?.clone();
        if !binding.mutable {
            return Err(Diagnostic::at(
                self.source,
                name_span,
                format!("cannot modify immutable array `{name}`"),
            ));
        }
        if let Type::Array { length, .. } = binding.ty {
            check_constant_index(self.source, index, length)?;
        }
        let index = self.lower_expr(index)?;
        self.require_type(index.ty, Type::I32, name_span)?;
        let index_local = self.store_temporary(index);
        let right = self.lower_expr(value)?;
        match binding.ty {
            Type::Array { element, .. } => {
                let element_type = element.as_type();
                let assigned = match operator {
                    AssignmentOperator::Assign => {
                        self.require_type(right.ty, element_type, value.span)?;
                        right
                    }
                    operator => {
                        if !(element_type.is_integer() || element_type.is_float()) {
                            return Err(Diagnostic::at(
                                self.source,
                                name_span,
                                "compound assignment requires a numeric element",
                            ));
                        }
                        self.require_type(right.ty, element_type, value.span)?;
                        Expr {
                            kind: ir::ExprKind::Binary {
                                operator: assignment_binary(operator),
                                left: Box::new(Expr {
                                    kind: ir::ExprKind::Index {
                                        array: Box::new(Expr {
                                            kind: ir::ExprKind::Local(binding.local),
                                            ty: binding.ty,
                                        }),
                                        index: Box::new(Expr {
                                            kind: ir::ExprKind::Local(index_local),
                                            ty: Type::I32,
                                        }),
                                    },
                                    ty: element_type,
                                }),
                                right: Box::new(right),
                            },
                            ty: element_type,
                        }
                    }
                };
                self.emit(Instruction::SetIndex {
                    local: binding.local,
                    index: Expr {
                        kind: ir::ExprKind::Local(index_local),
                        ty: Type::I32,
                    },
                    value: assigned,
                });
            }
            Type::Slice(element) => {
                let element_type = element.as_type();
                let assigned = match operator {
                    AssignmentOperator::Assign => {
                        self.require_type(right.ty, element_type, value.span)?;
                        right
                    }
                    operator => {
                        if !(element_type.is_integer() || element_type.is_float()) {
                            return Err(Diagnostic::at(
                                self.source,
                                name_span,
                                "compound assignment requires a numeric element",
                            ));
                        }
                        self.require_type(right.ty, element_type, value.span)?;
                        Expr {
                            kind: ir::ExprKind::Binary {
                                operator: assignment_binary(operator),
                                left: Box::new(Expr {
                                    kind: ir::ExprKind::SliceIndex {
                                        slice: Box::new(Expr {
                                            kind: ir::ExprKind::Local(binding.local),
                                            ty: binding.ty,
                                        }),
                                        index: Box::new(Expr {
                                            kind: ir::ExprKind::Local(index_local),
                                            ty: Type::I32,
                                        }),
                                    },
                                    ty: element_type,
                                }),
                                right: Box::new(right),
                            },
                            ty: element_type,
                        }
                    }
                };
                self.emit(Instruction::SetSliceIndex {
                    slice: Expr {
                        kind: ir::ExprKind::Local(binding.local),
                        ty: binding.ty,
                    },
                    index: Expr {
                        kind: ir::ExprKind::Local(index_local),
                        ty: Type::I32,
                    },
                    value: assigned,
                });
            }
            other => {
                return Err(Diagnostic::at(
                    self.source,
                    name_span,
                    format!("cannot index value of type `{other}`"),
                ));
            }
        }
        Ok(())
    }

    fn lower_expression_statement(&mut self, expression: &ast::Expr) -> Result<(), Diagnostic> {
        let ExprKind::Call {
            callee, arguments, ..
        } = &expression.kind
        else {
            return Err(Diagnostic::at(
                self.source,
                expression.span,
                "only function calls can be used as expression statements",
            ));
        };
        if matches!(callee.as_str(), "print" | "println") {
            let parts = self.lower_print(arguments, callee == "println", expression.span)?;
            self.emit(Instruction::Print(parts));
        } else {
            let value = self.lower_expr(expression)?;
            self.emit(Instruction::Evaluate(value));
        }
        Ok(())
    }

    fn lower_print(
        &mut self,
        arguments: &[ast::Expr],
        newline: bool,
        span: Span,
    ) -> Result<Vec<PrintPart>, Diagnostic> {
        if arguments.is_empty() {
            return if newline {
                Ok(vec![PrintPart::Text("\n".to_string())])
            } else {
                Ok(Vec::new())
            };
        }
        let ExprKind::String(format) = &arguments[0].kind else {
            return Err(Diagnostic::at(
                self.source,
                arguments[0].span,
                "the first print argument must be a string literal",
            ));
        };
        let format_parts = parse_format(self.source, format, span)?;
        let placeholder_count = format_parts
            .iter()
            .filter(|part| matches!(part, FormatPart::Placeholder))
            .count();
        if placeholder_count != arguments.len() - 1 {
            return Err(Diagnostic::at(
                self.source,
                span,
                format!(
                    "format string has {placeholder_count} placeholders but {} values were provided",
                    arguments.len() - 1
                ),
            ));
        }

        let mut values = arguments[1..].iter();
        let mut parts = Vec::new();
        for part in format_parts {
            match part {
                FormatPart::Text(text) if !text.is_empty() => parts.push(PrintPart::Text(text)),
                FormatPart::Text(_) => {}
                FormatPart::Placeholder => {
                    let value = self.lower_expr(values.next().unwrap())?;
                    if matches!(value.ty, Type::Unit | Type::Array { .. }) {
                        return Err(Diagnostic::at(
                            self.source,
                            span,
                            format!("cannot format `{}`", value.ty),
                        ));
                    }
                    parts.push(PrintPart::Value(value));
                }
            }
        }
        if newline {
            parts.push(PrintPart::Text("\n".to_string()));
        }
        Ok(parts)
    }

    fn lower_return(&mut self, value: Option<&ast::Expr>, span: Span) -> Result<(), Diagnostic> {
        let value = match (self.signature.return_type, value) {
            (Type::I32, None) if self.function.name == "main" => Some(Expr::i32(0)),
            (Type::Unit, None) => None,
            (Type::Unit, Some(_)) => {
                return Err(Diagnostic::at(
                    self.source,
                    span,
                    "a function without a return type cannot return a value",
                ));
            }
            (expected, Some(value)) => {
                self.check_try_escape(value, span)?;
                let value = self.lower_expr(value)?;
                self.require_type(value.ty, expected, span)?;
                Some(value)
            }
            (expected, None) => {
                return Err(Diagnostic::at(
                    self.source,
                    span,
                    format!("expected a return value of type `{expected}`"),
                ));
            }
        };
        self.emit_pending_defers();
        self.terminate(Terminator::Return(value));
        Ok(())
    }

    fn lower_defer(&mut self, value: &ast::Expr, span: Span) -> Result<(), Diagnostic> {
        let action = self.lower_expr(value)?;
        if action.ty != Type::Unit {
            return Err(Diagnostic::at(
                self.source,
                span,
                "`defer` expects a statement-like expression (e.g. a function call)",
            ));
        }
        self.deferred
            .last_mut()
            .expect("defer scope is balanced")
            .push(action);
        Ok(())
    }

    fn lower_try(
        &mut self,
        resources: &[TryResource],
        body: &[Statement],
    ) -> Result<(), Diagnostic> {
        // 先 lower 各资源的初始化表达式（在外层作用域，避免后续逃逸检查误判）。
        let mut resource_inits = Vec::with_capacity(resources.len());
        for resource in resources {
            let value = self.lower_expr(&resource.initializer)?;
            let Type::Slice(crate::ir::ScalarType::U8) = value.ty else {
                return Err(Diagnostic::at(
                    self.source,
                    resource.initializer.span,
                    "`try` resource must be a `[]u8` slice (e.g. from `allocate`)",
                ));
            };
            resource_inits.push((resource.name.clone(), resource.name_span, value));
        }
        // 建立 try 作用域：绑定资源变量 + 标记资源名 + 逆序 defer free。
        self.scopes.push(HashMap::new());
        self.deferred.push(Vec::new());
        self.try_resources.push(HashSet::new());
        for (name, _name_span, _value) in resource_inits.iter() {
            self.try_resources
                .last_mut()
                .expect("try scope is balanced")
                .insert(name.clone());
        }
        // 按声明顺序 allocate + 注册 defer free；出块时 deferred 栈 LIFO 逆序释放
        // （§12.8：声明顺序的逆序，即后声明的资源先释放）。
        for (name, _name_span, value) in resource_inits.into_iter() {
            let local = LocalId(self.locals.len());
            self.locals.push(value.ty);
            self.emit(Instruction::SetLocal { local, value });
            self.scopes.last_mut().unwrap().insert(
                name.clone(),
                Binding {
                    local,
                    ty: Type::Slice(crate::ir::ScalarType::U8),
                    mutable: true,
                },
            );
            let free = Expr {
                kind: ir::ExprKind::Free(Box::new(Expr {
                    kind: ir::ExprKind::Local(local),
                    ty: Type::Slice(crate::ir::ScalarType::U8),
                })),
                ty: Type::Unit,
            };
            self.deferred
                .last_mut()
                .expect("try scope is balanced")
                .push(free);
        }
        let result = self.lower_statements(body);
        let layer = self.deferred.pop().expect("try scope is balanced");
        for action in layer.into_iter().rev() {
            self.emit(Instruction::Evaluate(action));
        }
        self.try_resources.pop();
        self.scopes.pop();
        result
    }

    fn lower_deref_assignment(
        &mut self,
        target: &ast::Expr,
        operator: AssignmentOperator,
        value: &ast::Expr,
    ) -> Result<(), Diagnostic> {
        let target_span = target.span;
        let target = self.lower_expr(target)?;
        let Type::Ptr(pointer) = target.ty else {
            return Err(Diagnostic::at(
                self.source,
                target_span,
                format!("cannot dereference value of type `{}`", target.ty),
            ));
        };
        let pointee = pointer_type(pointer, self.types);
        let pointer_local = self.store_temporary(target);
        let pointer_expr = local_expr(pointer_local, Type::Ptr(pointer));
        let right = self.lower_expr(value)?;
        let assigned = match operator {
            AssignmentOperator::Assign => {
                self.require_type(right.ty, pointee, value.span)?;
                right
            }
            operator => {
                if !(pointee.is_integer() || pointee.is_float()) {
                    return Err(Diagnostic::at(
                        self.source,
                        value.span,
                        "compound assignment requires a numeric pointee",
                    ));
                }
                self.require_type(right.ty, pointee, value.span)?;
                Expr {
                    kind: ir::ExprKind::Binary {
                        operator: assignment_binary(operator),
                        left: Box::new(Expr {
                            kind: ir::ExprKind::Deref(Box::new(pointer_expr.clone())),
                            ty: pointee,
                        }),
                        right: Box::new(right),
                    },
                    ty: pointee,
                }
            }
        };
        self.emit(Instruction::SetDeref {
            pointer: pointer_expr,
            value: assigned,
        });
        Ok(())
    }

    fn lower_ptr_field_assignment(
        &mut self,
        base: &ast::Expr,
        field: &str,
        field_span: Span,
        operator: AssignmentOperator,
        value: &ast::Expr,
    ) -> Result<(), Diagnostic> {
        let base_value = self.lower_expr(base)?;
        let Type::Ptr(crate::ir::Pointer::Struct(id)) = base_value.ty else {
            return Err(Diagnostic::at(
                self.source,
                field_span,
                format!(
                    "`->` requires a pointer to a struct, found `{}`",
                    base_value.ty
                ),
            ));
        };
        let TypeDef::Struct { fields, .. } = &self.types[id.0] else {
            unreachable!("pointer field access resolves a struct type");
        };
        let index = fields
            .iter()
            .position(|field_def| field_def.name == field)
            .ok_or_else(|| {
                Diagnostic::at(
                    self.source,
                    field_span,
                    format!("struct has no field named `{field}`"),
                )
            })?;
        let field_ty = fields[index].ty;
        let pointer_local = self.store_temporary(base_value);
        let pointer_expr = local_expr(pointer_local, Type::Ptr(crate::ir::Pointer::Struct(id)));
        let right = self.lower_expr(value)?;
        let assigned = match operator {
            AssignmentOperator::Assign => {
                self.require_type(right.ty, field_ty, value.span)?;
                right
            }
            operator => {
                if !(field_ty.is_integer() || field_ty.is_float()) {
                    return Err(Diagnostic::at(
                        self.source,
                        value.span,
                        "compound assignment requires a numeric field",
                    ));
                }
                self.require_type(right.ty, field_ty, value.span)?;
                Expr {
                    kind: ir::ExprKind::Binary {
                        operator: assignment_binary(operator),
                        left: Box::new(Expr {
                            kind: ir::ExprKind::PtrField {
                                base: Box::new(pointer_expr.clone()),
                                field: index,
                            },
                            ty: field_ty,
                        }),
                        right: Box::new(right),
                    },
                    ty: field_ty,
                }
            }
        };
        self.emit(Instruction::SetPtrField {
            pointer: pointer_expr,
            field: index,
            value: assigned,
        });
        Ok(())
    }

    fn lower_if(
        &mut self,
        condition: &ast::Expr,
        then_statements: &[Statement],
        else_statements: Option<&[Statement]>,
    ) -> Result<(), Diagnostic> {
        let condition_value = self.lower_expr(condition)?;
        self.require_type(condition_value.ty, Type::Bool, condition.span)?;
        let then_block = self.new_block();
        let else_block = self.new_block();
        let merge_block = self.new_block();
        self.terminate(Terminator::Branch {
            condition: condition_value,
            then_block,
            else_block,
        });

        self.current = then_block;
        self.with_scope(|lowerer| lowerer.lower_statements(then_statements))?;
        if !self.is_terminated(self.current) {
            self.terminate(Terminator::Jump(merge_block));
        }

        self.current = else_block;
        if let Some(statements) = else_statements {
            self.with_scope(|lowerer| lowerer.lower_statements(statements))?;
        }
        if !self.is_terminated(self.current) {
            self.terminate(Terminator::Jump(merge_block));
        }
        self.current = merge_block;
        Ok(())
    }

    fn lower_loop(&mut self, statements: &[Statement]) -> Result<(), Diagnostic> {
        let body = self.new_block();
        let exit = self.new_block();
        self.terminate(Terminator::Jump(body));
        self.current = body;
        self.loops.push(LoopTargets {
            break_block: exit,
            continue_block: body,
        });
        let result = self.with_scope(|lowerer| lowerer.lower_statements(statements));
        self.loops.pop();
        result?;
        if !self.is_terminated(self.current) {
            self.terminate(Terminator::Jump(body));
        }
        self.current = exit;
        Ok(())
    }

    fn lower_while(
        &mut self,
        condition: &ast::Expr,
        statements: &[Statement],
    ) -> Result<(), Diagnostic> {
        let condition_block = self.new_block();
        let body = self.new_block();
        let exit = self.new_block();
        self.terminate(Terminator::Jump(condition_block));

        self.current = condition_block;
        let value = self.lower_expr(condition)?;
        self.require_type(value.ty, Type::Bool, condition.span)?;
        self.terminate(Terminator::Branch {
            condition: value,
            then_block: body,
            else_block: exit,
        });

        self.current = body;
        self.loops.push(LoopTargets {
            break_block: exit,
            continue_block: condition_block,
        });
        let result = self.with_scope(|lowerer| lowerer.lower_statements(statements));
        self.loops.pop();
        result?;
        if !self.is_terminated(self.current) {
            self.terminate(Terminator::Jump(condition_block));
        }
        self.current = exit;
        Ok(())
    }

    fn lower_for(
        &mut self,
        name: &str,
        name_span: Span,
        iterable: &ForIterable,
        statements: &[Statement],
    ) -> Result<(), Diagnostic> {
        match iterable {
            ForIterable::Range {
                start,
                end,
                inclusive,
            } => self.lower_range_for(name, name_span, start, end, *inclusive, statements),
            ForIterable::Array(array) => self.lower_array_for(name, name_span, array, statements),
        }
    }

    fn lower_range_for(
        &mut self,
        name: &str,
        _name_span: Span,
        start: &ast::Expr,
        end: &ast::Expr,
        inclusive: bool,
        statements: &[Statement],
    ) -> Result<(), Diagnostic> {
        let start_value = self.lower_expr(start)?;
        let end_value = self.lower_expr(end)?;
        self.require_type(start_value.ty, Type::I32, start.span)?;
        self.require_type(end_value.ty, Type::I32, end.span)?;
        let counter = self.store_temporary(start_value);
        let limit = self.store_temporary(end_value);
        let condition_block = self.new_block();
        let body = self.new_block();
        let increment = self.new_block();
        let add = inclusive.then(|| self.new_block());
        let exit = self.new_block();
        self.terminate(Terminator::Jump(condition_block));

        self.current = condition_block;
        self.terminate(Terminator::Branch {
            condition: Expr {
                kind: ir::ExprKind::Binary {
                    operator: if inclusive {
                        BinaryOperator::LessEqual
                    } else {
                        BinaryOperator::Less
                    },
                    left: Box::new(local_expr(counter, Type::I32)),
                    right: Box::new(local_expr(limit, Type::I32)),
                },
                ty: Type::Bool,
            },
            then_block: body,
            else_block: exit,
        });

        self.current = body;
        self.loops.push(LoopTargets {
            break_block: exit,
            continue_block: increment,
        });
        let result = self.with_scope(|lowerer| {
            lowerer.scopes.last_mut().unwrap().insert(
                name.to_string(),
                Binding {
                    local: counter,
                    ty: Type::I32,
                    mutable: false,
                },
            );
            lowerer.lower_statements(statements)
        });
        self.loops.pop();
        result?;
        if !self.is_terminated(self.current) {
            self.terminate(Terminator::Jump(increment));
        }

        self.current = increment;
        if let Some(add) = add {
            self.terminate(Terminator::Branch {
                condition: Expr {
                    kind: ir::ExprKind::Binary {
                        operator: BinaryOperator::Equal,
                        left: Box::new(local_expr(counter, Type::I32)),
                        right: Box::new(local_expr(limit, Type::I32)),
                    },
                    ty: Type::Bool,
                },
                then_block: exit,
                else_block: add,
            });
            self.current = add;
        }
        self.emit(Instruction::SetLocal {
            local: counter,
            value: Expr {
                kind: ir::ExprKind::Binary {
                    operator: BinaryOperator::Add,
                    left: Box::new(local_expr(counter, Type::I32)),
                    right: Box::new(Expr::i32(1)),
                },
                ty: Type::I32,
            },
        });
        self.terminate(Terminator::Jump(condition_block));
        self.current = exit;
        Ok(())
    }

    fn lower_array_for(
        &mut self,
        name: &str,
        _name_span: Span,
        array: &ast::Expr,
        statements: &[Statement],
    ) -> Result<(), Diagnostic> {
        let array_value = self.lower_expr(array)?;
        let Type::Array { element, length } = array_value.ty else {
            return Err(Diagnostic::at(
                self.source,
                array.span,
                "`for` expects an array or integer range",
            ));
        };
        let array_local = self.store_temporary(array_value);
        let index_local = self.store_temporary(Expr::i32(0));
        let item_local = self.new_local(element.as_type());
        let condition_block = self.new_block();
        let body = self.new_block();
        let increment = self.new_block();
        let exit = self.new_block();
        self.terminate(Terminator::Jump(condition_block));

        self.current = condition_block;
        self.terminate(Terminator::Branch {
            condition: Expr {
                kind: ir::ExprKind::Binary {
                    operator: BinaryOperator::Less,
                    left: Box::new(local_expr(index_local, Type::I32)),
                    right: Box::new(Expr::i32(length as i32)),
                },
                ty: Type::Bool,
            },
            then_block: body,
            else_block: exit,
        });

        self.current = body;
        self.emit(Instruction::SetLocal {
            local: item_local,
            value: Expr {
                kind: ir::ExprKind::Index {
                    array: Box::new(local_expr(array_local, Type::Array { element, length })),
                    index: Box::new(local_expr(index_local, Type::I32)),
                },
                ty: element.as_type(),
            },
        });
        self.loops.push(LoopTargets {
            break_block: exit,
            continue_block: increment,
        });
        let result = self.with_scope(|lowerer| {
            lowerer.scopes.last_mut().unwrap().insert(
                name.to_string(),
                Binding {
                    local: item_local,
                    ty: element.as_type(),
                    mutable: false,
                },
            );
            lowerer.lower_statements(statements)
        });
        self.loops.pop();
        result?;
        if !self.is_terminated(self.current) {
            self.terminate(Terminator::Jump(increment));
        }

        self.current = increment;
        self.emit(Instruction::SetLocal {
            local: index_local,
            value: Expr {
                kind: ir::ExprKind::Binary {
                    operator: BinaryOperator::Add,
                    left: Box::new(local_expr(index_local, Type::I32)),
                    right: Box::new(Expr::i32(1)),
                },
                ty: Type::I32,
            },
        });
        self.terminate(Terminator::Jump(condition_block));
        self.current = exit;
        Ok(())
    }

    fn lower_expr(&mut self, expression: &ast::Expr) -> Result<Expr, Diagnostic> {
        match &expression.kind {
            ExprKind::Number(value) => lower_number(self.source, value, false, expression.span),
            ExprKind::Character(value) => Ok(Expr {
                kind: ir::ExprKind::Char(*value),
                ty: Type::Char,
            }),
            ExprKind::Boolean(value) => Ok(Expr {
                kind: ir::ExprKind::Bool(*value),
                ty: Type::Bool,
            }),
            ExprKind::String(value) => Ok(Expr {
                kind: ir::ExprKind::String(value.clone()),
                ty: Type::String,
            }),
            ExprKind::Array(values) => {
                if values.is_empty() {
                    return Err(Diagnostic::at(
                        self.source,
                        expression.span,
                        "cannot infer the type of an empty array",
                    ));
                }
                let mut lowered = Vec::with_capacity(values.len());
                for value in values {
                    lowered.push(self.lower_expr(value)?);
                }
                let element = lowered[0].ty.as_scalar().ok_or_else(|| {
                    Diagnostic::at(
                        self.source,
                        values[0].span,
                        "array elements must be scalar values",
                    )
                })?;
                for (value, source_value) in lowered.iter().zip(values) {
                    self.require_type(value.ty, element.as_type(), source_value.span)?;
                }
                Ok(Expr {
                    kind: ir::ExprKind::Array(lowered),
                    ty: Type::Array {
                        element,
                        length: values.len(),
                    },
                })
            }
            ExprKind::RepeatArray { value, length } => {
                if *length == 0 {
                    return Err(Diagnostic::at(
                        self.source,
                        expression.span,
                        "array length must be greater than zero",
                    ));
                }
                let value = self.lower_expr(value)?;
                let element = value.ty.as_scalar().ok_or_else(|| {
                    Diagnostic::at(
                        self.source,
                        expression.span,
                        "array elements must be scalar values",
                    )
                })?;
                Ok(Expr {
                    kind: ir::ExprKind::RepeatArray {
                        value: Box::new(value),
                        length: *length,
                    },
                    ty: Type::Array {
                        element,
                        length: *length,
                    },
                })
            }
            ExprKind::Index { array, index } => {
                let array_value = self.lower_expr(array)?;
                let index_value = self.lower_expr(index)?;
                self.require_type(index_value.ty, Type::I32, index.span)?;
                match array_value.ty {
                    Type::Array { element, length } => {
                        check_constant_index(self.source, index, length)?;
                        Ok(Expr {
                            kind: ir::ExprKind::Index {
                                array: Box::new(array_value),
                                index: Box::new(index_value),
                            },
                            ty: element.as_type(),
                        })
                    }
                    Type::Slice(element) => Ok(Expr {
                        kind: ir::ExprKind::SliceIndex {
                            slice: Box::new(array_value),
                            index: Box::new(index_value),
                        },
                        ty: element.as_type(),
                    }),
                    other => Err(Diagnostic::at(
                        self.source,
                        array.span,
                        format!("cannot index value of type `{other}`"),
                    )),
                }
            }
            ExprKind::Name(name) => {
                let binding = self.lookup(name, expression.span)?;
                Ok(Expr {
                    kind: ir::ExprKind::Local(binding.local),
                    ty: binding.ty,
                })
            }
            ExprKind::Call {
                callee,
                callee_span,
                arguments,
            } => self.lower_call(callee, *callee_span, arguments),
            ExprKind::Cast { value, ty } => {
                let value = self.lower_expr(value)?;
                let to = resolve_type(self.source, ty, self.types, self.type_ids)?;
                if !is_castable(value.ty) || !is_castable(to) {
                    return Err(Diagnostic::at(
                        self.source,
                        expression.span,
                        format!("cannot cast `{}` to `{to}`", value.ty),
                    ));
                }
                Ok(Expr {
                    kind: ir::ExprKind::Cast {
                        value: Box::new(value),
                        to,
                    },
                    ty: to,
                })
            }
            ExprKind::Unary { operator, operand } => {
                if matches!(operator, UnaryOperator::Negate)
                    && let ExprKind::Number(value) = &operand.kind
                {
                    return lower_number(self.source, value, true, expression.span);
                }
                let operand = self.lower_expr(operand)?;
                let expected = match operator {
                    UnaryOperator::Negate
                        if operand.ty.is_signed_integer() || operand.ty.is_float() =>
                    {
                        operand.ty
                    }
                    UnaryOperator::Negate => {
                        return Err(Diagnostic::at(
                            self.source,
                            expression.span,
                            format!("cannot negate `{}`", operand.ty),
                        ));
                    }
                    UnaryOperator::Not => Type::Bool,
                };
                self.require_type(operand.ty, expected, expression.span)?;
                Ok(Expr {
                    kind: ir::ExprKind::Unary {
                        operator: *operator,
                        operand: Box::new(operand),
                    },
                    ty: expected,
                })
            }
            ExprKind::Binary {
                operator,
                left,
                right,
            } => self.lower_binary(*operator, left, right, expression.span),
            ExprKind::Field {
                base,
                field,
                field_span,
            } => self.lower_field(base, field, *field_span),
            ExprKind::AddressOf { operand } => self.lower_address_of(operand, expression.span),
            ExprKind::Deref { operand } => self.lower_deref(operand, expression.span),
            ExprKind::PtrField {
                base,
                field,
                field_span,
            } => self.lower_ptr_field(base, field, *field_span),
            ExprKind::Match { value, arms } => self.lower_match(value, arms),
        }
    }

    fn lower_call(
        &mut self,
        callee: &str,
        callee_span: Span,
        arguments: &[ast::Expr],
    ) -> Result<Expr, Diagnostic> {
        if callee == "length" {
            if arguments.len() != 1 {
                return Err(Diagnostic::at(
                    self.source,
                    callee_span,
                    "`length` expects one argument",
                ));
            }
            let value = self.lower_expr(&arguments[0])?;
            match value.ty {
                Type::String => {
                    return Ok(Expr {
                        kind: ir::ExprKind::StringLength(Box::new(value)),
                        ty: Type::I32,
                    });
                }
                Type::Slice(_) => {
                    return Ok(Expr {
                        kind: ir::ExprKind::SliceLen(Box::new(value)),
                        ty: Type::I32,
                    });
                }
                other => {
                    return Err(Diagnostic::at(
                        self.source,
                        arguments[0].span,
                        format!("`length` expects a string or slice, found `{other}`"),
                    ));
                }
            }
        }
        if callee == "allocate" {
            if arguments.len() != 1 {
                return Err(Diagnostic::at(
                    self.source,
                    callee_span,
                    "`allocate` expects one argument",
                ));
            }
            let value = self.lower_expr(&arguments[0])?;
            if !value.ty.is_integer() {
                return Err(Diagnostic::at(
                    self.source,
                    arguments[0].span,
                    format!("`allocate` expects an integer length, found `{}`", value.ty),
                ));
            }
            return Ok(Expr {
                kind: ir::ExprKind::Allocate(Box::new(value)),
                ty: Type::Slice(crate::ir::ScalarType::U8),
            });
        }
        if callee == "free" {
            if arguments.len() != 1 {
                return Err(Diagnostic::at(
                    self.source,
                    callee_span,
                    "`free` expects one argument",
                ));
            }
            let value = self.lower_expr(&arguments[0])?;
            // §12.3a：`string` 与 `[]u8` 别名等价，`free` 直接复用字节切片通道。
            if !matches!(value.ty, Type::Slice(_) | Type::String) {
                return Err(Diagnostic::at(
                    self.source,
                    arguments[0].span,
                    format!("`free` expects a `[]u8` slice, found `{}`", value.ty),
                ));
            }
            return Ok(Expr {
                kind: ir::ExprKind::Free(Box::new(value)),
                ty: Type::Unit,
            });
        }
        if matches!(callee, "print" | "println") {
            return Err(Diagnostic::at(
                self.source,
                callee_span,
                format!("`{callee}` can only be used as a statement"),
            ));
        }
        // 结构体构造：`TypeName(args)`。
        if let Some(id) = self.type_ids.get(callee)
            && let TypeDef::Struct { .. } = &self.types[id.0]
        {
            return self.lower_struct_init(*id, arguments, callee_span);
        }
        // 枚举项构造：`Enum.Variant(args)`。
        if let Some((enum_name, variant_name)) = callee.rsplit_once('.')
            && let Some(id) = self.type_ids.get(enum_name)
            && let TypeDef::Enum { variants, .. } = &self.types[id.0]
        {
            return self.lower_enum_init(*id, variants, variant_name, arguments, callee_span);
        }
        let signature = self.signatures.get(callee).ok_or_else(|| {
            Diagnostic::at(
                self.source,
                callee_span,
                format!("unknown function `{callee}`"),
            )
        })?;
        if arguments.len() != signature.parameters.len() {
            return Err(Diagnostic::at(
                self.source,
                callee_span,
                format!(
                    "function `{callee}` expects {} arguments but {} were provided",
                    signature.parameters.len(),
                    arguments.len()
                ),
            ));
        }
        let mut lowered = Vec::with_capacity(arguments.len());
        for (argument, expected) in arguments.iter().zip(&signature.parameters) {
            let value = self.lower_expr(argument)?;
            self.require_type(value.ty, *expected, argument.span)?;
            lowered.push(value);
        }
        Ok(Expr {
            kind: ir::ExprKind::Call {
                function: signature.id,
                arguments: lowered,
            },
            ty: signature.return_type,
        })
    }

    fn lower_struct_init(
        &mut self,
        id: TypeId,
        arguments: &[ast::Expr],
        span: Span,
    ) -> Result<Expr, Diagnostic> {
        let TypeDef::Struct { fields, .. } = &self.types[id.0] else {
            unreachable!("struct init resolves a struct type");
        };
        if arguments.len() != fields.len() {
            return Err(Diagnostic::at(
                self.source,
                span,
                format!(
                    "struct constructor expects {} arguments but {} were provided",
                    fields.len(),
                    arguments.len()
                ),
            ));
        }
        let mut lowered = Vec::with_capacity(arguments.len());
        for (argument, field) in arguments.iter().zip(fields) {
            let value = self.lower_expr(argument)?;
            self.require_type(value.ty, field.ty, argument.span)?;
            lowered.push(value);
        }
        Ok(Expr {
            kind: ir::ExprKind::StructInit { fields: lowered },
            ty: Type::Struct(id),
        })
    }

    fn lower_enum_init(
        &mut self,
        id: TypeId,
        variants: &[EnumVariant],
        variant_name: &str,
        arguments: &[ast::Expr],
        span: Span,
    ) -> Result<Expr, Diagnostic> {
        let index = variants
            .iter()
            .position(|variant| variant.name == variant_name)
            .ok_or_else(|| {
                Diagnostic::at(
                    self.source,
                    span,
                    format!("unknown variant `{variant_name}`"),
                )
            })?;
        let variant = &variants[index];
        if arguments.len() != variant.fields.len() {
            return Err(Diagnostic::at(
                self.source,
                span,
                format!(
                    "variant `{variant_name}` expects {} arguments but {} were provided",
                    variant.fields.len(),
                    arguments.len()
                ),
            ));
        }
        let mut lowered = Vec::with_capacity(arguments.len());
        for (argument, expected) in arguments.iter().zip(&variant.fields) {
            let value = self.lower_expr(argument)?;
            self.require_type(value.ty, *expected, argument.span)?;
            lowered.push(value);
        }
        Ok(Expr {
            kind: ir::ExprKind::EnumInit {
                variant: index,
                arguments: lowered,
            },
            ty: Type::Enum(id),
        })
    }

    fn lower_field(
        &mut self,
        base: &ast::Expr,
        field: &str,
        field_span: Span,
    ) -> Result<Expr, Diagnostic> {
        // 无参枚举项引用：`Enum.Variant`（无括号）等价于 `Enum.Variant()`。
        if let ExprKind::Name(name) = &base.kind
            && let Some(id) = self.type_ids.get(name)
            && let TypeDef::Enum { variants, .. } = &self.types[id.0]
        {
            let index = variants
                .iter()
                .position(|variant| variant.name == field)
                .ok_or_else(|| {
                    Diagnostic::at(
                        self.source,
                        field_span,
                        format!("unknown variant `{field}`"),
                    )
                })?;
            if !variants[index].fields.is_empty() {
                return Err(Diagnostic::at(
                    self.source,
                    field_span,
                    format!("variant `{field}` requires arguments"),
                ));
            }
            return Ok(Expr {
                kind: ir::ExprKind::EnumInit {
                    variant: index,
                    arguments: Vec::new(),
                },
                ty: Type::Enum(*id),
            });
        }
        let base_value = self.lower_expr(base)?;
        let Type::Struct(id) = base_value.ty else {
            return Err(Diagnostic::at(
                self.source,
                field_span,
                format!("cannot access field on value of type `{}`", base_value.ty),
            ));
        };
        let TypeDef::Struct { fields, .. } = &self.types[id.0] else {
            unreachable!("struct field access resolves a struct type");
        };
        let index = fields
            .iter()
            .position(|field_def| field_def.name == field)
            .ok_or_else(|| {
                Diagnostic::at(
                    self.source,
                    field_span,
                    format!("struct has no field named `{field}`"),
                )
            })?;
        Ok(Expr {
            kind: ir::ExprKind::Field {
                base: Box::new(base_value),
                field: index,
            },
            ty: fields[index].ty,
        })
    }

    fn lower_address_of(&mut self, operand: &ast::Expr, span: Span) -> Result<Expr, Diagnostic> {
        // 首版最小模型：`&` 仅作用于局部变量（`&p`）。字段地址 `&s.field` 属文档约定
        // 的安全子集（§12.7），本里程碑不实现。
        if !matches!(operand.kind, ExprKind::Name(_)) {
            return Err(Diagnostic::at(
                self.source,
                span,
                "`&` can only take the address of a local variable in this milestone",
            ));
        }
        let value = self.lower_expr(operand)?;
        let pointer = match value.ty {
            Type::Struct(id) => crate::ir::Pointer::Struct(id),
            Type::Enum(id) => crate::ir::Pointer::Enum(id),
            ty => match ty.as_scalar() {
                Some(scalar) => crate::ir::Pointer::Scalar(scalar),
                None => {
                    return Err(Diagnostic::at(
                        self.source,
                        span,
                        format!("cannot take the address of a value of type `{ty}`"),
                    ));
                }
            },
        };
        Ok(Expr {
            kind: ir::ExprKind::AddressOf(Box::new(value)),
            ty: Type::Ptr(pointer),
        })
    }

    fn lower_deref(&mut self, operand: &ast::Expr, span: Span) -> Result<Expr, Diagnostic> {
        let value = self.lower_expr(operand)?;
        let Type::Ptr(pointer) = value.ty else {
            return Err(Diagnostic::at(
                self.source,
                span,
                format!("cannot dereference value of type `{}`", value.ty),
            ));
        };
        let pointee = pointer_type(pointer, self.types);
        Ok(Expr {
            kind: ir::ExprKind::Deref(Box::new(value)),
            ty: pointee,
        })
    }

    fn lower_ptr_field(
        &mut self,
        base: &ast::Expr,
        field: &str,
        field_span: Span,
    ) -> Result<Expr, Diagnostic> {
        let base_value = self.lower_expr(base)?;
        let Type::Ptr(crate::ir::Pointer::Struct(id)) = base_value.ty else {
            return Err(Diagnostic::at(
                self.source,
                field_span,
                format!(
                    "`->` requires a pointer to a struct, found `{}`",
                    base_value.ty
                ),
            ));
        };
        let TypeDef::Struct { fields, .. } = &self.types[id.0] else {
            unreachable!("pointer field access resolves a struct type");
        };
        let index = fields
            .iter()
            .position(|field_def| field_def.name == field)
            .ok_or_else(|| {
                Diagnostic::at(
                    self.source,
                    field_span,
                    format!("struct has no field named `{field}`"),
                )
            })?;
        Ok(Expr {
            kind: ir::ExprKind::PtrField {
                base: Box::new(base_value),
                field: index,
            },
            ty: fields[index].ty,
        })
    }

    fn lower_match(
        &mut self,
        value: &ast::Expr,
        arms: &[ast::MatchArm],
    ) -> Result<Expr, Diagnostic> {
        let value_span = value.span;
        let value = self.lower_expr(value)?;
        let Type::Enum(id) = value.ty else {
            return Err(Diagnostic::at(
                self.source,
                value_span,
                "`match` expects an enum value",
            ));
        };
        let TypeDef::Enum { variants, .. } = &self.types[id.0] else {
            unreachable!("match resolves an enum type");
        };

        let mut lowered_arms = Vec::with_capacity(arms.len());
        let mut result_type: Option<Type> = None;
        let mut matched = vec![false; variants.len()];
        let mut has_wildcard = false;

        for arm in arms {
            let (variant_index, binding_names, pattern_span) = match &arm.pattern {
                ast::MatchPattern::Wildcard => {
                    has_wildcard = true;
                    (None, Vec::new(), value_span)
                }
                ast::MatchPattern::Enum {
                    name,
                    name_span,
                    bindings,
                } => {
                    let variant_name = name.rsplit_once('.').map(|(_, v)| v).unwrap_or(name);
                    let index = variants
                        .iter()
                        .position(|variant| variant.name == variant_name)
                        .ok_or_else(|| {
                            Diagnostic::at(
                                self.source,
                                *name_span,
                                format!("unknown variant `{variant_name}`"),
                            )
                        })?;
                    if matched[index] {
                        return Err(Diagnostic::at(
                            self.source,
                            *name_span,
                            format!("variant `{variant_name}` is matched more than once"),
                        ));
                    }
                    matched[index] = true;
                    (Some(index), bindings.clone(), *name_span)
                }
            };

            // 为解构绑定创建局部变量：与 variant 字段一一对应。
            let mut binding_locals = Vec::new();
            let mut binding_types = Vec::new();
            if let Some(index) = variant_index {
                let variant = &variants[index];
                if binding_names.len() != variant.fields.len() {
                    return Err(Diagnostic::at(
                        self.source,
                        pattern_span,
                        format!(
                            "variant `{}` expects {} bindings but {} were provided",
                            variant.name,
                            variant.fields.len(),
                            binding_names.len()
                        ),
                    ));
                }
                for field in &variant.fields {
                    binding_locals.push(self.new_local(*field));
                    binding_types.push(*field);
                }
            }

            // 在绑定作用域内 lowering 分支体。
            let body = self.with_scope(|lowerer| {
                let scope = lowerer.scopes.last_mut().unwrap();
                for ((name, local), ty) in binding_names
                    .iter()
                    .zip(binding_locals.iter())
                    .zip(binding_types.iter())
                {
                    // `_` 表示忽略该字段，不绑定到任何变量。
                    if name == "_" {
                        continue;
                    }
                    scope.insert(
                        name.clone(),
                        Binding {
                            local: *local,
                            ty: *ty,
                            mutable: false,
                        },
                    );
                }
                lowerer.lower_expr(&arm.body)
            })?;

            match result_type {
                None => result_type = Some(body.ty),
                Some(expected) => self.require_type(body.ty, expected, arm.body.span)?,
            }
            let pattern = match variant_index {
                Some(v) => ir::MatchPattern::Variant {
                    variant: v,
                    bindings: binding_locals,
                },
                None => ir::MatchPattern::Wildcard,
            };
            lowered_arms.push(ir::MatchArm { pattern, body });
        }

        // 穷尽性检查：所有 variant 都被匹配，或存在通配符。
        if !has_wildcard {
            for (index, is_matched) in matched.iter().enumerate() {
                if !is_matched {
                    return Err(Diagnostic::at(
                        self.source,
                        value_span,
                        format!("match is missing variant `{}`", variants[index].name),
                    ));
                }
            }
        }

        let ty = result_type.unwrap_or(Type::Unit);
        Ok(Expr {
            kind: ir::ExprKind::Match {
                value: Box::new(value),
                arms: lowered_arms,
            },
            ty,
        })
    }

    fn lower_binary(
        &mut self,
        operator: BinaryOperator,
        left: &ast::Expr,
        right: &ast::Expr,
        span: Span,
    ) -> Result<Expr, Diagnostic> {
        let left = self.lower_expr(left)?;
        let right = self.lower_expr(right)?;
        // 字符串拼接 `a + b`（M14 §12.3b）：唯一隐式分配点。
        if matches!(operator, BinaryOperator::Add) && left.ty == Type::String {
            self.require_type(right.ty, Type::String, span)?;
            return Ok(Expr {
                kind: ir::ExprKind::StringConcat {
                    left: Box::new(left),
                    right: Box::new(right),
                },
                ty: Type::String,
            });
        }
        let ty = match operator {
            BinaryOperator::Add
            | BinaryOperator::Subtract
            | BinaryOperator::Multiply
            | BinaryOperator::Divide
            | BinaryOperator::Remainder => {
                if !(left.ty.is_integer() || left.ty.is_float())
                    || matches!(operator, BinaryOperator::Remainder) && left.ty.is_float()
                {
                    return Err(Diagnostic::at(
                        self.source,
                        span,
                        format!("operator is not supported for `{}`", left.ty),
                    ));
                }
                self.require_type(right.ty, left.ty, span)?;
                left.ty
            }
            BinaryOperator::Less
            | BinaryOperator::LessEqual
            | BinaryOperator::Greater
            | BinaryOperator::GreaterEqual => {
                if !(left.ty.is_integer() || left.ty.is_float() || left.ty == Type::Char) {
                    return Err(Diagnostic::at(
                        self.source,
                        span,
                        format!("values of type `{}` cannot be ordered", left.ty),
                    ));
                }
                self.require_type(right.ty, left.ty, span)?;
                Type::Bool
            }
            BinaryOperator::Equal | BinaryOperator::NotEqual => {
                if matches!(left.ty, Type::Unit | Type::Array { .. }) {
                    return Err(Diagnostic::at(
                        self.source,
                        span,
                        format!("values of type `{}` cannot be compared", left.ty),
                    ));
                }
                self.require_type(right.ty, left.ty, span)?;
                Type::Bool
            }
            BinaryOperator::And | BinaryOperator::Or => {
                self.require_type(left.ty, Type::Bool, span)?;
                self.require_type(right.ty, Type::Bool, span)?;
                Type::Bool
            }
        };
        Ok(Expr {
            kind: ir::ExprKind::Binary {
                operator,
                left: Box::new(left),
                right: Box::new(right),
            },
            ty,
        })
    }

    fn require_type(&self, actual: Type, expected: Type, span: Span) -> Result<(), Diagnostic> {
        if actual == expected {
            Ok(())
        } else {
            Err(Diagnostic::at(
                self.source,
                span,
                format!("expected `{expected}`, found `{actual}`"),
            ))
        }
    }

    fn lookup(&self, name: &str, span: Span) -> Result<&Binding, Diagnostic> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name))
            .ok_or_else(|| Diagnostic::at(self.source, span, format!("unknown variable `{name}`")))
    }

    fn new_local(&mut self, ty: Type) -> LocalId {
        let local = LocalId(self.locals.len());
        self.locals.push(ty);
        local
    }

    fn store_temporary(&mut self, value: Expr) -> LocalId {
        let local = self.new_local(value.ty);
        self.emit(Instruction::SetLocal { local, value });
        local
    }

    fn with_scope<T>(
        &mut self,
        operation: impl FnOnce(&mut Self) -> Result<T, Diagnostic>,
    ) -> Result<T, Diagnostic> {
        self.scopes.push(HashMap::new());
        self.deferred.push(Vec::new());
        self.try_resources.push(HashSet::new());
        let result = operation(self);
        // fall-through：本层 defer 动作逆序执行（提前 return 已由 emit_pending_defers drain）。
        let layer = self.deferred.pop().expect("defer scope is balanced");
        for action in layer.into_iter().rev() {
            self.emit(Instruction::Evaluate(action));
        }
        self.try_resources.pop();
        self.scopes.pop();
        result
    }

    /// 在提前退出（return / break / continue）前，从最内到最外执行所有挂起的 defer。
    fn emit_pending_defers(&mut self) {
        let mut pending = Vec::new();
        for layer in self.deferred.iter_mut().rev() {
            for action in layer.drain(..).rev() {
                pending.push(action);
            }
        }
        for action in pending {
            self.emit(Instruction::Evaluate(action));
        }
    }

    /// 判断当前作用域链中，`name` 是否为 `try` 资源。
    fn is_try_resource(&self, name: &str) -> bool {
        self.try_resources
            .iter()
            .rev()
            .any(|layer| layer.contains(name))
    }

    /// `try` 资源逃逸检查（§4.2/§12.2）：资源名作为「值」被复制/绑定/返回即报错。
    ///
    /// 语法级检查：只要 `value` 顶层直接是资源名（`Name(resource)`），即判定逃逸；
    /// 原地写入（`buf[i] = ...`）与只读传入（`print(buf)`）不受影响，因为它们
    /// 不把资源名作为「值」绑定给新变量。
    fn check_try_escape(&self, value: &ast::Expr, span: Span) -> Result<(), Diagnostic> {
        if matches!(&value.kind, ast::ExprKind::Name(name) if self.is_try_resource(name)) {
            return Err(Diagnostic::at(
                self.source,
                span,
                "`try` resource cannot escape its block (re-bind, assign out, or return)",
            ));
        }
        Ok(())
    }

    fn new_block(&mut self) -> BlockId {
        let id = BlockId(self.blocks.len());
        self.blocks.push(WorkingBlock {
            instructions: Vec::new(),
            terminator: None,
            reachable: false,
        });
        id
    }

    fn emit(&mut self, instruction: Instruction) {
        self.blocks[self.current.0].instructions.push(instruction);
    }

    fn terminate(&mut self, terminator: Terminator) {
        debug_assert!(self.blocks[self.current.0].terminator.is_none());
        if self.blocks[self.current.0].reachable {
            match &terminator {
                Terminator::Jump(target) => self.blocks[target.0].reachable = true,
                Terminator::Branch {
                    then_block,
                    else_block,
                    ..
                } => {
                    self.blocks[then_block.0].reachable = true;
                    self.blocks[else_block.0].reachable = true;
                }
                Terminator::Return(_) => {}
            }
        }
        self.blocks[self.current.0].terminator = Some(terminator);
    }

    fn is_terminated(&self, block: BlockId) -> bool {
        self.blocks[block.0].terminator.is_some()
    }
}

enum FormatPart {
    Text(String),
    Placeholder,
}

fn parse_format(
    source: &SourceFile,
    format: &str,
    span: Span,
) -> Result<Vec<FormatPart>, Diagnostic> {
    let mut parts = Vec::new();
    let mut text = String::new();
    let mut chars = format.chars().peekable();
    while let Some(character) = chars.next() {
        match (character, chars.peek().copied()) {
            ('{', Some('{')) => {
                chars.next();
                text.push('{');
            }
            ('}', Some('}')) => {
                chars.next();
                text.push('}');
            }
            ('{', Some('}')) => {
                chars.next();
                parts.push(FormatPart::Text(std::mem::take(&mut text)));
                parts.push(FormatPart::Placeholder);
            }
            ('{' | '}', _) => {
                return Err(Diagnostic::at(
                    source,
                    span,
                    "unmatched brace in format string",
                ));
            }
            _ => text.push(character),
        }
    }
    parts.push(FormatPart::Text(text));
    Ok(parts)
}

fn resolve_type(
    source: &SourceFile,
    type_ref: &ast::TypeRef,
    types: &[TypeDef],
    type_ids: &HashMap<String, TypeId>,
) -> Result<Type, Diagnostic> {
    match &type_ref.kind {
        TypeRefKind::Name(name) => match name.as_str() {
            "i8" => Ok(Type::I8),
            "i16" => Ok(Type::I16),
            "i32" => Ok(Type::I32),
            "i64" => Ok(Type::I64),
            "u8" => Ok(Type::U8),
            "u16" => Ok(Type::U16),
            "u32" => Ok(Type::U32),
            "u64" => Ok(Type::U64),
            "f32" => Ok(Type::F32),
            "f64" => Ok(Type::F64),
            "char" => Ok(Type::Char),
            "bool" => Ok(Type::Bool),
            "string" => Ok(Type::String),
            _ => match type_ids.get(name) {
                Some(id) => Ok(type_of(&types[id.0], *id)),
                None => Err(Diagnostic::at(
                    source,
                    type_ref.span,
                    format!("unknown type `{name}`"),
                )),
            },
        },
        TypeRefKind::Array { element, length } => {
            if *length == 0 {
                return Err(Diagnostic::at(
                    source,
                    type_ref.span,
                    "array length must be greater than zero",
                ));
            }
            if *length > i32::MAX as usize {
                return Err(Diagnostic::at(
                    source,
                    type_ref.span,
                    "array length is too large",
                ));
            }
            let element_type = resolve_type(source, element, types, type_ids)?;
            let scalar = element_type.as_scalar().ok_or_else(|| {
                Diagnostic::at(
                    source,
                    element.span,
                    "nested arrays are not implemented yet",
                )
            })?;
            Ok(Type::Array {
                element: scalar,
                length: *length,
            })
        }
        TypeRefKind::Slice { element } => {
            // 首版切片仅支持 `[]u8`（与 `string` 别名等价，见提案 §12.3a/§12.4）。
            let element_type = resolve_type(source, element, types, type_ids)?;
            let scalar = element_type.as_scalar().ok_or_else(|| {
                Diagnostic::at(source, element.span, "slice elements must be scalar values")
            })?;
            if !matches!(scalar, crate::ir::ScalarType::U8) {
                return Err(Diagnostic::at(
                    source,
                    element.span,
                    "only `[]u8` slices are supported in this milestone",
                ));
            }
            Ok(Type::Slice(scalar))
        }
        TypeRefKind::Pointer { inner } => {
            let inner_type = resolve_type(source, inner, types, type_ids)?;
            let pointer = match inner_type {
                Type::Struct(id) => crate::ir::Pointer::Struct(id),
                Type::Enum(id) => crate::ir::Pointer::Enum(id),
                ty => match ty.as_scalar() {
                    Some(scalar) => crate::ir::Pointer::Scalar(scalar),
                    None => {
                        return Err(Diagnostic::at(
                            source,
                            inner.span,
                            format!("cannot form a pointer to `{ty}` in this milestone"),
                        ));
                    }
                },
            };
            Ok(Type::Ptr(pointer))
        }
    }
}

/// 根据类型定义构造对应的 `Type`。
fn type_of(def: &TypeDef, id: TypeId) -> Type {
    match def {
        TypeDef::Struct { .. } => Type::Struct(id),
        TypeDef::Enum { .. } => Type::Enum(id),
    }
}

/// 解引用指针后的类型（M14）。
fn pointer_type(pointer: Pointer, types: &[TypeDef]) -> Type {
    match pointer {
        Pointer::Scalar(scalar) => scalar.as_type(),
        Pointer::Struct(id) => type_of(&types[id.0], id),
        Pointer::Enum(id) => type_of(&types[id.0], id),
    }
}

fn local_expr(local: LocalId, ty: Type) -> Expr {
    Expr {
        kind: ir::ExprKind::Local(local),
        ty,
    }
}

fn default_expr(ty: Type) -> Expr {
    match ty {
        Type::Unit => unreachable!("Unit returns do not need a value"),
        ty if ty.is_integer() => Expr {
            kind: ir::ExprKind::Integer(0),
            ty,
        },
        ty if ty.is_float() => Expr {
            kind: ir::ExprKind::Float(0.0),
            ty,
        },
        Type::Char => Expr {
            kind: ir::ExprKind::Char('\0'),
            ty,
        },
        Type::Bool => Expr {
            kind: ir::ExprKind::Bool(false),
            ty,
        },
        Type::String => Expr {
            kind: ir::ExprKind::String(String::new()),
            ty,
        },
        Type::Array { element, length } => Expr {
            kind: ir::ExprKind::RepeatArray {
                value: Box::new(default_expr(element.as_type())),
                length,
            },
            ty,
        },
        _ => unreachable!("all scalar types have defaults"),
    }
}

fn assignment_binary(operator: AssignmentOperator) -> BinaryOperator {
    match operator {
        AssignmentOperator::Add => BinaryOperator::Add,
        AssignmentOperator::Subtract => BinaryOperator::Subtract,
        AssignmentOperator::Multiply => BinaryOperator::Multiply,
        AssignmentOperator::Divide => BinaryOperator::Divide,
        AssignmentOperator::Remainder => BinaryOperator::Remainder,
        AssignmentOperator::Assign => unreachable!("plain assignment is handled separately"),
    }
}

fn lower_number(
    source: &SourceFile,
    raw: &str,
    negative: bool,
    span: Span,
) -> Result<Expr, Diagnostic> {
    let (digits, suffix) = raw.rsplit_once('_').unwrap_or((raw, ""));
    let float_literal = digits.contains(['.', 'e', 'E']);
    if float_literal || matches!(suffix, "f32" | "f64") {
        let ty = match suffix {
            "f32" => Type::F32,
            "" => Type::F64,
            "f64" => Type::F64,
            _ => {
                return Err(Diagnostic::at(
                    source,
                    span,
                    format!("invalid numeric suffix `{suffix}`"),
                ));
            }
        };
        let mut value = digits
            .parse::<f64>()
            .map_err(|_| Diagnostic::at(source, span, "invalid floating-point literal"))?;
        if negative {
            value = -value;
        }
        if !value.is_finite() || ty == Type::F32 && !(value as f32).is_finite() {
            return Err(Diagnostic::at(
                source,
                span,
                format!("literal does not fit in `{ty}`"),
            ));
        }
        return Ok(Expr {
            kind: ir::ExprKind::Float(value),
            ty,
        });
    }
    let ty = match suffix {
        "" | "i32" => Type::I32,
        "i8" => Type::I8,
        "i16" => Type::I16,
        "i64" => Type::I64,
        "u8" => Type::U8,
        "u16" => Type::U16,
        "u32" => Type::U32,
        "u64" => Type::U64,
        _ => {
            return Err(Diagnostic::at(
                source,
                span,
                format!("invalid numeric suffix `{suffix}`"),
            ));
        }
    };
    if negative && !ty.is_signed_integer() {
        return Err(Diagnostic::at(
            source,
            span,
            format!("cannot negate `{ty}` literal"),
        ));
    }
    let magnitude = digits
        .parse::<u64>()
        .map_err(|_| Diagnostic::at(source, span, "integer literal is too large"))?;
    let bits = ty.bits().unwrap();
    let maximum = if ty.is_signed_integer() {
        (1u128 << (bits - 1)) - 1
    } else if bits == 64 {
        u64::MAX as u128
    } else {
        (1u128 << bits) - 1
    };
    let allowed = maximum + u128::from(negative);
    if u128::from(magnitude) > allowed {
        return Err(Diagnostic::at(
            source,
            span,
            format!("literal does not fit in `{ty}`"),
        ));
    }
    let value = if negative {
        0u64.wrapping_sub(magnitude)
    } else {
        magnitude
    };
    Ok(Expr {
        kind: ir::ExprKind::Integer(value),
        ty,
    })
}

fn is_castable(ty: Type) -> bool {
    ty.is_integer() || ty.is_float() || ty == Type::Char
}

fn parse_unsuffixed_integer(value: &str) -> Option<i64> {
    (!value.contains('_') && !value.contains(['.', 'e', 'E']))
        .then(|| value.parse::<i64>().ok())
        .flatten()
}

fn check_constant_index(
    source: &SourceFile,
    index: &ast::Expr,
    length: usize,
) -> Result<(), Diagnostic> {
    let constant = match &index.kind {
        ExprKind::Number(value) => parse_unsuffixed_integer(value),
        ExprKind::Unary {
            operator: UnaryOperator::Negate,
            operand,
        } => match &operand.kind {
            ExprKind::Number(value) => parse_unsuffixed_integer(value).map(|value| -value),
            _ => None,
        },
        _ => None,
    };
    if constant.is_some_and(|value| value < 0 || value as usize >= length) {
        Err(Diagnostic::at(
            source,
            index.span,
            format!("array index is out of bounds for length {length}"),
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::{lexer, parser};

    use super::*;

    fn lower_text(text: &str) -> Result<ir::Program, Diagnostic> {
        let source = SourceFile::new(PathBuf::from("main.do"), text.to_string());
        let tokens = lexer::lex(&source)?;
        let ast = parser::parse(&source, tokens)?;
        lower(&source, &ast)
    }

    #[test]
    fn lowers_forward_function_calls() {
        let program = lower_text(
            "fn main() { return add(2, 3); } fn add(a: i32, b: i32): i32 { return a + b; }",
        )
        .unwrap();
        assert_eq!(program.functions.len(), 2);
    }

    #[test]
    fn rejects_wrong_call_arguments() {
        let error = lower_text("fn add(a: i32): i32 { return a; } fn main() { return add(true); }")
            .unwrap_err();
        assert!(error.to_string().contains("expected `i32`, found `bool`"));
    }

    #[test]
    fn validates_print_placeholders() {
        lower_text("fn main() { println(\"{} = {}\", \"answer\", 42); }").unwrap();
        let error = lower_text("fn main() { println(\"{} {}\", 1); }").unwrap_err();
        assert!(error.to_string().contains("2 placeholders"));
    }

    #[test]
    fn rejects_duplicate_functions_and_missing_returns() {
        let duplicate = lower_text("fn main() {} fn main() {}").unwrap_err();
        assert!(duplicate.to_string().contains("already defined"));

        let missing =
            lower_text("fn value(flag: bool): i32 { if flag { return 1; } } fn main() {}")
                .unwrap_err();
        assert!(
            missing
                .to_string()
                .contains("may exit without returning `i32`")
        );
    }

    #[test]
    fn rejects_assignment_to_val() {
        let error = lower_text("fn main() { val answer = 42; answer = 0; }").unwrap_err();
        assert!(error.to_string().contains("immutable variable"));
    }

    #[test]
    fn requires_boolean_conditions() {
        let error = lower_text("fn main() { if 1 { return 1; } }").unwrap_err();
        assert!(error.to_string().contains("expected `bool`"));
    }

    #[test]
    fn rejects_loop_control_outside_loop() {
        let break_error = lower_text("fn main() { break; }").unwrap_err();
        assert!(break_error.to_string().contains("outside a loop"));
    }

    #[test]
    fn enforces_block_scope() {
        let error =
            lower_text("fn main() { if true { val scoped = 1; } return scoped; }").unwrap_err();
        assert!(error.to_string().contains("unknown variable `scoped`"));
    }

    #[test]
    fn validates_m6_array_types_and_mutability() {
        lower_text(
            "fn copy(value: [i32; 2]): [i32; 2] { return value; } fn main() { var a = copy([1, 2]); a[0] = 3; for x in a { println(\"{}\", x); } }",
        )
        .unwrap();

        let immutable =
            lower_text("fn main() { val values = [1, 2]; values[0] = 3; }").unwrap_err();
        assert!(immutable.to_string().contains("immutable array"));

        let mismatch =
            lower_text("fn take(value: [i32; 2]) {} fn main() { take([1, 2, 3]); }").unwrap_err();
        assert!(mismatch.to_string().contains("expected `[i32; 2]`"));

        let bounds =
            lower_text("fn main() { val values = [1, 2]; return values[-1]; }").unwrap_err();
        assert!(bounds.to_string().contains("out of bounds"));
    }

    #[test]
    fn m13_lowers_structs_and_enums() {
        lower_text(
            "struct Point { x: i32, y: i32 } enum Shape { Circle(f64), Rectangle(f64, f64), Empty } fn main() { var p = Point(1, 2); val c = Shape.Circle(2.0); val area = match c { Shape.Circle(r) => r, Shape.Rectangle(w, h) => w + h, Shape.Empty => 0.0 }; return p.x + area as i32; }",
        )
        .unwrap();
    }

    #[test]
    fn m14_allows_user_types_in_function_signatures() {
        // M14 起结构体可作为函数参数与返回值（按值传递，浅拷贝）。
        lower_text("struct Point { x: i32 } fn area(p: Point): i32 { return p.x; } fn main() {}")
            .unwrap();
        lower_text("struct Point { x: i32 } fn make(): Point { return Point(1); } fn main() {}")
            .unwrap();
    }

    #[test]
    fn m13_rejects_unknown_fields_and_non_exhaustive_match() {
        let field =
            lower_text("struct Point { x: i32 } fn main() { var p = Point(1); return p.y; }")
                .unwrap_err();
        assert!(field.to_string().contains("no field named `y`"));

        let exhaustive = lower_text(
            "enum Shape { Circle(f64), Rectangle(f64, f64) } fn main() { val s = Shape.Circle(1.0); return match s { Shape.Circle(r) => 1 }; }",
        )
        .unwrap_err();
        assert!(exhaustive.to_string().contains("missing variant"));
    }
}
