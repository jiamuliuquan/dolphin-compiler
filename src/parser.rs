use crate::ast::{
    AssignmentOperator, BinaryOperator, Block, EnumDecl, Expr, ExprKind, FieldDecl, ForIterable,
    Function, ImplBlock, MatchArm, MatchPattern, MethodSignature, Parameter, PathRef, Program,
    Statement, StatementKind, StructDecl, TraitDecl, TryResource, TypeRef, TypeRefKind,
    UnaryOperator, VariantDecl,
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
        let mut traits = Vec::new();
        let mut impls = Vec::new();
        while !self.check(&TokenKind::Eof) {
            match self.current().kind {
                TokenKind::Fn => functions.push(self.parse_function()?),
                TokenKind::Struct => structs.push(self.parse_struct()?),
                TokenKind::Enum => enums.push(self.parse_enum()?),
                TokenKind::Trait => traits.push(self.parse_trait(false)?),
                TokenKind::Impl => impls.push(self.parse_impl()?),
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
                        TokenKind::Trait => {
                            let mut trait_decl = self.parse_trait(false)?;
                            trait_decl.public = public;
                            traits.push(trait_decl);
                        }
                        _ => {
                            return Err(Diagnostic::at(
                                self.source,
                                self.current().span,
                                "expected `fn`, `struct`, `enum`, or `trait` after `pub`",
                            ));
                        }
                    }
                }
                _ => {
                    return Err(Diagnostic::at(
                        self.source,
                        self.current().span,
                        "expected a function, struct, enum, trait, or impl",
                    ));
                }
            }
        }
        if functions.is_empty()
            && structs.is_empty()
            && enums.is_empty()
            && traits.is_empty()
            && impls.is_empty()
        {
            return Err(Diagnostic::at(
                self.source,
                self.current().span,
                "expected a function, struct, enum, trait, or impl",
            ));
        }
        Ok(Program {
            package,
            uses,
            functions,
            structs,
            enums,
            traits,
            impls,
        })
    }

    fn parse_function(&mut self) -> Result<Function, Diagnostic> {
        let public = self.consume(&TokenKind::Pub).is_some();
        let start = self.expect_simple(TokenKind::Fn, "expected `fn`")?;
        let (name, name_span) = self.expect_identifier("expected function name")?;
        let type_params = self.parse_type_params()?;
        self.expect_simple(TokenKind::LeftParen, "expected `(` after function name")?;
        let mut parameters = Vec::new();
        if !self.check(&TokenKind::RightParen) {
            loop {
                parameters.push(self.parse_parameter()?);
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
            type_params,
            parameters,
            return_type,
            body,
            span: start.merge(block_span),
        })
    }

    /// 解析可选的类型参数列表 `<T, U>`（M15）。
    fn parse_type_params(&mut self) -> Result<Vec<String>, Diagnostic> {
        if self.consume(&TokenKind::Less).is_none() {
            return Ok(Vec::new());
        }
        let mut params = Vec::new();
        if !self.check(&TokenKind::Greater) {
            loop {
                let (name, _span) = self.expect_identifier("expected type parameter name")?;
                params.push(name);
                if self.consume(&TokenKind::Comma).is_none() {
                    break;
                }
            }
        }
        self.expect_simple(TokenKind::Greater, "expected `>` after type parameters")?;
        Ok(params)
    }

    /// 前瞻探测：当前 `<` 之后是否匹配「类型 (`,` 类型)* `>` `(`」，
    /// 即是否构成 `foo<T>(...)` 泛型调用（而非 `a < b` 比较）。
    fn looks_like_generic_call(&self) -> bool {
        let mut i = self.position;
        if !matches!(self.tokens.get(i).map(|t| &t.kind), Some(TokenKind::Less)) {
            return false;
        }
        i += 1;
        loop {
            match self.tokens.get(i).map(|t| &t.kind) {
                Some(TokenKind::Identifier(_)) => i += 1,
                _ => return false,
            }
            match self.tokens.get(i).map(|t| &t.kind) {
                Some(TokenKind::Comma) => {
                    i += 1;
                    continue;
                }
                Some(TokenKind::Greater) => {
                    i += 1;
                    break;
                }
                _ => return false,
            }
        }
        matches!(self.tokens.get(i).map(|t| &t.kind), Some(TokenKind::LeftParen))
    }

    /// 解析单个参数。`self` 无类型标注时（值传递 self，§4.4）记为其隐式类型 `Self`。
    fn parse_parameter(&mut self) -> Result<Parameter, Diagnostic> {
        let (name, name_span) = self.expect_identifier("expected parameter name")?;
        let ty = if name == "self" && !self.check(&TokenKind::Colon) {
            // `self` 值传递：隐式类型为当前类型 `Self`（阶段 3 识别）。
            TypeRef {
                kind: TypeRefKind::Name("Self".to_string()),
                span: name_span,
            }
        } else {
            self.expect_simple(TokenKind::Colon, "expected `:` after parameter name")?;
            self.parse_type("expected parameter type")?
        };
        Ok(Parameter {
            name,
            name_span,
            ty,
        })
    }

    fn parse_struct(&mut self) -> Result<StructDecl, Diagnostic> {
        self.expect_simple(TokenKind::Struct, "expected `struct`")?;
        let (name, name_span) = self.expect_identifier("expected struct name")?;
        let type_params = self.parse_type_params()?;
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
            type_params,
            fields,
        })
    }

    fn parse_enum(&mut self) -> Result<EnumDecl, Diagnostic> {
        self.expect_simple(TokenKind::Enum, "expected `enum`")?;
        let (name, name_span) = self.expect_identifier("expected enum name")?;
        let type_params = self.parse_type_params()?;
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
            type_params,
            variants,
        })
    }

    /// 解析方法签名（不含函数体），用于 trait 声明（M15）。
    fn parse_method_signature(&mut self) -> Result<MethodSignature, Diagnostic> {
        self.expect_simple(TokenKind::Fn, "expected `fn`")?;
        let (name, name_span) = self.expect_identifier("expected method name")?;
        self.expect_simple(TokenKind::LeftParen, "expected `(` after method name")?;
        let mut parameters = Vec::new();
        if !self.check(&TokenKind::RightParen) {
            loop {
                parameters.push(self.parse_parameter()?);
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
        self.expect_simple(TokenKind::Semicolon, "expected `;` after method signature")?;
        Ok(MethodSignature {
            name,
            name_span,
            parameters,
            return_type,
        })
    }

    fn parse_trait(&mut self, public: bool) -> Result<TraitDecl, Diagnostic> {
        self.expect_simple(TokenKind::Trait, "expected `trait`")?;
        let (name, name_span) = self.expect_identifier("expected trait name")?;
        self.expect_simple(TokenKind::LeftBrace, "expected `{` after trait name")?;
        let mut methods = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.check(&TokenKind::Eof) {
            methods.push(self.parse_method_signature()?);
        }
        self.expect_simple(TokenKind::RightBrace, "expected `}` after trait body")?;
        Ok(TraitDecl {
            source_id: 0,
            public,
            name,
            name_span,
            methods,
        })
    }

    /// 解析 `impl` 块：`impl Type { ... }` 或 `impl Trait for Type { ... }`（M15）。
    fn parse_impl(&mut self) -> Result<ImplBlock, Diagnostic> {
        self.expect_simple(TokenKind::Impl, "expected `impl`")?;
        let (first, _first_span) = self.expect_identifier("expected type or trait name")?;
        let (trait_name, type_name) = if self.consume(&TokenKind::For).is_some() {
            let (type_name, _type_span) = self.expect_identifier("expected type name")?;
            (Some(first), type_name)
        } else {
            (None, first)
        };
        self.expect_simple(TokenKind::LeftBrace, "expected `{` after impl")?;
        let mut methods = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.check(&TokenKind::Eof) {
            methods.push(self.parse_function()?);
        }
        self.expect_simple(TokenKind::RightBrace, "expected `}` after impl body")?;
        Ok(ImplBlock {
            source_id: 0,
            trait_name,
            type_name,
            methods,
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
        if let Some(question) = self.consume(&TokenKind::Question) {
            // 可选类型语法糖 `?T`（M15），等价 `Option<T>`，见提案 §11.3。
            let inner = self.parse_type(message)?;
            let span = question.merge(inner.span);
            return Ok(TypeRef {
                kind: TypeRefKind::Generic {
                    name: "Option".to_string(),
                    args: vec![*Box::new(inner)],
                },
                span,
            });
        }
        let (name, span) = self.expect_identifier(message)?;
        // 泛型实例 `Vec<i32>`（M15）。
        if let Some(less) = self.consume(&TokenKind::Less) {
            let mut args = Vec::new();
            if !self.check(&TokenKind::Greater) {
                loop {
                    args.push(self.parse_type("expected type argument")?);
                    if self.consume(&TokenKind::Comma).is_none() {
                        break;
                    }
                }
            }
            let right = self.expect_simple(TokenKind::Greater, "expected `>` after type arguments")?;
            return Ok(TypeRef {
                kind: TypeRefKind::Generic { name, args },
                span: span.merge(right).merge(less),
            });
        }
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
                // 显式泛型实参 `foo<T>(...)`（M15，如 `allocate<T>(n)`）。
                // 仅当 `<` 后能匹配「类型列表 `>` `(`」时才是泛型调用，否则是普通比较。
                let mut type_args = None;
                if self.check(&TokenKind::Less) && self.looks_like_generic_call() {
                    self.advance();
                    let mut args = Vec::new();
                    if !self.check(&TokenKind::Greater) {
                        loop {
                            args.push(self.parse_type("expected type argument")?);
                            if self.consume(&TokenKind::Comma).is_none() {
                                break;
                            }
                        }
                    }
                    self.expect_simple(TokenKind::Greater, "expected `>` after type arguments")?;
                    type_args = Some(args);
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
                            type_args,
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

    #[test]
    fn parses_m15_generic_function() {
        let program = parse_text("fn id<T>(x: T): T { return x; }");
        assert_eq!(program.functions.len(), 1);
        assert_eq!(program.functions[0].type_params, ["T"]);
    }

    #[test]
    fn parses_m15_generic_struct_and_enum() {
        let program = parse_text(
            "struct Vec<T> { data: []T, len: i32 } enum Result<T, E> { Ok(T), Err(E) }",
        );
        assert_eq!(program.structs[0].type_params, ["T"]);
        assert_eq!(program.enums[0].type_params, ["T", "E"]);
    }

    #[test]
    fn parses_m15_generic_type_arguments() {
        let program = parse_text("fn main() { var v: Vec<i32> = Vec(); }");
        let type_name = &program.functions[0].body[0];
        let StatementKind::Variable { type_name: Some(ty), .. } = &type_name.kind else {
            panic!("expected variable with type annotation");
        };
        match &ty.kind {
            TypeRefKind::Generic { name, args } => {
                assert_eq!(name, "Vec");
                assert_eq!(args.len(), 1);
                assert!(matches!(args[0].kind, TypeRefKind::Name(_)));
            }
            other => panic!("expected generic type, got {other:?}"),
        }
    }

    #[test]
    fn parses_m15_trait_decl() {
        let program = parse_text("trait Shape { fn area(self): f64; fn scale(self: *Shape, f: f64); }");
        assert_eq!(program.traits.len(), 1);
        assert_eq!(program.traits[0].name, "Shape");
        assert_eq!(program.traits[0].methods.len(), 2);
        assert_eq!(program.traits[0].methods[0].name, "area");
    }

    #[test]
    fn parses_m15_impl_blocks() {
        let program = parse_text(
            "impl Circle { fn area(self): f64 { return 1.0; } } impl Shape for Circle { fn area(self): f64 { return 1.0; } }",
        );
        assert_eq!(program.impls.len(), 2);
        assert!(program.impls[0].trait_name.is_none());
        assert_eq!(program.impls[0].type_name, "Circle");
        assert_eq!(program.impls[1].trait_name.as_deref(), Some("Shape"));
        assert_eq!(program.impls[1].type_name, "Circle");
    }

    #[test]
    fn parses_m15_optional_type_sugar() {
        let program = parse_text("fn main() { var p: ?*i32 = 0; }");
        let StatementKind::Variable { type_name: Some(ty), .. } = &program.functions[0].body[0].kind
        else {
            panic!("expected variable with type annotation");
        };
        match &ty.kind {
            TypeRefKind::Generic { name, args } => {
                assert_eq!(name, "Option");
                assert_eq!(args.len(), 1);
                assert!(matches!(args[0].kind, TypeRefKind::Pointer { .. }));
            }
            other => panic!("expected Option sugar, got {other:?}"),
        }
    }
}
