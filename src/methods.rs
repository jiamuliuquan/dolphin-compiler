//! M15 方法提升：把 `impl` 块的方法提升为带 `self` 第一参数的普通函数，
//! 并生成方法表供 lowering 阶段做方法调用 desugar（`x.foo()` → `Type.foo(x)`）。
//!
//! 方法函数名编码：固有方法 `Type.method`，契约方法 `Trait$Type.method`。

use std::collections::HashMap;

use crate::ast::{self, Expr, ExprKind, Statement, StatementKind, TypeRef, TypeRefKind};

/// 方法表：接收者类型名 → (方法名 → 方法函数名)。
#[derive(Clone, Default)]
pub struct MethodTable {
    methods: HashMap<String, HashMap<String, String>>,
}

impl MethodTable {
    pub fn lookup(&self, type_name: &str, method_name: &str) -> Option<&str> {
        self.methods
            .get(type_name)
            .and_then(|m| m.get(method_name))
            .map(|s| s.as_str())
    }
}

/// trait 实现表：(类型名, trait 名)。
pub type TraitImpls = std::collections::HashSet<(String, String)>;

/// 把 `program.impls` 的方法提升为普通函数，返回方法表与 trait 实现表。
pub fn resolve_methods(program: &mut ast::Program) -> (MethodTable, TraitImpls) {
    let mut table = MethodTable::default();
    let mut trait_impls = TraitImpls::new();
    let mut methods: Vec<ast::Function> = Vec::new();
    for impl_block in program.impls.drain(..) {
        let type_name = impl_block.type_name;
        let trait_name = impl_block.trait_name;
        let assoc_bindings = impl_block.assoc_bindings;
        if let Some(t) = &trait_name {
            trait_impls.insert((type_name.clone(), t.clone()));
        }
        for method in impl_block.methods {
            let method_name = method.name.clone();
            // 方法函数名：固有 `Type.method`，契约 `Trait$Type.method`。
            let fn_name = match &trait_name {
                Some(t) => format!("{t}${type_name}.{method_name}"),
                None => format!("{type_name}.{method_name}"),
            };
            let mut f = method;
            // 替换签名与函数体中的 `Self` 与 `Self::Item` 为实际类型。
            replace_self(&mut f, &type_name, &assoc_bindings);
            f.name = fn_name.clone();
            f.type_params.clear();
            methods.push(f);
            // 记录方法表：接收者类型名 → 方法名 → 方法函数名。
            table
                .methods
                .entry(type_name.clone())
                .or_default()
                .insert(method_name, fn_name);
        }
    }
    // 追加到普通函数列表（方法也是可调用的函数）。
    program.functions.append(&mut methods);
    (table, trait_impls)
}

/// 把函数签名与函数体中的 `Self` / `Self::Item` 类型引用替换为实际类型。
fn replace_self(function: &mut ast::Function, type_name: &str, assoc: &[(String, TypeRef)]) {
    for parameter in &mut function.parameters {
        parameter.ty = replace_type(&parameter.ty, type_name, assoc);
    }
    if let Some(ret) = &mut function.return_type {
        *ret = replace_type(ret, type_name, assoc);
    }
    replace_block(&mut function.body, type_name, assoc);
}

fn replace_type(ty: &TypeRef, type_name: &str, assoc: &[(String, TypeRef)]) -> TypeRef {
    match &ty.kind {
        TypeRefKind::Name(name) if name == "Self" => TypeRef {
            kind: TypeRefKind::Name(type_name.to_string()),
            span: ty.span,
        },
        TypeRefKind::SelfAssoc(assoc_name) => {
            // `Self::Item` → impl 绑定的具体类型。
            match assoc.iter().find(|(n, _)| n == assoc_name) {
                Some((_, bound)) => bound.clone(),
                None => ty.clone(),
            }
        }
        TypeRefKind::Name(_) => ty.clone(),
        TypeRefKind::Array { element, length } => TypeRef {
            kind: TypeRefKind::Array {
                element: Box::new(replace_type(element, type_name, assoc)),
                length: *length,
            },
            span: ty.span,
        },
        TypeRefKind::Slice { element } => TypeRef {
            kind: TypeRefKind::Slice {
                element: Box::new(replace_type(element, type_name, assoc)),
            },
            span: ty.span,
        },
        TypeRefKind::Pointer { inner } => TypeRef {
            kind: TypeRefKind::Pointer {
                inner: Box::new(replace_type(inner, type_name, assoc)),
            },
            span: ty.span,
        },
        TypeRefKind::Generic { name, args } => TypeRef {
            kind: TypeRefKind::Generic {
                name: name.clone(),
                args: args
                    .iter()
                    .map(|a| replace_type(a, type_name, assoc))
                    .collect(),
            },
            span: ty.span,
        },
    }
}

fn replace_block(block: &mut [Statement], type_name: &str, assoc: &[(String, TypeRef)]) {
    for statement in block {
        replace_statement(statement, type_name, assoc);
    }
}

fn replace_statement(statement: &mut Statement, type_name: &str, assoc: &[(String, TypeRef)]) {
    match &mut statement.kind {
        StatementKind::Variable {
            type_name: ty,
            initializer,
            ..
        } => {
            if let Some(ty) = ty {
                *ty = replace_type(ty, type_name, assoc);
            }
            replace_expr(initializer, type_name, assoc);
        }
        StatementKind::Assignment { value, .. } => replace_expr(value, type_name, assoc),
        StatementKind::IndexAssignment { index, value, .. } => {
            replace_expr(index, type_name, assoc);
            replace_expr(value, type_name, assoc);
        }
        StatementKind::DerefAssignment { target, value, .. } => {
            replace_expr(target, type_name, assoc);
            replace_expr(value, type_name, assoc);
        }
        StatementKind::PtrFieldAssignment { base, value, .. } => {
            replace_expr(base, type_name, assoc);
            replace_expr(value, type_name, assoc);
        }
        StatementKind::Defer { value } => replace_expr(value, type_name, assoc),
        StatementKind::Try { resources, body } => {
            for resource in resources {
                replace_expr(&mut resource.initializer, type_name, assoc);
            }
            replace_block(body, type_name, assoc);
        }
        StatementKind::Expression(expr) => replace_expr(expr, type_name, assoc),
        StatementKind::If {
            condition,
            then_block,
            else_block,
        } => {
            replace_expr(condition, type_name, assoc);
            replace_block(then_block, type_name, assoc);
            if let Some(else_block) = else_block {
                replace_block(else_block, type_name, assoc);
            }
        }
        StatementKind::Loop(body) => replace_block(body, type_name, assoc),
        StatementKind::While { condition, body } => {
            replace_expr(condition, type_name, assoc);
            replace_block(body, type_name, assoc);
        }
        StatementKind::For {
            iterable, body, ..
        } => {
            match iterable {
                ast::ForIterable::Range { start, end, .. } => {
                    replace_expr(start, type_name, assoc);
                    replace_expr(end, type_name, assoc);
                }
                ast::ForIterable::Array(array) => replace_expr(array, type_name, assoc),
            }
            replace_block(body, type_name, assoc);
        }
        StatementKind::Break | StatementKind::Continue => {}
        StatementKind::Return(expr) => {
            if let Some(expr) = expr {
                replace_expr(expr, type_name, assoc);
            }
        }
    }
}

fn replace_expr(expr: &mut Expr, type_name: &str, assoc: &[(String, TypeRef)]) {
    match &mut expr.kind {
        ExprKind::Call { arguments, .. } => {
            for argument in arguments {
                replace_expr(argument, type_name, assoc);
            }
        }
        ExprKind::Array(values) => {
            for value in values {
                replace_expr(value, type_name, assoc);
            }
        }
        ExprKind::RepeatArray { value, .. } => replace_expr(value, type_name, assoc),
        ExprKind::Index { array, index } => {
            replace_expr(array, type_name, assoc);
            replace_expr(index, type_name, assoc);
        }
        ExprKind::Cast { value, ty } => {
            replace_expr(value, type_name, assoc);
            *ty = replace_type(ty, type_name, assoc);
        }
        ExprKind::Unary { operand, .. } => replace_expr(operand, type_name, assoc),
        ExprKind::Binary { left, right, .. } => {
            replace_expr(left, type_name, assoc);
            replace_expr(right, type_name, assoc);
        }
        ExprKind::Field { base, .. } => replace_expr(base, type_name, assoc),
        ExprKind::AddressOf { operand }
        | ExprKind::Deref { operand }
        | ExprKind::PtrField { base: operand, .. } => replace_expr(operand, type_name, assoc),
        ExprKind::Match { value, arms } => {
            replace_expr(value, type_name, assoc);
            for arm in arms {
                replace_expr(&mut arm.body, type_name, assoc);
            }
        }
        ExprKind::Number(_)
        | ExprKind::Character(_)
        | ExprKind::Boolean(_)
        | ExprKind::String(_)
        | ExprKind::Name(_) => {}
    }
}
