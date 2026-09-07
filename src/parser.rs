use crate::ast::{
    AssignmentOperator, BinaryOperator, Block, EnumDecl, Expr, ExprKind, FieldDecl, ForIterable,
    Function, MatchArm, MatchPattern, Parameter, PathRef, Program, Statement, StatementKind,
    StructDecl, TryResource, TypeRef, TypeRefKind, UnaryOperator, VariantDecl,
};
use crate::diagnostic::Diagnostic;
use crate::source::{SourceFile, Span};
use crate::token::{Token, TokenKind};

pub fn parse(source: &SourceFile, tokens: Vec<Token>) -> Result<Program, Diagnostic> {
    Parser::new(source, tokens).parse_program()
}

struct Parser<'a> {
    source: &'a SourceFile,
    tokens: Vec<Token>,
    position: usize,
}

impl<'a> Parser<'a> {
    fn new(source: &'a SourceFile, tokens: Vec<Token>) -> Self {
        Self {
            source,
            tokens,
            position: 0,
        }
    }

    fn parse_program(mut self) -> Result<Program, Diagnostic> {
        let package = if self.consume(&TokenKind::Pkg).is_some() {
            let path = self.parse_path("expected package path")?;
            self.expect_simple(
                TokenKind::Semicolon,
                "expected `;` after package declaration",
            )?;
            Some(path)
        } else {
            None
        };
        let mut uses = Vec::new();
        while self.consume(&TokenKind::Use).is_some() {
            uses.push(self.parse_path("expected import path")?);
            self.expect_simple(TokenKind::Semicolon, "expected `;` after import")?;
        }
        let mut functions = Vec::new();
        let mut structs = Vec::new();
        let mut enums = Vec::new();
        while !self.check(&TokenKind::Eof) {
            match self.current().kind {
                TokenKind::Fn => functions.push(self.parse_function()?),
                TokenKind::Struct => structs.push(self.parse_struct()?),
                TokenKind::Enum => enums.push(self.parse_enum()?),
                TokenKind::Pub => {
                    let public = self.consume(&TokenKind::Pub).is_some();
                    match self.current().kind {
                        TokenKind::Fn => {
                            let mut function = self.parse_function()?;
                            function.public = public;
                            functions.push(function);
                        }
                        TokenKind::Struct => {
                            let mut structure = self.parse_struct()?;
                            structure.public = public;
                            structs.push(structure);
                        }
                        TokenKind::Enum => {
                            let mut enumeration = self.parse_enum()?;
                            enumeration.public = public;
                            enums.push(enumeration);
                        }
                        _ => {
                            return Err(Diagnostic::at(
                                self.source,
                                self.current().span,
                                "expected `fn`, `struct`, or `enum` after `pub`",
                            ));
                        }
                    }
                }
                _ => {
                    return Err(Diagnostic::at(
                        self.source,
                        self.current().span,
                        "expected a function, struct, or enum",
                    ));
                }
            }
        }
        if functions.is_empty() && structs.is_empty() && enums.is_empty() {
            return Err(Diagnostic::at(
                self.source,
                self.current().span,
                "expected a function, struct, or enum",
            ));
        }
        Ok(Program {
            package,
            uses,
            functions,
            structs,
            enums,
        })
    }

    fn parse_function(&mut self) -> Result<Function, Diagnostic> {
        let public = self.consume(&TokenKind::Pub).is_some();
        let start = self.expect_simple(TokenKind::Fn, "expected `fn`")?;
        let (name, name_span) = self.expect_identifier("expected function name")?;
        self.expect_simple(TokenKind::LeftParen, "expected `(` after function name")?;
        let mut parameters = Vec::new();
        if !self.check(&TokenKind::RightParen) {
            loop {
                let (parameter_name, parameter_span) =
                    self.expect_identifier("expected parameter name")?;
                self.expect_simple(TokenKind::Colon, "expected `:` after parameter name")?;
                let ty = self.parse_type("expected parameter type")?;
                parameters.push(Parameter {
                    name: parameter_name,
                    name_span: parameter_span,
                    ty,
                });
                if self.consume(&TokenKind::Comma).is_none() {
                    break;
                }
            }
        }
        self.expect_simple(TokenKind::RightParen, "expected `)` after parameters")?;
        let return_type = if self.consume(&TokenKind::Colon).is_some() {
            Some(self.parse_type("expected return type")?)
        } else {
            None
        };
        let (block_span, body) = self.parse_block()?;
        Ok(Function {
            source_id: 0,
            public,
            name,
            name_span,
            parameters,
            return_type,
            body,
            span: start.merge(block_span),
        })
    }

    fn parse_struct(&mut self) -> Result<StructDecl, Diagnostic> {
        self.expect_simple(TokenKind::Struct, "expected `struct`")?;
        let (name, name_span) = self.expect_identifier("expected struct name")?;
        self.expect_simple(TokenKind::LeftBrace, "expected `{` after struct name")?;
        let mut fields = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.check(&TokenKind::Eof) {
            let (field_name, field_span) = self.expect_identifier("expected field name")?;
            self.expect_simple(TokenKind::Colon, "expected `:` after field name")?;
            let ty = self.parse_type("expected field type")?;
            fields.push(FieldDecl {
                name: field_name,
                name_span: field_span,
                ty,
            });
            // 最后一个字段的逗号可选。
            if self.consume(&TokenKind::Comma).is_none() {
                break;
            }
        }
        let right = self.expect_simple(TokenKind::RightBrace, "expected `}` after struct body")?;
        let _ = right;
        Ok(StructDecl {
            source_id: 0,
            public: false,
            name,
            name_span,
            fields,
        })
    }

    fn parse_enum(&mut self) -> Result<EnumDecl, Diagnostic> {
        self.expect_simple(TokenKind::Enum, "expected `enum`")?;
        let (name, name_span) = self.expect_identifier("expected enum name")?;
        self.expect_simple(TokenKind::LeftBrace, "expected `{` after enum name")?;
        let mut variants = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.check(&TokenKind::Eof) {
            let (variant_name, variant_span) = self.expect_identifier("expected variant name")?;
            let fields = if self.consume(&TokenKind::LeftParen).is_some() {
                let mut fields = Vec::new();
                if !self.check(&TokenKind::RightParen) {
                    loop {
                        fields.push(self.parse_type("expected variant field type")?);
                        if self.consume(&TokenKind::Comma).is_none() {
                            break;
                        }
                    }
                }
                self.expect_simple(TokenKind::RightParen, "expected `)` after variant fields")?;
                fields
            } else {
                Vec::new()
            };
            variants.push(VariantDecl {
                name: variant_name,
                name_span: variant_span,
                fields,
            });
            // 最后一个 variant 的逗号可选。
            if self.consume(&TokenKind::Comma).is_none() {
                break;
            }
        }
        self.expect_simple(TokenKind::RightBrace, "expected `}` after enum body")?;
        Ok(EnumDecl {
            source_id: 0,
            public: false,
            name,
            name_span,
            variants,
        })
    }

    fn parse_path(&mut self, message: &str) -> Result<PathRef, Diagnostic> {
        let (first, first_span) = self.expect_identifier(message)?;
        let mut segments = vec![first];
        let mut span = first_span;
        while self.consume(&TokenKind::Dot).is_some() {
            let (segment, segment_span) = self.expect_identifier("expected name after `.`")?;
            segments.push(segment);
            span = span.merge(segment_span);
        }
        Ok(PathRef { segments, span })
    }

    fn parse_type(&mut self, message: &str) -> Result<TypeRef, Diagnostic> {
        if let Some(left) = self.consume(&TokenKind::LeftBracket) {
            if self.consume(&TokenKind::RightBracket).is_some() {
                // 动态切片 `[]T`（M14）。
                let element = self.parse_type(message)?;
                let span = left.merge(element.span);
                return Ok(TypeRef {
                    kind: TypeRefKind::Slice {
                        element: Box::new(element),
                    },
                    span,
                });
            }
            let element = self.parse_type(message)?;
            self.expect_simple(TokenKind::Semicolon, "expected `;` in array type")?;
            let (length, length_span) = self.expect_array_length()?;
            let right =
                self.expect_simple(TokenKind::RightBracket, "expected `]` after array type")?;
            return Ok(TypeRef {
                kind: TypeRefKind::Array {
                    element: Box::new(element),
                    length,
                },
                span: left.merge(right).merge(length_span),
            });
        }
        if let Some(star) = self.consume(&TokenKind::Star) {
            // 显式指针 `*T`（M14）。
            let inner = self.parse_type(message)?;
            let span = star.merge(inner.span);
            return Ok(TypeRef {
                kind: TypeRefKind::Pointer {
                    inner: Box::new(inner),
                },
                span,
            });
        }
        let (name, span) = self.expect_identifier(message)?;
        Ok(TypeRef {
            kind: TypeRefKind::Name(name),
            span,
        })
    }

    fn parse_block(&mut self) -> Result<(Span, Block), Diagnostic> {
        let left = self.expect_simple(TokenKind::LeftBrace, "expected `{`")?;
        let mut statements = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.check(&TokenKind::Eof) {
            statements.push(self.parse_statement()?);
        }
        let right = self.expect_simple(TokenKind::RightBrace, "expected `}` after block")?;
        Ok((left.merge(right), statements))
    }

    fn parse_statement(&mut self) -> Result<Statement, Diagnostic> {
        let start = self.current().span;
        let kind = match self.current().kind {
            TokenKind::Var | TokenKind::Val => self.parse_variable()?,
            TokenKind::If => self.parse_if()?,
            TokenKind::Loop => {
                self.advance();
                let (_, block) = self.parse_block()?;
                StatementKind::Loop(block)
            }
            TokenKind::While => self.parse_while()?,
            TokenKind::For => self.parse_for()?,
            TokenKind::Break => {
                self.advance();
                self.expect_simple(TokenKind::Semicolon, "expected `;` after `break`")?;
                StatementKind::Break
            }
            TokenKind::Continue => {
                self.advance();
                self.expect_simple(TokenKind::Semicolon, "expected `;` after `continue`")?;
                StatementKind::Continue
            }
            TokenKind::Return => self.parse_return()?,
            TokenKind::Defer => self.parse_defer()?,
            TokenKind::Try => self.parse_try()?,
            TokenKind::Star => self.parse_deref_assignment()?,
            TokenKind::Identifier(_) if self.is_ptr_field_assignment() => {
                self.parse_ptr_field_assignment()?
            }
            TokenKind::Identifier(_) if self.is_index_assignment() => {
                self.parse_index_assignment()?
            }
            TokenKind::Identifier(_) if self.is_assignment() => self.parse_assignment()?,
            _ => {
                let expression = self.parse_expression(0)?;
                self.expect_simple(TokenKind::Semicolon, "expected `;` after expression")?;
                StatementKind::Expression(expression)
            }
        };
        let end = self.previous().span;
        Ok(Statement {
            kind,
            span: start.merge(end),
        })
    }

    fn parse_variable(&mut self) -> Result<StatementKind, Diagnostic> {
        let mutable = matches!(self.advance().kind, TokenKind::Var);
        let (name, name_span) = self.expect_identifier("expected variable name")?;
        let type_name = if self.consume(&TokenKind::Colon).is_some() {
            Some(self.parse_type("expected type name after `:`")?)
        } else {
            None
        };
        self.expect_simple(TokenKind::Equal, "variables must have an initializer")?;
        let initializer = self.parse_expression(0)?;
        self.expect_simple(
            TokenKind::Semicolon,
            "expected `;` after variable declaration",
        )?;
        Ok(StatementKind::Variable {
            mutable,
            name,
            name_span,
            type_name,
            initializer,
        })
    }

    fn parse_assignment(&mut self) -> Result<StatementKind, Diagnostic> {
        let (name, name_span) = self.expect_identifier("expected variable name")?;
        let operator = match self.advance().kind {
            TokenKind::Equal => AssignmentOperator::Assign,
            TokenKind::PlusEqual => AssignmentOperator::Add,
            TokenKind::MinusEqual => AssignmentOperator::Subtract,
            TokenKind::StarEqual => AssignmentOperator::Multiply,
            TokenKind::SlashEqual => AssignmentOperator::Divide,
            TokenKind::PercentEqual => AssignmentOperator::Remainder,
            _ => unreachable!("assignment lookahead checked the operator"),
        };
        let value = self.parse_expression(0)?;
        self.expect_simple(TokenKind::Semicolon, "expected `;` after assignment")?;
        Ok(StatementKind::Assignment {
            name,
            name_span,
            operator,
            value,
        })
    }

    fn parse_index_assignment(&mut self) -> Result<StatementKind, Diagnostic> {
        let (name, name_span) = self.expect_identifier("expected variable name")?;
        self.expect_simple(TokenKind::LeftBracket, "expected `[` after variable name")?;
        let index = self.parse_expression(0)?;
        self.expect_simple(TokenKind::RightBracket, "expected `]` after array index")?;
        let operator = self.parse_assignment_operator();
        let value = self.parse_expression(0)?;
        self.expect_simple(TokenKind::Semicolon, "expected `;` after assignment")?;
        Ok(StatementKind::IndexAssignment {
            name,
            name_span,
            index,
            operator,
            value,
        })
    }

    fn parse_if(&mut self) -> Result<StatementKind, Diagnostic> {
        self.advance();
        let condition = self.parse_expression(0)?;
        let (_, then_block) = self.parse_block()?;
        let else_block = if self.consume(&TokenKind::Else).is_some() {
            let (_, block) = self.parse_block()?;
            Some(block)
        } else {
            None
        };
        Ok(StatementKind::If {
            condition,
            then_block,
            else_block,
        })
    }

    fn parse_while(&mut self) -> Result<StatementKind, Diagnostic> {
        self.advance();
        let condition = self.parse_expression(0)?;
        let (_, body) = self.parse_block()?;
        Ok(StatementKind::While { condition, body })
    }

    fn parse_for(&mut self) -> Result<StatementKind, Diagnostic> {
        self.advance();
        let (name, name_span) = self.expect_identifier("expected loop variable after `for`")?;
        self.expect_simple(TokenKind::In, "expected `in` after loop variable")?;
        let start = self.parse_expression(0)?;
        let iterable = if self.check(&TokenKind::DotDot) || self.check(&TokenKind::DotDotEqual) {
            let inclusive = self.check(&TokenKind::DotDotEqual);
            self.advance();
            let end = self.parse_expression(0)?;
            ForIterable::Range {
                start,
                end,
                inclusive,
            }
        } else {
            ForIterable::Array(start)
        };
        let (_, body) = self.parse_block()?;
        Ok(StatementKind::For {
            name,
            name_span,
            iterable,
            body,
        })
    }

    fn parse_return(&mut self) -> Result<StatementKind, Diagnostic> {
        self.advance();
        let value = if self.check(&TokenKind::Semicolon) {
            None
        } else {
            Some(self.parse_expression(0)?)
        };
        self.expect_simple(TokenKind::Semicolon, "expected `;` after `return`")?;
        Ok(StatementKind::Return(value))
    }

    fn parse_defer(&mut self) -> Result<StatementKind, Diagnostic> {
        self.advance();
        let value = self.parse_expression(0)?;
        self.expect_simple(
            TokenKind::Semicolon,
            "expected `;` after `defer` expression",
        )?;
        Ok(StatementKind::Defer { value })
    }

    fn parse_try(&mut self) -> Result<StatementKind, Diagnostic> {
        self.advance();
        self.expect_simple(TokenKind::LeftParen, "expected `(` after `try`")?;
        let mut resources = Vec::new();
        if !self.check(&TokenKind::RightParen) {
            loop {
                if !matches!(self.advance().kind, TokenKind::Var) {
                    return Err(Diagnostic::at(
                        self.source,
                        self.previous().span,
                        "expected `var` in `try` resource declaration",
                    ));
                }
                let (name, name_span) = self.expect_identifier("expected resource name")?;
                self.expect_simple(TokenKind::Equal, "expected `=` in `try` resource")?;
                let initializer = self.parse_expression(0)?;
                resources.push(TryResource {
                    name,
                    name_span,
                    initializer,
                });
                if self.consume(&TokenKind::Comma).is_none() {
                    break;
                }
            }
        }
        self.expect_simple(TokenKind::RightParen, "expected `)` after `try` resources")?;
        let (_, body) = self.parse_block()?;
        Ok(StatementKind::Try { resources, body })
    }

    fn parse_deref_assignment(&mut self) -> Result<StatementKind, Diagnostic> {
        self.advance(); // consume `*`
        let target = self.parse_expression(70)?;
        let operator = self.parse_assignment_operator();
        let value = self.parse_expression(0)?;
        self.expect_simple(TokenKind::Semicolon, "expected `;` after assignment")?;
        Ok(StatementKind::DerefAssignment {
            target,
            operator,
            value,
        })
    }

    fn parse_ptr_field_assignment(&mut self) -> Result<StatementKind, Diagnostic> {
        // 首版指针字段赋值的 base 为变量名（指针存放在局部变量中）。
        let (base_name, base_span) = self.expect_identifier("expected pointer variable")?;
        let base = Expr {
            kind: ExprKind::Name(base_name),
            span: base_span,
        };
        self.expect_simple(
            TokenKind::Arrow,
            "expected `->` in pointer field assignment",
        )?;
        let (field, field_span) = self.expect_identifier("expected field name after `->`")?;
        let operator = self.parse_assignment_operator();
        let value = self.parse_expression(0)?;
        self.expect_simple(TokenKind::Semicolon, "expected `;` after assignment")?;
        Ok(StatementKind::PtrFieldAssignment {
            base,
            field,
            field_span,
            operator,
            value,
        })
    }

    fn parse_expression(&mut self, minimum_precedence: u8) -> Result<Expr, Diagnostic> {
        let mut left = self.parse_prefix()?;
        loop {
            if self.check(&TokenKind::As) && 65 >= minimum_precedence {
                self.advance();
                let ty = self.parse_type("expected type after `as`")?;
                let span = left.span.merge(ty.span);
                left = Expr {
                    kind: ExprKind::Cast {
                        value: Box::new(left),
                        ty,
                    },
                    span,
                };
                continue;
            }
            let Some((operator, precedence)) = self.binary_operator() else {
                break;
            };
            if precedence < minimum_precedence {
                break;
            }
            self.advance();
            let right = self.parse_expression(precedence + 1)?;
            let span = left.span.merge(right.span);
            left = Expr {
                kind: ExprKind::Binary {
                    operator,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            };
        }
        Ok(left)
    }

    fn parse_prefix(&mut self) -> Result<Expr, Diagnostic> {
        let token = self.advance().clone();
        let mut expression = match token.kind {
            TokenKind::Number(value) => Expr {
                kind: ExprKind::Number(value),
                span: token.span,
            },
            TokenKind::Character(value) => Expr {
                kind: ExprKind::Character(value),
                span: token.span,
            },
            TokenKind::String(value) => Expr {
                kind: ExprKind::String(value),
                span: token.span,
            },
            TokenKind::True | TokenKind::False => Expr {
                kind: ExprKind::Boolean(matches!(token.kind, TokenKind::True)),
                span: token.span,
            },
            TokenKind::LeftBracket => self.parse_array_literal(token.span)?,
            TokenKind::Match => self.parse_match(token.span)?,
            TokenKind::Identifier(name) => {
                // 收集点号路径段（可能用于函数调用、构造或字段访问）。
                let mut segments = vec![name];
                while self.consume(&TokenKind::Dot).is_some() {
                    let (segment, _) = self.expect_identifier("expected name after `.`")?;
                    segments.push(segment);
                }
                if self.check(&TokenKind::LeftParen) {
                    // 带括号：函数调用或结构体/枚举构造，交由 lower 按名称区分。
                    let path = segments.join(".");
                    self.advance();
                    let mut arguments = Vec::new();
                    if !self.check(&TokenKind::RightParen) {
                        loop {
                            arguments.push(self.parse_expression(0)?);
                            if self.consume(&TokenKind::Comma).is_none() {
                                break;
                            }
                        }
                    }
                    let right = self.expect_simple(
                        TokenKind::RightParen,
                        "expected `)` after function arguments",
                    )?;
                    Expr {
                        kind: ExprKind::Call {
                            callee: path,
                            callee_span: token.span,
                            arguments,
                        },
                        span: token.span.merge(right),
                    }
                } else if segments.len() == 1 {
                    Expr {
                        kind: ExprKind::Name(segments.into_iter().next().unwrap()),
                        span: token.span,
                    }
                } else {
                    // 无括号的点号链：字段访问。第一个段是名字，其余段是 Field 后缀。
                    let mut expression = Expr {
                        kind: ExprKind::Name(segments[0].clone()),
                        span: token.span,
                    };
                    for field in &segments[1..] {
                        expression = Expr {
                            kind: ExprKind::Field {
                                base: Box::new(expression),
                                field: field.clone(),
                                field_span: token.span,
                            },
                            span: token.span,
                        };
                    }
                    expression
                }
            }
            TokenKind::Minus | TokenKind::Bang => {
                let operator = if matches!(token.kind, TokenKind::Minus) {
                    UnaryOperator::Negate
                } else {
                    UnaryOperator::Not
                };
                let operand = self.parse_expression(70)?;
                let span = token.span.merge(operand.span);
                Expr {
                    kind: ExprKind::Unary {
                        operator,
                        operand: Box::new(operand),
                    },
                    span,
                }
            }
            TokenKind::Amper => {
                // 取址 `&e`（M14）：产生 `*T`。
                let operand = self.parse_expression(70)?;
                let span = token.span.merge(operand.span);
                Expr {
                    kind: ExprKind::AddressOf {
                        operand: Box::new(operand),
                    },
                    span,
                }
            }
            TokenKind::Star => {
                // 解引用 `*e`（M14）：读取指针指向的值。
                let operand = self.parse_expression(70)?;
                let span = token.span.merge(operand.span);
                Expr {
                    kind: ExprKind::Deref {
                        operand: Box::new(operand),
                    },
                    span,
                }
            }
            TokenKind::LeftParen => {
                let expression = self.parse_expression(0)?;
                let right =
                    self.expect_simple(TokenKind::RightParen, "expected `)` after expression")?;
                Expr {
                    span: token.span.merge(right),
                    ..expression
                }
            }
            _ => {
                return Err(Diagnostic::at(
                    self.source,
                    token.span,
                    "expected an expression",
                ));
            }
        };
        // 后缀：数组下标 `[i]` 与字段访问 `.field`。
        loop {
            if self.consume(&TokenKind::LeftBracket).is_some() {
                let index = self.parse_expression(0)?;
                let right =
                    self.expect_simple(TokenKind::RightBracket, "expected `]` after array index")?;
                let span = expression.span.merge(right);
                expression = Expr {
                    kind: ExprKind::Index {
                        array: Box::new(expression),
                        index: Box::new(index),
                    },
                    span,
                };
            } else if self.consume(&TokenKind::Dot).is_some() {
                let (field, field_span) =
                    self.expect_identifier("expected field name after `.`")?;
                let span = expression.span.merge(field_span);
                expression = Expr {
                    kind: ExprKind::Field {
                        base: Box::new(expression),
                        field,
                        field_span,
                    },
                    span,
                };
            } else if self.consume(&TokenKind::Arrow).is_some() {
                let (field, field_span) =
                    self.expect_identifier("expected field name after `->`")?;
                let span = expression.span.merge(field_span);
                expression = Expr {
                    kind: ExprKind::PtrField {
                        base: Box::new(expression),
                        field,
                        field_span,
                    },
                    span,
                };
            } else {
                break;
            }
        }
        Ok(expression)
    }

    fn parse_match(&mut self, start: Span) -> Result<Expr, Diagnostic> {
        let value = self.parse_expression(0)?;
        self.expect_simple(TokenKind::LeftBrace, "expected `{` after match value")?;
        let mut arms = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.check(&TokenKind::Eof) {
            let pattern = self.parse_match_pattern()?;
            self.expect_simple(TokenKind::FatArrow, "expected `=>` after match pattern")?;
            let body = self.parse_expression(0)?;
            arms.push(MatchArm { pattern, body });
            // 最后一个 arm 的逗号可选。
            if self.consume(&TokenKind::Comma).is_none() {
                break;
            }
        }
        let right = self.expect_simple(TokenKind::RightBrace, "expected `}` after match arms")?;
        Ok(Expr {
            kind: ExprKind::Match {
                value: Box::new(value),
                arms,
            },
            span: start.merge(right),
        })
    }

    fn parse_match_pattern(&mut self) -> Result<MatchPattern, Diagnostic> {
        if self.consume(&TokenKind::Underscore).is_some() {
            return Ok(MatchPattern::Wildcard);
        }
        let (name, name_span) = self.expect_identifier("expected match pattern")?;
        let mut path = name;
        while self.consume(&TokenKind::Dot).is_some() {
            let (segment, _) = self.expect_identifier("expected name after `.`")?;
            path.push('.');
            path.push_str(&segment);
        }
        let bindings = if self.consume(&TokenKind::LeftParen).is_some() {
            let mut bindings = Vec::new();
            if !self.check(&TokenKind::RightParen) {
                loop {
                    // 绑定名可以是标识符，或 `_` 表示忽略该字段。
                    if self.consume(&TokenKind::Underscore).is_some() {
                        bindings.push("_".to_string());
                    } else {
                        let (binding, _) = self.expect_identifier("expected binding name")?;
                        bindings.push(binding);
                    }
                    if self.consume(&TokenKind::Comma).is_none() {
                        break;
                    }
                }
            }
            self.expect_simple(TokenKind::RightParen, "expected `)` after pattern bindings")?;
            bindings
        } else {
            Vec::new()
        };
        Ok(MatchPattern::Enum {
            name: path,
            name_span,
            bindings,
        })
    }

    fn parse_array_literal(&mut self, left: Span) -> Result<Expr, Diagnostic> {
        if let Some(right) = self.consume(&TokenKind::RightBracket) {
            return Ok(Expr {
                kind: ExprKind::Array(Vec::new()),
                span: left.merge(right),
            });
        }
        let first = self.parse_expression(0)?;
        if self.consume(&TokenKind::Semicolon).is_some() {
            let (length, _) = self.expect_array_length()?;
            let right =
                self.expect_simple(TokenKind::RightBracket, "expected `]` after array literal")?;
            return Ok(Expr {
                kind: ExprKind::RepeatArray {
                    value: Box::new(first),
                    length,
                },
                span: left.merge(right),
            });
        }
        let mut values = vec![first];
        while self.consume(&TokenKind::Comma).is_some() {
            if self.check(&TokenKind::RightBracket) {
                break;
            }
            values.push(self.parse_expression(0)?);
        }
        let right =
            self.expect_simple(TokenKind::RightBracket, "expected `]` after array literal")?;
        Ok(Expr {
            kind: ExprKind::Array(values),
            span: left.merge(right),
        })
    }

    fn binary_operator(&self) -> Option<(BinaryOperator, u8)> {
        match self.current().kind {
            TokenKind::OrOr => Some((BinaryOperator::Or, 10)),
            TokenKind::AndAnd => Some((BinaryOperator::And, 20)),
            TokenKind::EqualEqual => Some((BinaryOperator::Equal, 30)),
            TokenKind::BangEqual => Some((BinaryOperator::NotEqual, 30)),
            TokenKind::Less => Some((BinaryOperator::Less, 40)),
            TokenKind::LessEqual => Some((BinaryOperator::LessEqual, 40)),
            TokenKind::Greater => Some((BinaryOperator::Greater, 40)),
            TokenKind::GreaterEqual => Some((BinaryOperator::GreaterEqual, 40)),
            TokenKind::Plus => Some((BinaryOperator::Add, 50)),
            TokenKind::Minus => Some((BinaryOperator::Subtract, 50)),
            TokenKind::Star => Some((BinaryOperator::Multiply, 60)),
            TokenKind::Slash => Some((BinaryOperator::Divide, 60)),
            TokenKind::Percent => Some((BinaryOperator::Remainder, 60)),
            _ => None,
        }
    }

    fn is_assignment(&self) -> bool {
        matches!(
            self.tokens.get(self.position + 1).map(|token| &token.kind),
            Some(
                TokenKind::Equal
                    | TokenKind::PlusEqual
                    | TokenKind::MinusEqual
                    | TokenKind::StarEqual
                    | TokenKind::SlashEqual
                    | TokenKind::PercentEqual
            )
        )
    }

    fn is_ptr_field_assignment(&self) -> bool {
        matches!(
            self.tokens.get(self.position + 1).map(|token| &token.kind),
            Some(TokenKind::Arrow)
        )
    }

    fn is_index_assignment(&self) -> bool {
        if !matches!(
            self.tokens.get(self.position + 1).map(|token| &token.kind),
            Some(TokenKind::LeftBracket)
        ) {
            return false;
        }
        let mut depth = 0usize;
        for (offset, token) in self.tokens.iter().skip(self.position + 1).enumerate() {
            match token.kind {
                TokenKind::LeftBracket => depth += 1,
                TokenKind::RightBracket => {
                    depth -= 1;
                    if depth == 0 {
                        let next = self.tokens.get(self.position + offset + 2);
                        return next.is_some_and(|next| {
                            matches!(
                                next.kind,
                                TokenKind::Equal
                                    | TokenKind::PlusEqual
                                    | TokenKind::MinusEqual
                                    | TokenKind::StarEqual
                                    | TokenKind::SlashEqual
                                    | TokenKind::PercentEqual
                            )
                        });
                    }
                }
                _ => {}
            }
        }
        false
    }

    fn parse_assignment_operator(&mut self) -> AssignmentOperator {
        match self.advance().kind {
            TokenKind::Equal => AssignmentOperator::Assign,
            TokenKind::PlusEqual => AssignmentOperator::Add,
            TokenKind::MinusEqual => AssignmentOperator::Subtract,
            TokenKind::StarEqual => AssignmentOperator::Multiply,
            TokenKind::SlashEqual => AssignmentOperator::Divide,
            TokenKind::PercentEqual => AssignmentOperator::Remainder,
            _ => unreachable!("assignment lookahead checked the operator"),
        }
    }

    fn expect_array_length(&mut self) -> Result<(usize, Span), Diagnostic> {
        let token = self.advance().clone();
        let TokenKind::Number(value) = token.kind else {
            return Err(Diagnostic::at(
                self.source,
                token.span,
                "expected array length",
            ));
        };
        if !value.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(Diagnostic::at(
                self.source,
                token.span,
                "expected array length",
            ));
        }
        let length = value
            .parse::<usize>()
            .map_err(|_| Diagnostic::at(self.source, token.span, "array length is too large"))?;
        Ok((length, token.span))
    }

    fn expect_identifier(&mut self, message: &str) -> Result<(String, Span), Diagnostic> {
        let token = self.advance().clone();
        match token.kind {
            TokenKind::Identifier(name) => Ok((name, token.span)),
            _ => Err(Diagnostic::at(self.source, token.span, message)),
        }
    }

    fn expect_simple(&mut self, expected: TokenKind, message: &str) -> Result<Span, Diagnostic> {
        self.consume(&expected)
            .ok_or_else(|| Diagnostic::at(self.source, self.current().span, message))
    }

    fn consume(&mut self, expected: &TokenKind) -> Option<Span> {
        self.check(expected).then(|| self.advance().span)
    }

    fn check(&self, expected: &TokenKind) -> bool {
        std::mem::discriminant(&self.current().kind) == std::mem::discriminant(expected)
    }

    fn current(&self) -> &Token {
        &self.tokens[self.position]
    }

    fn previous(&self) -> &Token {
        &self.tokens[self.position - 1]
    }

    fn advance(&mut self) -> &Token {
        let token = &self.tokens[self.position];
        if !matches!(token.kind, TokenKind::Eof) {
            self.position += 1;
        }
        token
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::lexer;

    use super::*;

    fn parse_text(text: &str) -> Program {
        let source = SourceFile::new(PathBuf::from("main.do"), text.to_string());
        let tokens = lexer::lex(&source).expect("lexing should succeed");
        parse(&source, tokens).expect("parsing should succeed")
    }

    #[test]
    fn parses_functions_calls_and_control_flow() {
        let program = parse_text(
            "fn add(a: i32, b: i32): i32 { return a + b; } fn main() { var i = add(1, 2); while i < 3 { i += 1; } return i; }",
        );
        assert_eq!(program.functions.len(), 2);
        assert_eq!(program.functions[0].parameters.len(), 2);
    }

    #[test]
    fn parses_print_expression_statement() {
        let program = parse_text("fn main() { println(\"value = {}\", 42); }");
        assert_eq!(program.functions[0].body.len(), 1);
    }

    #[test]
    fn parses_m6_arrays_and_for_loops() {
        let program = parse_text(
            "fn copy(values: [i32; 2]): [i32; 2] { return values; } fn main() { var values = [1, 2]; values[0] += 3; for value in values { println(\"{}\", value); } for i in 0..=2 { println(\"{}\", i); } }",
        );
        assert_eq!(program.functions.len(), 2);
        assert_eq!(program.functions[1].body.len(), 4);
    }

    #[test]
    fn parses_m7_package_imports_and_public_functions() {
        let program = parse_text(
            "pkg std.math; use values.limit; pub fn min(a: i32, b: i32): i32 { return limit(a, b); }",
        );
        assert_eq!(program.package.unwrap().segments, ["std", "math"]);
        assert_eq!(program.uses[0].segments, ["values", "limit"]);
        assert!(program.functions[0].public);
    }
}
