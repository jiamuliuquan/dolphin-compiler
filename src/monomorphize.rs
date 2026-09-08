//! M15 单态化（monomorphization）：在 lowering 之前把泛型函数/类型展开成具体实例。
//!
//! 见提案 §3.4：单态化是 AST 层预处理 pass，IR 与 codegen 零改动。
//! 实例名用稳定 mangling（`id<T=i32>` → `id$i32`）。
//!
//! MVP 范围：泛型实参仅支持字面量（整数/浮点/字符/布尔/字符串），暂不支持
//! 从变量推断实参类型或嵌套泛型调用。

use std::collections::{HashMap, HashSet};

use crate::ast::{self, Expr, ExprKind, Statement, StatementKind, TypeRef, TypeRefKind};
use crate::diagnostic::Diagnostic;
use crate::source::SourceFile;

pub fn monomorphize(
    sources: &[SourceFile],
    program: &mut ast::Program,
) -> Result<(), Diagnostic> {
    let _ = sources;
    Monomorphizer.run(program)
}

/// 类型实参：一组具体的 `TypeRef`。
type TypeArgs = Vec<TypeRef>;

/// 泛型定义集合（按种类分开，用于区分三种 `Call` 形式）。
struct Generics {
    fns: HashSet<String>,
    structs: HashSet<String>,
    enums: HashSet<String>,
}

struct Monomorphizer;

impl Monomorphizer {
    fn run(&self, program: &mut ast::Program) -> Result<(), Diagnostic> {
        let generics = Generics {
            fns: program
                .functions
                .iter()
                .filter(|f| !f.type_params.is_empty())
                .map(|f| f.name.clone())
                .collect(),
            structs: program
                .structs
                .iter()
                .filter(|s| !s.type_params.is_empty())
                .map(|s| s.name.clone())
                .collect(),
            enums: program
                .enums
                .iter()
                .filter(|e| !e.type_params.is_empty())
                .map(|e| e.name.clone())
                .collect(),
        };

        if generics.fns.is_empty() && generics.structs.is_empty() && generics.enums.is_empty() {
            return Ok(());
        }

        // 1. 收集实例请求。
        let mut fn_instances: HashSet<(String, TypeArgs)> = HashSet::new();
        let mut type_instances: HashSet<(String, TypeArgs)> = HashSet::new();
        collect_program(program, &generics, &mut fn_instances, &mut type_instances)?;

        // 2. 实例化泛型函数。
        let mut concrete_fns: Vec<ast::Function> = Vec::new();
        for function in program.functions.drain(..) {
            if function.type_params.is_empty() {
                concrete_fns.push(function);
                continue;
            }
            for (name, args) in &fn_instances {
                if *name != function.name {
                    continue;
                }
                concrete_fns.push(instantiate_function(&function, &function.type_params, args));
            }
        }
        program.functions = concrete_fns;

        // 3. 实例化泛型结构体/枚举。
        let mut concrete_structs: Vec<ast::StructDecl> = Vec::new();
        for structure in program.structs.drain(..) {
            if structure.type_params.is_empty() {
                concrete_structs.push(structure);
                continue;
            }
            for (name, args) in &type_instances {
                if *name != structure.name {
                    continue;
                }
                concrete_structs.push(instantiate_struct(&structure, &structure.type_params, args));
            }
        }
        program.structs = concrete_structs;

        let mut concrete_enums: Vec<ast::EnumDecl> = Vec::new();
        for enumeration in program.enums.drain(..) {
            if enumeration.type_params.is_empty() {
                concrete_enums.push(enumeration);
                continue;
            }
            for (name, args) in &type_instances {
                if *name != enumeration.name {
                    continue;
                }
                concrete_enums.push(instantiate_enum(&enumeration, &enumeration.type_params, args));
            }
        }
        program.enums = concrete_enums;

        // 4. 重写残留的泛型引用为 mangling 名。
        rewrite_program(program, &generics);

        Ok(())
    }
}

// ── 收集 ──────────────────────────────────────────────────────────────────

fn collect_program(
    program: &ast::Program,
    generics: &Generics,
    fn_instances: &mut HashSet<(String, TypeArgs)>,
    type_instances: &mut HashSet<(String, TypeArgs)>,
) -> Result<(), Diagnostic> {
    let generic_types = union_types(generics);
    for function in &program.functions {
        if let Some(ret) = &function.return_type {
            collect_type_ref(ret, &generic_types, type_instances);
        }
        for param in &function.parameters {
            collect_type_ref(&param.ty, &generic_types, type_instances);
        }
        collect_block(&function.body, generics, fn_instances, type_instances)?;
    }
    for structure in &program.structs {
        for field in &structure.fields {
            collect_type_ref(&field.ty, &generic_types, type_instances);
        }
    }
    for enumeration in &program.enums {
        for variant in &enumeration.variants {
            for field in &variant.fields {
                collect_type_ref(field, &generic_types, type_instances);
            }
        }
    }
    Ok(())
}

fn collect_block(
    block: &[Statement],
    generics: &Generics,
    fn_instances: &mut HashSet<(String, TypeArgs)>,
    type_instances: &mut HashSet<(String, TypeArgs)>,
) -> Result<(), Diagnostic> {
    for statement in block {
        collect_statement(statement, generics, fn_instances, type_instances)?;
    }
    Ok(())
}

fn collect_statement(
    statement: &Statement,
    generics: &Generics,
    fn_instances: &mut HashSet<(String, TypeArgs)>,
    type_instances: &mut HashSet<(String, TypeArgs)>,
) -> Result<(), Diagnostic> {
    let generic_types = union_types(generics);
    match &statement.kind {
        StatementKind::Variable {
            type_name,
            initializer,
            ..
        } => {
            if let Some(ty) = type_name {
                collect_type_ref(ty, &generic_types, type_instances);
            }
            collect_expr(initializer, generics, fn_instances, type_instances)?;
        }
        StatementKind::Assignment { value, .. }
        | StatementKind::Defer { value }
        | StatementKind::Expression(value) => {
            collect_expr(value, generics, fn_instances, type_instances)?;
        }
        StatementKind::IndexAssignment { index, value, .. } => {
            collect_expr(index, generics, fn_instances, type_instances)?;
            collect_expr(value, generics, fn_instances, type_instances)?;
        }
        StatementKind::DerefAssignment { target, value, .. } => {
            collect_expr(target, generics, fn_instances, type_instances)?;
            collect_expr(value, generics, fn_instances, type_instances)?;
        }
        StatementKind::PtrFieldAssignment { base, value, .. } => {
            collect_expr(base, generics, fn_instances, type_instances)?;
            collect_expr(value, generics, fn_instances, type_instances)?;
        }
        StatementKind::Try { resources, body } => {
            for resource in resources {
                collect_expr(
                    &resource.initializer,
                    generics,
                    fn_instances,
                    type_instances,
                )?;
            }
            collect_block(body, generics, fn_instances, type_instances)?;
        }
        StatementKind::If {
            condition,
            then_block,
            else_block,
        } => {
            collect_expr(condition, generics, fn_instances, type_instances)?;
            collect_block(then_block, generics, fn_instances, type_instances)?;
            if let Some(else_block) = else_block {
                collect_block(else_block, generics, fn_instances, type_instances)?;
            }
        }
        StatementKind::Loop(body) => {
            collect_block(body, generics, fn_instances, type_instances)?;
        }
        StatementKind::While { condition, body } => {
            collect_expr(condition, generics, fn_instances, type_instances)?;
            collect_block(body, generics, fn_instances, type_instances)?;
        }
        StatementKind::For {
            iterable, body, ..
        } => {
            match iterable {
                ast::ForIterable::Range { start, end, .. } => {
                    collect_expr(start, generics, fn_instances, type_instances)?;
                    collect_expr(end, generics, fn_instances, type_instances)?;
                }
                ast::ForIterable::Array(array) => {
                    collect_expr(array, generics, fn_instances, type_instances)?;
                }
            }
            collect_block(body, generics, fn_instances, type_instances)?;
        }
        StatementKind::Break | StatementKind::Continue => {}
        StatementKind::Return(expr) => {
            if let Some(expr) = expr {
                collect_expr(expr, generics, fn_instances, type_instances)?;
            }
        }
    }
    Ok(())
}

fn collect_expr(
    expr: &Expr,
    generics: &Generics,
    fn_instances: &mut HashSet<(String, TypeArgs)>,
    type_instances: &mut HashSet<(String, TypeArgs)>,
) -> Result<(), Diagnostic> {
    match &expr.kind {
        ExprKind::Call {
            callee, arguments, ..
        } => {
            // 枚举构造 `Enum.Variant(args)`。
            if let Some((enum_name, _variant)) = callee.rsplit_once('.')
                && generics.enums.contains(enum_name)
            {
                type_instances.insert((enum_name.to_string(), infer_args(arguments)?));
            } else if generics.fns.contains(callee.as_str()) {
                // 泛型函数调用。
                fn_instances.insert((callee.clone(), infer_args(arguments)?));
            } else if generics.structs.contains(callee.as_str()) {
                // 泛型结构体构造 `Struct(args)`。
                type_instances.insert((callee.clone(), infer_args(arguments)?));
            }
            for argument in arguments {
                collect_expr(argument, generics, fn_instances, type_instances)?;
            }
        }
        ExprKind::Cast { value, ty } => {
            collect_type_ref(ty, &union_types(generics), type_instances);
            collect_expr(value, generics, fn_instances, type_instances)?;
        }
        ExprKind::Array(values) => {
            for value in values {
                collect_expr(value, generics, fn_instances, type_instances)?;
            }
        }
        ExprKind::RepeatArray { value, .. } => {
            collect_expr(value, generics, fn_instances, type_instances)?;
        }
        ExprKind::Index { array, index } => {
            collect_expr(array, generics, fn_instances, type_instances)?;
            collect_expr(index, generics, fn_instances, type_instances)?;
        }
        ExprKind::Unary { operand, .. } => {
            collect_expr(operand, generics, fn_instances, type_instances)?;
        }
        ExprKind::Binary { left, right, .. } => {
            collect_expr(left, generics, fn_instances, type_instances)?;
            collect_expr(right, generics, fn_instances, type_instances)?;
        }
        ExprKind::Field { base, .. } => {
            collect_expr(base, generics, fn_instances, type_instances)?;
        }
        ExprKind::AddressOf { operand }
        | ExprKind::Deref { operand }
        | ExprKind::PtrField { base: operand, .. } => {
            collect_expr(operand, generics, fn_instances, type_instances)?;
        }
        ExprKind::Match { value, arms } => {
            collect_expr(value, generics, fn_instances, type_instances)?;
            for arm in arms {
                collect_expr(&arm.body, generics, fn_instances, type_instances)?;
            }
        }
        ExprKind::Number(_)
        | ExprKind::Character(_)
        | ExprKind::Boolean(_)
        | ExprKind::String(_)
        | ExprKind::Name(_) => {}
    }
    Ok(())
}

/// 推断一组实参的类型（MVP：字面量）。
fn infer_args(arguments: &[Expr]) -> Result<TypeArgs, Diagnostic> {
    arguments.iter().map(infer_literal_type).collect()
}

/// 推断字面量表达式的类型；非字面量报错（MVP 限制）。
fn infer_literal_type(expr: &Expr) -> Result<TypeRef, Diagnostic> {
    let name = match &expr.kind {
        ExprKind::Number(raw) => {
            let (digits, suffix) = raw.rsplit_once('_').unwrap_or((raw.as_str(), ""));
            let float_literal = digits.contains(['.', 'e', 'E']);
            if float_literal || matches!(suffix, "f32" | "f64") {
                if suffix == "f32" {
                    "f32"
                } else {
                    "f64"
                }
            } else {
                match suffix {
                    "" | "i32" => "i32",
                    "i8" => "i8",
                    "i16" => "i16",
                    "i64" => "i64",
                    "u8" => "u8",
                    "u16" => "u16",
                    "u32" => "u32",
                    "u64" => "u64",
                    _ => {
                        return Err(Diagnostic::plain(format!(
                            "invalid numeric suffix `{suffix}` in generic argument"
                        )));
                    }
                }
            }
        }
        ExprKind::Character(_) => "char",
        ExprKind::Boolean(_) => "bool",
        ExprKind::String(_) => "string",
        _ => {
            return Err(Diagnostic::plain(
                "cannot infer generic argument type from this expression (MVP: literals only)",
            ));
        }
    };
    Ok(TypeRef {
        kind: TypeRefKind::Name(name.to_string()),
        span: expr.span,
    })
}

// ── 重写 ──────────────────────────────────────────────────────────────────

fn rewrite_program(program: &mut ast::Program, generics: &Generics) {
    let generic_types = union_types(generics);
    for function in &mut program.functions {
        for param in &mut function.parameters {
            rewrite_type_ref(&mut param.ty, &generic_types);
        }
        if let Some(ret) = &mut function.return_type {
            rewrite_type_ref(ret, &generic_types);
        }
        rewrite_block(&mut function.body, generics);
    }
    for structure in &mut program.structs {
        for field in &mut structure.fields {
            rewrite_type_ref(&mut field.ty, &generic_types);
        }
    }
    for enumeration in &mut program.enums {
        for variant in &mut enumeration.variants {
            for field in &mut variant.fields {
                rewrite_type_ref(field, &generic_types);
            }
        }
    }
}

fn rewrite_block(block: &mut [Statement], generics: &Generics) {
    for statement in block {
        rewrite_statement(statement, generics);
    }
}

fn rewrite_statement(statement: &mut Statement, generics: &Generics) {
    match &mut statement.kind {
        StatementKind::Variable {
            type_name,
            initializer,
            ..
        } => {
            if let Some(ty) = type_name {
                rewrite_type_ref(ty, &union_types(generics));
            }
            rewrite_expr(initializer, generics);
        }
        StatementKind::Assignment { value, .. } => rewrite_expr(value, generics),
        StatementKind::IndexAssignment { index, value, .. } => {
            rewrite_expr(index, generics);
            rewrite_expr(value, generics);
        }
        StatementKind::DerefAssignment { target, value, .. } => {
            rewrite_expr(target, generics);
            rewrite_expr(value, generics);
        }
        StatementKind::PtrFieldAssignment { base, value, .. } => {
            rewrite_expr(base, generics);
            rewrite_expr(value, generics);
        }
        StatementKind::Defer { value } => rewrite_expr(value, generics),
        StatementKind::Try { resources, body } => {
            for resource in resources {
                rewrite_expr(&mut resource.initializer, generics);
            }
            rewrite_block(body, generics);
        }
        StatementKind::Expression(expr) => rewrite_expr(expr, generics),
        StatementKind::If {
            condition,
            then_block,
            else_block,
        } => {
            rewrite_expr(condition, generics);
            rewrite_block(then_block, generics);
            if let Some(else_block) = else_block {
                rewrite_block(else_block, generics);
            }
        }
        StatementKind::Loop(body) => rewrite_block(body, generics),
        StatementKind::While { condition, body } => {
            rewrite_expr(condition, generics);
            rewrite_block(body, generics);
        }
        StatementKind::For {
            iterable, body, ..
        } => {
            match iterable {
                ast::ForIterable::Range { start, end, .. } => {
                    rewrite_expr(start, generics);
                    rewrite_expr(end, generics);
                }
                ast::ForIterable::Array(array) => rewrite_expr(array, generics),
            }
            rewrite_block(body, generics);
        }
        StatementKind::Break | StatementKind::Continue => {}
        StatementKind::Return(expr) => {
            if let Some(expr) = expr {
                rewrite_expr(expr, generics);
            }
        }
    }
}

fn rewrite_expr(expr: &mut Expr, generics: &Generics) {
    match &mut expr.kind {
        ExprKind::Call { callee, arguments, .. } => {
            if let Some((enum_name, variant)) = callee.rsplit_once('.')
                && generics.enums.contains(enum_name)
            {
                let args = infer_args(arguments).expect("collected enum args");
                *callee = format!("{}.{}", mangle(enum_name, &args), variant);
            } else if generics.fns.contains(callee.as_str()) {
                let args = infer_args(arguments).expect("collected fn args");
                *callee = mangle(callee, &args);
            } else if generics.structs.contains(callee.as_str()) {
                let args = infer_args(arguments).expect("collected struct args");
                *callee = mangle(callee, &args);
            }
            for argument in arguments {
                rewrite_expr(argument, generics);
            }
        }
        ExprKind::Cast { value, ty } => {
            rewrite_expr(value, generics);
            rewrite_type_ref(ty, &union_types(generics));
        }
        ExprKind::Array(values) => {
            for value in values {
                rewrite_expr(value, generics);
            }
        }
        ExprKind::RepeatArray { value, .. } => rewrite_expr(value, generics),
        ExprKind::Index { array, index } => {
            rewrite_expr(array, generics);
            rewrite_expr(index, generics);
        }
        ExprKind::Unary { operand, .. } => rewrite_expr(operand, generics),
        ExprKind::Binary { left, right, .. } => {
            rewrite_expr(left, generics);
            rewrite_expr(right, generics);
        }
        ExprKind::Field { base, .. } => rewrite_expr(base, generics),
        ExprKind::AddressOf { operand }
        | ExprKind::Deref { operand }
        | ExprKind::PtrField { base: operand, .. } => rewrite_expr(operand, generics),
        ExprKind::Match { value, arms } => {
            rewrite_expr(value, generics);
            for arm in arms {
                rewrite_expr(&mut arm.body, generics);
            }
        }
        ExprKind::Number(_)
        | ExprKind::Character(_)
        | ExprKind::Boolean(_)
        | ExprKind::String(_)
        | ExprKind::Name(_) => {}
    }
}

// ── 实例化 ────────────────────────────────────────────────────────────────

fn instantiate_function(function: &ast::Function, params: &[String], args: &TypeArgs) -> ast::Function {
    let map = param_map(params, args);
    let mut concrete = function.clone();
    concrete.name = mangle(&function.name, args);
    concrete.type_params.clear();
    for param in &mut concrete.parameters {
        param.ty = substitute_type(&param.ty, &map);
    }
    if let Some(ret) = &mut concrete.return_type {
        *ret = substitute_type(ret, &map);
    }
    substitute_block(&mut concrete.body, &map);
    concrete
}

fn instantiate_struct(
    structure: &ast::StructDecl,
    params: &[String],
    args: &TypeArgs,
) -> ast::StructDecl {
    let map = param_map(params, args);
    let mut concrete = structure.clone();
    concrete.name = mangle(&structure.name, args);
    concrete.type_params.clear();
    for field in &mut concrete.fields {
        field.ty = substitute_type(&field.ty, &map);
    }
    concrete
}

fn instantiate_enum(
    enumeration: &ast::EnumDecl,
    params: &[String],
    args: &TypeArgs,
) -> ast::EnumDecl {
    let map = param_map(params, args);
    let mut concrete = enumeration.clone();
    concrete.name = mangle(&enumeration.name, args);
    concrete.type_params.clear();
    for variant in &mut concrete.variants {
        for field in &mut variant.fields {
            *field = substitute_type(field, &map);
        }
    }
    concrete
}

fn param_map(params: &[String], args: &TypeArgs) -> HashMap<String, TypeRef> {
    params.iter().cloned().zip(args.iter().cloned()).collect()
}

/// 替换类型引用中的类型参数。
fn substitute_type(ty: &TypeRef, map: &HashMap<String, TypeRef>) -> TypeRef {
    match &ty.kind {
        TypeRefKind::Name(name) => map.get(name).cloned().unwrap_or_else(|| ty.clone()),
        TypeRefKind::Array { element, length } => TypeRef {
            kind: TypeRefKind::Array {
                element: Box::new(substitute_type(element, map)),
                length: *length,
            },
            span: ty.span,
        },
        TypeRefKind::Slice { element } => TypeRef {
            kind: TypeRefKind::Slice {
                element: Box::new(substitute_type(element, map)),
            },
            span: ty.span,
        },
        TypeRefKind::Pointer { inner } => TypeRef {
            kind: TypeRefKind::Pointer {
                inner: Box::new(substitute_type(inner, map)),
            },
            span: ty.span,
        },
        TypeRefKind::Generic { name, args } => TypeRef {
            kind: TypeRefKind::Generic {
                name: name.clone(),
                args: args.iter().map(|a| substitute_type(a, map)).collect(),
            },
            span: ty.span,
        },
    }
}

fn substitute_block(block: &mut [Statement], map: &HashMap<String, TypeRef>) {
    for statement in block {
        substitute_statement(statement, map);
    }
}

fn substitute_statement(statement: &mut Statement, map: &HashMap<String, TypeRef>) {
    match &mut statement.kind {
        StatementKind::Variable {
            type_name,
            initializer,
            ..
        } => {
            if let Some(ty) = type_name {
                *ty = substitute_type(ty, map);
            }
            substitute_expr(initializer, map);
        }
        StatementKind::Assignment { value, .. } => substitute_expr(value, map),
        StatementKind::IndexAssignment { index, value, .. } => {
            substitute_expr(index, map);
            substitute_expr(value, map);
        }
        StatementKind::DerefAssignment { target, value, .. } => {
            substitute_expr(target, map);
            substitute_expr(value, map);
        }
        StatementKind::PtrFieldAssignment { base, value, .. } => {
            substitute_expr(base, map);
            substitute_expr(value, map);
        }
        StatementKind::Defer { value } => substitute_expr(value, map),
        StatementKind::Try { resources, body } => {
            for resource in resources {
                substitute_expr(&mut resource.initializer, map);
            }
            substitute_block(body, map);
        }
        StatementKind::Expression(expr) => substitute_expr(expr, map),
        StatementKind::If {
            condition,
            then_block,
            else_block,
        } => {
            substitute_expr(condition, map);
            substitute_block(then_block, map);
            if let Some(else_block) = else_block {
                substitute_block(else_block, map);
            }
        }
        StatementKind::Loop(body) => substitute_block(body, map),
        StatementKind::While { condition, body } => {
            substitute_expr(condition, map);
            substitute_block(body, map);
        }
        StatementKind::For {
            iterable, body, ..
        } => {
            match iterable {
                ast::ForIterable::Range { start, end, .. } => {
                    substitute_expr(start, map);
                    substitute_expr(end, map);
                }
                ast::ForIterable::Array(array) => substitute_expr(array, map),
            }
            substitute_block(body, map);
        }
        StatementKind::Break | StatementKind::Continue => {}
        StatementKind::Return(expr) => {
            if let Some(expr) = expr {
                substitute_expr(expr, map);
            }
        }
    }
}

fn substitute_expr(expr: &mut Expr, map: &HashMap<String, TypeRef>) {
    match &mut expr.kind {
        ExprKind::Call { arguments, .. } => {
            for argument in arguments {
                substitute_expr(argument, map);
            }
        }
        ExprKind::Array(values) => {
            for value in values {
                substitute_expr(value, map);
            }
        }
        ExprKind::RepeatArray { value, .. } => substitute_expr(value, map),
        ExprKind::Index { array, index } => {
            substitute_expr(array, map);
            substitute_expr(index, map);
        }
        ExprKind::Cast { value, ty } => {
            substitute_expr(value, map);
            *ty = substitute_type(ty, map);
        }
        ExprKind::Unary { operand, .. } => substitute_expr(operand, map),
        ExprKind::Binary { left, right, .. } => {
            substitute_expr(left, map);
            substitute_expr(right, map);
        }
        ExprKind::Field { base, .. } => substitute_expr(base, map),
        ExprKind::AddressOf { operand }
        | ExprKind::Deref { operand }
        | ExprKind::PtrField { base: operand, .. } => substitute_expr(operand, map),
        ExprKind::Match { value, arms } => {
            substitute_expr(value, map);
            for arm in arms {
                substitute_expr(&mut arm.body, map);
            }
        }
        ExprKind::Number(_)
        | ExprKind::Character(_)
        | ExprKind::Boolean(_)
        | ExprKind::String(_)
        | ExprKind::Name(_) => {}
    }
}

// ── 类型引用辅助 ──────────────────────────────────────────────────────────

/// 收集类型引用里的泛型实例请求。
fn collect_type_ref(
    ty: &TypeRef,
    generic_types: &HashSet<String>,
    type_instances: &mut HashSet<(String, TypeArgs)>,
) {
    match &ty.kind {
        TypeRefKind::Generic { name, args } => {
            if generic_types.contains(name) {
                type_instances.insert((name.clone(), args.clone()));
            }
            for arg in args {
                collect_type_ref(arg, generic_types, type_instances);
            }
        }
        TypeRefKind::Array { element, .. }
        | TypeRefKind::Slice { element }
        | TypeRefKind::Pointer { inner: element } => {
            collect_type_ref(element, generic_types, type_instances);
        }
        TypeRefKind::Name(_) => {}
    }
}

/// 把泛型类型引用重写为 mangling 名。
fn rewrite_type_ref(ty: &mut TypeRef, generic_types: &HashSet<String>) {
    match &mut ty.kind {
        TypeRefKind::Generic { name, args } => {
            for arg in args.iter_mut() {
                rewrite_type_ref(arg, generic_types);
            }
            if generic_types.contains(name.as_str()) {
                let mangled = mangle(name, args);
                ty.kind = TypeRefKind::Name(mangled);
            }
        }
        TypeRefKind::Array { element, .. }
        | TypeRefKind::Slice { element }
        | TypeRefKind::Pointer { inner: element } => {
            rewrite_type_ref(element, generic_types);
        }
        TypeRefKind::Name(_) => {}
    }
}

fn union_types(generics: &Generics) -> HashSet<String> {
    generics
        .structs
        .iter()
        .chain(generics.enums.iter())
        .cloned()
        .collect()
}

/// 稳定 mangling：`id<T=i32>` → `id$i32`。
fn mangle(name: &str, args: &TypeArgs) -> String {
    let mut out = String::from(name);
    for arg in args {
        out.push('$');
        out.push_str(&type_mangle_name(arg));
    }
    out
}

fn type_mangle_name(ty: &TypeRef) -> String {
    match &ty.kind {
        TypeRefKind::Name(name) => name.clone(),
        TypeRefKind::Slice { element } => format!("slice${}", type_mangle_name(element)),
        TypeRefKind::Pointer { inner } => format!("ptr${}", type_mangle_name(inner)),
        TypeRefKind::Array { element, length } => {
            format!("array${length}${}", type_mangle_name(element))
        }
        TypeRefKind::Generic { name, args } => {
            let mut out = name.clone();
            for arg in args {
                out.push('$');
                out.push_str(&type_mangle_name(arg));
            }
            out
        }
    }
}
