use crate::ast::{
    AssignmentOperator, AssociatedTypeBinding, AssociatedTypeDecl, BinaryOperator, Block, EnumDecl,
    Expr, ExprKind, FieldDecl, ForIterable, Function, ImplBlock, MatchArm, MatchPattern, Parameter,
    PathRef, Program, Statement, StatementKind, StructDecl, TraitDecl, TypeParamDecl, TypeRef,
    TypeRefKind, UnaryOperator, VariantDecl,
};
use dolphin_source::diagnostic::Diagnostic;
use dolphin_source::source::{SourceFile, Span};
use dolphin_source::token::{Token, TokenKind};

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
                TokenKind::Trait => traits.push(self.parse_trait()?),
                TokenKind::Impl => impls.push(self.parse_impl()?),
                TokenKind::Extern => {
                    if self.check_extern_struct() {
                        self.advance();
                        let mut structure = self.parse_struct()?;
                        structure.extern_c = true;
                        structs.push(structure);
                    } else {
                        self.parse_extern_block(&mut functions)?;
                    }
                }
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
                            let mut item = self.parse_trait()?;
                            item.public = public;
                            traits.push(item);
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
                        "expected a function, struct, or enum",
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

    /// 解析 `trait Name<T> { type Item; fn method(self: *Self): T; }`。
    fn parse_trait(&mut self) -> Result<TraitDecl, Diagnostic> {
        self.expect_simple(TokenKind::Trait, "expected `trait`")?;
        let (name, name_span) = self.expect_identifier("expected trait name")?;
        let type_params = self.parse_type_params()?;
        self.expect_simple(TokenKind::LeftBrace, "expected `{` after trait name")?;
        let mut associated_types = Vec::new();
        let mut methods = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.check(&TokenKind::Eof) {
            if self.check_type_keyword() {
                self.advance();
                let (assoc, assoc_span) =
                    self.expect_identifier("expected associated type name")?;
                self.expect_simple(
                    TokenKind::Semicolon,
                    "expected `;` after associated type declaration",
                )?;
                associated_types.push(AssociatedTypeDecl {
                    name: assoc,
                    name_span: assoc_span,
                });
                continue;
            }
            let public = self.consume(&TokenKind::Pub).is_some();
            let (start, method_name, method_span, method_params, parameters, return_type) =
                self.parse_function_parts()?;
            let end = self.expect_simple(
                TokenKind::Semicolon,
                "trait method must end with `;` (no default body yet)",
            )?;
            methods.push(Function {
                source_id: 0,
                public,
                name: method_name,
                name_span: method_span,
                type_params: method_params,
                parameters,
                return_type,
                body: Vec::new(),
                span: start.merge(end),
                extern_c: false,
                link_name: None,
            });
        }
        self.expect_simple(TokenKind::RightBrace, "expected `}` after trait body")?;
        Ok(TraitDecl {
            source_id: 0,
            public: false,
            name,
            name_span,
            type_params,
            associated_types,
            methods,
        })
    }

    /// 解析 `impl Type { ... }` 或 `impl Trait for Type { ... }`。
    fn parse_impl(&mut self) -> Result<ImplBlock, Diagnostic> {
        self.expect_simple(TokenKind::Impl, "expected `impl`")?;
        let type_params = self.parse_type_params()?;
        let (first, first_span) = self.expect_identifier("expected type or trait name")?;
        let first_arguments = self.parse_optional_type_arguments();
        let (trait_name, trait_span, trait_arguments, type_name, type_span, type_arguments) =
            if self.consume(&TokenKind::For).is_some() {
                let (target, target_span) =
                    self.expect_identifier("expected type name after `for`")?;
                let target_arguments = self.parse_optional_type_arguments();
                (
                    Some(first),
                    first_span,
                    first_arguments,
                    target,
                    target_span,
                    target_arguments,
                )
            } else {
                (
                    None,
                    first_span,
                    Vec::new(),
                    first,
                    first_span,
                    first_arguments,
                )
            };
        self.expect_simple(TokenKind::LeftBrace, "expected `{` after impl header")?;
        let mut associated_types = Vec::new();
        let mut methods = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.check(&TokenKind::Eof) {
            if self.check_type_keyword() {
                self.advance();
                let (assoc, assoc_span) =
                    self.expect_identifier("expected associated type name")?;
                self.expect_simple(TokenKind::Equal, "expected `=` after associated type name")?;
                let ty = self.parse_type("expected associated type binding")?;
                self.expect_simple(
                    TokenKind::Semicolon,
                    "expected `;` after associated type binding",
                )?;
                associated_types.push(AssociatedTypeBinding {
                    name: assoc,
                    name_span: assoc_span,
                    ty,
                });
                continue;
            }
            let public = self.consume(&TokenKind::Pub).is_some();
            let mut method = self.parse_function()?;
            method.public = public;
            methods.push(method);
        }
        self.expect_simple(TokenKind::RightBrace, "expected `}` after impl body")?;
        Ok(ImplBlock {
            source_id: 0,
            module: String::new(),
            trait_name,
            trait_span,
            trait_arguments,
            type_name,
            type_span,
            type_arguments,
            type_params,
            associated_types,
            methods,
        })
    }

    fn check_type_keyword(&self) -> bool {
        matches!(&self.current().kind, TokenKind::Identifier(name) if name == "type")
    }

    /// 解析可选的类型实参；缺失时返回空列表。`<` 后无法构成完整类型实参时回溯，
    /// 由调用方继续按非泛型形式诊断。
    fn parse_optional_type_arguments(&mut self) -> Vec<TypeRef> {
        self.try_parse_type_arguments_checked()
            .map(|(arguments, _)| arguments)
            .unwrap_or_default()
    }

    fn parse_function(&mut self) -> Result<Function, Diagnostic> {
        let public = self.consume(&TokenKind::Pub).is_some();
        let (start, name, name_span, type_params, parameters, return_type) =
            self.parse_function_parts()?;
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
            extern_c: false,
            link_name: None,
        })
    }

    #[allow(clippy::type_complexity)]
    fn parse_function_parts(
        &mut self,
    ) -> Result<
        (
            Span,
            String,
            Span,
            Vec<TypeParamDecl>,
            Vec<Parameter>,
            Option<TypeRef>,
        ),
        Diagnostic,
    > {
        let start = self.expect_simple(TokenKind::Fn, "expected `fn`")?;
        let (name, name_span) = self.expect_identifier("expected function name")?;
        let type_params = self.parse_type_params()?;
        let parameters = self.parse_parameters()?;
        let return_type = if self.consume(&TokenKind::Colon).is_some() {
            Some(self.parse_type("expected return type")?)
        } else {
            None
        };
        Ok((start, name, name_span, type_params, parameters, return_type))
    }

    /// 解析可选的泛型参数列表 `<T, U>`。
    fn parse_type_params(&mut self) -> Result<Vec<TypeParamDecl>, Diagnostic> {
        if self.consume(&TokenKind::Less).is_none() {
            return Ok(Vec::new());
        }
        let mut params = Vec::new();
        loop {
            let (name, name_span) = self.expect_identifier("expected type parameter name")?;
            let bound = if self.consume(&TokenKind::Colon).is_some() {
                let path = self.parse_path("expected trait bound after `:`")?;
                Some(path.segments.join("."))
            } else {
                None
            };
            params.push(TypeParamDecl {
                name,
                name_span,
                bound,
            });
            if self.consume(&TokenKind::Comma).is_some() {
                continue;
            }
            break;
        }
        self.expect_simple(TokenKind::Greater, "expected `>` after type parameters")?;
        Ok(params)
    }

    /// 解析参数列表，支持首参数为方法接收者 `self` / `self: *Self`。
    fn parse_parameters(&mut self) -> Result<Vec<Parameter>, Diagnostic> {
        self.expect_simple(TokenKind::LeftParen, "expected `(` after function name")?;
        let mut parameters = Vec::new();
        if !self.check(&TokenKind::RightParen) {
            loop {
                if parameters.is_empty()
                    && let TokenKind::Identifier(name) = &self.current().kind
                    && name == "self"
                {
                    let span = self.current().span;
                    self.advance();
                    let ty = if self.consume(&TokenKind::Colon).is_some() {
                        self.parse_type("expected receiver type after `self:`")?
                    } else {
                        TypeRef {
                            kind: TypeRefKind::Name {
                                name: "Self".to_string(),
                                arguments: Vec::new(),
                            },
                            span,
                        }
                    };
                    parameters.push(Parameter {
                        name: "self".to_string(),
                        name_span: span,
                        ty,
                        receiver: true,
                    });
                    if self.consume(&TokenKind::Comma).is_none() {
                        break;
                    }
                    continue;
                }
                let (parameter_name, parameter_span) =
                    self.expect_identifier("expected parameter name")?;
                self.expect_simple(TokenKind::Colon, "expected `:` after parameter name")?;
                let ty = self.parse_type("expected parameter type")?;
                parameters.push(Parameter {
                    name: parameter_name,
                    name_span: parameter_span,
                    ty,
                    receiver: false,
                });
                if self.consume(&TokenKind::Comma).is_none() {
                    break;
                }
            }
        }
        self.expect_simple(TokenKind::RightParen, "expected `)` after parameters")?;
        Ok(parameters)
    }

    /// `extern` 后是否跟 `struct`（而非 `"C" { ... }`）。
    fn check_extern_struct(&self) -> bool {
        matches!(
            self.tokens.get(self.position + 1).map(|token| &token.kind),
            Some(TokenKind::Struct)
        )
    }

    /// 解析 `extern "C" { pub fn name(...): T; ... }`。
    fn parse_extern_block(&mut self, functions: &mut Vec<Function>) -> Result<(), Diagnostic> {
        self.expect_simple(TokenKind::Extern, "expected `extern`")?;
        let abi = self.advance().clone();
        let TokenKind::String(name) = abi.kind else {
            return Err(Diagnostic::at(
                self.source,
                abi.span,
                "expected an ABI string literal after `extern`",
            ));
        };
        if name != "C" {
            return Err(Diagnostic::at(
                self.source,
                abi.span,
                format!("unsupported extern ABI `{name}`; only `\"C\"` is supported"),
            ));
        }
        self.expect_simple(TokenKind::LeftBrace, "expected `{` after `extern \"C\"`")?;
        while !self.check(&TokenKind::RightBrace) && !self.check(&TokenKind::Eof) {
            let public = self.consume(&TokenKind::Pub).is_some();
            let mut function = self.parse_extern_function()?;
            function.public = public;
            functions.push(function);
        }
        self.expect_simple(TokenKind::RightBrace, "expected `}` after extern block")?;
        Ok(())
    }

    /// 解析一条 extern 函数声明（无函数体，以 `;` 结束）。
    fn parse_extern_function(&mut self) -> Result<Function, Diagnostic> {
        let start = self.expect_simple(TokenKind::Fn, "expected `fn` in extern block")?;
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
                    receiver: false,
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
        let end = self.expect_simple(TokenKind::Semicolon, "expected `;` after extern function")?;
        Ok(Function {
            source_id: 0,
            public: false,
            name: name.clone(),
            name_span,
            type_params: Vec::new(),
            parameters,
            return_type,
            body: Vec::new(),
            span: start.merge(end),
            extern_c: true,
            link_name: Some(name),
        })
    }

    fn parse_struct(&mut self) -> Result<StructDecl, Diagnostic> {
        self.expect_simple(TokenKind::Struct, "expected `struct`")?;
        let (name, name_span) = self.expect_identifier("expected struct name")?;
        let type_params = self.parse_type_params()?;
        self.expect_simple(TokenKind::LeftBrace, "expected `{` after struct name")?;
        let mut fields = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.check(&TokenKind::Eof) {
            let public = self.consume(&TokenKind::Pub).is_some();
            let (field_name, field_span) = self.expect_identifier("expected field name")?;
            self.expect_simple(TokenKind::Colon, "expected `:` after field name")?;
            let ty = self.parse_type("expected field type")?;
            fields.push(FieldDecl {
                name: field_name,
                name_span: field_span,
                ty,
                public,
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
            extern_c: false,
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
        // 指针：`*T` 或 `*const T`。
        if let Some(left) = self.consume(&TokenKind::Star) {
            let mutable = !self.consume_const_keyword();
            let pointee = self.parse_type(message)?;
            let span = left.merge(pointee.span);
            return Ok(TypeRef {
                kind: TypeRefKind::Ptr {
                    pointee: Box::new(pointee),
                    mutable,
                },
                span,
            });
        }
        if let Some(left) = self.consume(&TokenKind::LeftBracket) {
            // 切片：`[]T` 或 `[]const T`。
            if self.consume(&TokenKind::RightBracket).is_some() {
                let mutable = !self.consume_const_keyword();
                let element = self.parse_type(message)?;
                let span = left.merge(element.span);
                return Ok(TypeRef {
                    kind: TypeRefKind::Slice {
                        element: Box::new(element),
                        mutable,
                    },
                    span,
                });
            }
            // 定长数组：`[T; N]`。
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
        let (mut name, mut span) = self.expect_identifier(message)?;
        // 模块限定类型路径：`numbers.Pair`。
        while self.check(&TokenKind::Dot)
            && matches!(
                self.tokens.get(self.position + 1).map(|token| &token.kind),
                Some(TokenKind::Identifier(_))
            )
        {
            self.advance();
            let (segment, segment_span) = self.expect_identifier("expected type name after `.`")?;
            name.push('.');
            name.push_str(&segment);
            span = span.merge(segment_span);
        }
        // 关联类型路径：`Self::Item` / `I::Item`。
        while self.consume(&TokenKind::ColonColon).is_some() {
            let (segment, segment_span) =
                self.expect_identifier("expected associated type name after `::`")?;
            name.push_str("::");
            name.push_str(&segment);
            span = span.merge(segment_span);
        }
        let (arguments, end) = self.parse_type_arguments_after(span, message)?;
        Ok(TypeRef {
            kind: TypeRefKind::Name { name, arguments },
            span: span.merge(end),
        })
    }

    /// 类型位置上的泛型实参 `<T, U>`（若存在）。返回 (实参, 结束 span)。
    fn parse_type_arguments_after(
        &mut self,
        span: Span,
        message: &str,
    ) -> Result<(Vec<TypeRef>, Span), Diagnostic> {
        if self.consume(&TokenKind::Less).is_none() {
            return Ok((Vec::new(), span));
        }
        let mut arguments = Vec::new();
        loop {
            arguments.push(self.parse_type(message)?);
            if self.consume(&TokenKind::Comma).is_some() {
                continue;
            }
            break;
        }
        let end = self.expect_simple(TokenKind::Greater, "expected `>` after type arguments")?;
        Ok((arguments, end))
    }

    /// 尝试解析显式类型实参 `<T, U>`（带回溯）。
    ///
    /// 失败时恢复位置并返回 `None`，调用方即可按比较运算继续解析。
    fn try_parse_type_arguments_checked(&mut self) -> Option<(Vec<TypeRef>, Span)> {
        let saved = self.position;
        let start = self.consume(&TokenKind::Less)?;
        let mut arguments = Vec::new();
        loop {
            match self.parse_type("expected type argument") {
                Ok(ty) => arguments.push(ty),
                Err(_) => {
                    self.position = saved;
                    return None;
                }
            }
            if self.consume(&TokenKind::Comma).is_some() {
                continue;
            }
            break;
        }
        match self.expect_simple(TokenKind::Greater, "expected `>` after type arguments") {
            Ok(end) => Some((arguments, start.merge(end))),
            Err(_) => {
                self.position = saved;
                None
            }
        }
    }

    /// 解析以标识符起始的表达式：名字、字段链、调用、`Type<i32>::assoc()`、
    /// `Enum<i32>.Variant`（M15）。
    ///
    /// `<` 仍可表示比较：仅当 `<` 与前一路径**紧邻**，且完整类型实参后接
    /// `(`、`::` 或 `.` 时，才按泛型路径解析，否则回退为比较运算。
    fn parse_path_expression(&mut self, start: Span, name: String) -> Result<Expr, Diagnostic> {
        let mut segments = vec![name];
        let mut segment_spans = vec![start];
        let mut path_end = start.end;
        while self.check(&TokenKind::Dot) {
            self.advance();
            let (segment, segment_span) = self.expect_identifier("expected name after `.`")?;
            segments.push(segment);
            segment_spans.push(segment_span);
            path_end = segment_span.end;
        }

        let mut type_arguments = Vec::new();
        let mut explicit_type_arguments = false;
        if self.check(&TokenKind::Less) && self.current().span.start == path_end {
            let saved = self.position;
            match self.try_parse_type_arguments_checked() {
                Some((arguments, _))
                    if matches!(
                        self.current().kind,
                        TokenKind::LeftParen | TokenKind::ColonColon | TokenKind::Dot
                    ) =>
                {
                    type_arguments = arguments;
                    explicit_type_arguments = true;
                }
                _ => self.position = saved,
            }
        }

        if self.check(&TokenKind::ColonColon) {
            let mut path = segments.join(".");
            while self.consume(&TokenKind::ColonColon).is_some() {
                let (segment, segment_span) = self.expect_identifier("expected name after `::`")?;
                path.push_str("::");
                path.push_str(&segment);
                path_end = segment_span.end;
            }
            if self.check(&TokenKind::LeftParen) {
                return self.parse_call(path, Span::new(start.start, path_end), type_arguments);
            }
            return Err(Diagnostic::at(
                self.source,
                start,
                "associated items are not supported in this position yet",
            ));
        }

        if self.check(&TokenKind::LeftParen) {
            return self.parse_call(
                segments.join("."),
                Span::new(start.start, path_end),
                type_arguments,
            );
        }

        if explicit_type_arguments {
            if self.check(&TokenKind::Dot) {
                self.advance();
                let (variant, variant_span) =
                    self.expect_identifier("expected variant name after `.`")?;
                let path = format!("{}.{}", segments.join("."), variant);
                let span = start.merge(variant_span);
                if self.check(&TokenKind::LeftParen) {
                    return self.parse_call(path, span, type_arguments);
                }
                return Ok(Expr {
                    kind: ExprKind::Call {
                        callee: path,
                        callee_span: span,
                        type_arguments,
                        arguments: Vec::new(),
                    },
                    span,
                });
            }
            return Err(Diagnostic::at(
                self.source,
                start,
                "type arguments must be followed by `(`, `::`, or `.`",
            ));
        }

        if segments.len() == 1 {
            return Ok(Expr {
                kind: ExprKind::Name(segments.into_iter().next().unwrap()),
                span: start,
            });
        }
        // 无括号的点号链：字段访问。
        let mut expression = Expr {
            kind: ExprKind::Name(segments[0].clone()),
            span: start,
        };
        for (index, field) in segments[1..].iter().enumerate() {
            let field_span = segment_spans[index + 1];
            expression = Expr {
                kind: ExprKind::Field {
                    base: Box::new(expression),
                    field: field.clone(),
                    field_span,
                },
                span: Span::new(start.start, field_span.end),
            };
        }
        Ok(expression)
    }

    fn parse_call(
        &mut self,
        callee: String,
        start: Span,
        type_arguments: Vec<TypeRef>,
    ) -> Result<Expr, Diagnostic> {
        self.expect_simple(TokenKind::LeftParen, "expected `(` after function name")?;
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
        Ok(Expr {
            kind: ExprKind::Call {
                callee,
                callee_span: start,
                type_arguments,
                arguments,
            },
            span: start.merge(right),
        })
    }

    /// 若当前 token 是标识符 `const` 则消费并返回 true。
    fn consume_const_keyword(&mut self) -> bool {
        if matches!(&self.current().kind, TokenKind::Identifier(name) if name == "const") {
            self.advance();
            true
        } else {
            false
        }
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
            TokenKind::Defer => {
                self.advance();
                let call = self.parse_expression(0)?;
                self.expect_simple(TokenKind::Semicolon, "expected `;` after `defer` call")?;
                StatementKind::Defer(call)
            }
            TokenKind::Return => self.parse_return()?,
            TokenKind::Identifier(_) if self.is_index_assignment() => {
                self.parse_index_assignment()?
            }
            TokenKind::Identifier(_) if self.is_field_assignment() => {
                self.parse_field_assignment()?
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

    fn parse_field_assignment(&mut self) -> Result<StatementKind, Diagnostic> {
        let (name, name_span) = self.expect_identifier("expected variable name")?;
        let deref = if self.consume(&TokenKind::Arrow).is_some() {
            true
        } else {
            self.expect_simple(TokenKind::Dot, "expected `.` or `->` after variable name")?;
            false
        };
        let (field, field_span) = self.expect_identifier("expected field name")?;
        let operator = self.parse_assignment_operator();
        let value = self.parse_expression(0)?;
        self.expect_simple(TokenKind::Semicolon, "expected `;` after assignment")?;
        Ok(StatementKind::FieldAssignment {
            name,
            name_span,
            field,
            field_span,
            deref,
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
            TokenKind::Identifier(name) => self.parse_path_expression(token.span, name)?,
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
            TokenKind::Ampersand => {
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
                // `p->field` 等价于 `(*p).field`。
                let (field, field_span) =
                    self.expect_identifier("expected field name after `->`")?;
                let span = expression.span.merge(field_span);
                let deref = Expr {
                    kind: ExprKind::Deref {
                        operand: Box::new(expression),
                    },
                    span,
                };
                expression = Expr {
                    kind: ExprKind::Field {
                        base: Box::new(deref),
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
        let mut type_arguments = Vec::new();
        if self.check(&TokenKind::Less)
            && self.current().span.start == name_span.end
            && let Some((arguments, _)) = self.try_parse_type_arguments_checked()
        {
            type_arguments = arguments;
        }
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
            type_arguments,
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

    fn is_field_assignment(&self) -> bool {
        // 形如 `name.field = ...` 或 `name->field = ...`：
        // identifier (`.` 或 `->`) identifier 赋值运算符。
        if !matches!(
            self.tokens.get(self.position + 1).map(|token| &token.kind),
            Some(TokenKind::Dot | TokenKind::Arrow)
        ) {
            return false;
        }
        if !matches!(
            self.tokens.get(self.position + 2).map(|token| &token.kind),
            Some(TokenKind::Identifier(_))
        ) {
            return false;
        }
        matches!(
            self.tokens.get(self.position + 3).map(|token| &token.kind),
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

    use dolphin_source::lexer;

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
    fn parses_intrinsic_type_arguments() {
        let program = parse_text("fn main() { val b = mem.alloc<u8>(4); mem.free(b); }");
        let StatementKind::Variable { initializer, .. } = &program.functions[0].body[0].kind else {
            panic!("first statement should be a variable");
        };
        let ExprKind::Call {
            callee,
            type_arguments,
            ..
        } = &initializer.kind
        else {
            panic!("initializer should be a call");
        };
        assert_eq!(callee, "mem.alloc");
        assert_eq!(type_arguments.len(), 1);
    }

    #[test]
    fn comparison_is_not_parsed_as_type_arguments() {
        let program = parse_text("fn main() { val a = 1; val b = 2; if a < b { return; } }");
        assert_eq!(program.functions[0].body.len(), 3);
    }
}
