use std::collections::HashMap;

use crate::ast::{
    self, AssignmentOperator, BinaryOperator, ExprKind, ForIterable, Statement, StatementKind,
    TypeRefKind, UnaryOperator,
};
use crate::diagnostic::Diagnostic;
use crate::ir::{
    self, BasicBlock, BlockId, Expr, FunctionId, Instruction, LocalId, PrintPart, Terminator, Type,
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
}

impl<'a> ProgramLowerer<'a> {
    fn new(sources: &'a [SourceFile], ast: &'a ast::Program) -> Self {
        Self {
            sources,
            ast,
            signatures: HashMap::new(),
            main: None,
        }
    }

    fn lower(mut self) -> Result<ir::Program, Diagnostic> {
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
                )
                .lower()?,
            );
        }
        Ok(ir::Program {
            functions,
            main: self.main.expect("signature collection checks main"),
        })
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
                parameters.push(resolve_type(source, &parameter.ty)?);
            }
            let is_main = function.name == "main";
            let return_type = match &function.return_type {
                Some(ty) => resolve_type(source, ty)?,
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
    parameters: Vec<LocalId>,
    locals: Vec<Type>,
    scopes: Vec<HashMap<String, Binding>>,
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
    ) -> Self {
        Self {
            source,
            function,
            signature,
            signatures,
            parameters: Vec::new(),
            locals: Vec::new(),
            scopes: vec![HashMap::new()],
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
                        let declared = resolve_type(self.source, ty)?;
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
                self.terminate(Terminator::Jump(targets.continue_block));
            }
            StatementKind::Return(value) => self.lower_return(value.as_ref(), statement.span)?,
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
        let Type::Array { element, length } = binding.ty else {
            return Err(Diagnostic::at(
                self.source,
                name_span,
                format!("cannot index value of type `{}`", binding.ty),
            ));
        };
        check_constant_index(self.source, index, length)?;
        let index = self.lower_expr(index)?;
        self.require_type(index.ty, Type::I32, name_span)?;
        let index_local = self.store_temporary(index);
        let right = self.lower_expr(value)?;
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
        &self,
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
        self.terminate(Terminator::Return(value));
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

    fn lower_expr(&self, expression: &ast::Expr) -> Result<Expr, Diagnostic> {
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
                let Type::Array { element, length } = array_value.ty else {
                    return Err(Diagnostic::at(
                        self.source,
                        array.span,
                        format!("cannot index value of type `{}`", array_value.ty),
                    ));
                };
                check_constant_index(self.source, index, length)?;
                let index_value = self.lower_expr(index)?;
                self.require_type(index_value.ty, Type::I32, index.span)?;
                Ok(Expr {
                    kind: ir::ExprKind::Index {
                        array: Box::new(array_value),
                        index: Box::new(index_value),
                    },
                    ty: element.as_type(),
                })
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
                let to = resolve_type(self.source, ty)?;
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
        }
    }

    fn lower_call(
        &self,
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
            self.require_type(value.ty, Type::String, arguments[0].span)?;
            return Ok(Expr {
                kind: ir::ExprKind::StringLength(Box::new(value)),
                ty: Type::I32,
            });
        }
        if matches!(callee, "print" | "println") {
            return Err(Diagnostic::at(
                self.source,
                callee_span,
                format!("`{callee}` can only be used as a statement"),
            ));
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

    fn lower_binary(
        &self,
        operator: BinaryOperator,
        left: &ast::Expr,
        right: &ast::Expr,
        span: Span,
    ) -> Result<Expr, Diagnostic> {
        let left = self.lower_expr(left)?;
        let right = self.lower_expr(right)?;
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
        let result = operation(self);
        self.scopes.pop();
        result
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

fn resolve_type(source: &SourceFile, type_ref: &ast::TypeRef) -> Result<Type, Diagnostic> {
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
            _ => Err(Diagnostic::at(
                source,
                type_ref.span,
                format!("unknown type `{name}`"),
            )),
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
            let element_type = resolve_type(source, element)?;
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
        let source = SourceFile::new(PathBuf::from("main.dc"), text.to_string());
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
}
