//! 泛型单态化（M15-A）：模板收集、实例请求工作队列与具体类型布局。
//!
//! 设计要点：
//! - 泛型是编译期机制，最终 IR 中不含未展开的泛型；
//! - 实例 key 为 `(限定名, 具体类型实参)`，同名不同实例不会碰撞；
//! - 类型实例按需创建并登记占位，支持 `struct Node<T> { next: *Node<T> }` 这类
//!   经指针自引用；按值包含环在实例完成后立即检测；
//! - 泛型函数体在遇到泛型调用时登记新实例，直到队列不动点。

use std::collections::{HashMap, HashSet};

use dolphin_ir::ir::{self, EnumVariant, FunctionId, StructField, Type, TypeDef, TypeId};
use dolphin_ir::layout;
use dolphin_package::package::PackageId;
use dolphin_source::diagnostic::Diagnostic;
use dolphin_source::source::{SourceFile, Span};
use dolphin_syntax::ast::{self, FieldDecl, TypeRef, TypeRefKind, VariantDecl};

/// 单态化实例链深度上限（§2.2）。
pub const MAX_INSTANCE_DEPTH: usize = 128;
/// 单次构建实例总数上限（§2.2）。
pub const MAX_INSTANCES: usize = 10_000;

/// 泛型实例的稳定身份：`(PackageId, 限定名, 具体类型实参)`。
///
/// 同名定义来自不同包时不会碰撞（M15 规格 §2.2）。本批次所有定义都属于
/// `PackageId::ROOT`；源码标准库接入时使用 `PackageId::STD`。
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct GenericKey {
    pub package: PackageId,
    pub name: String,
    pub args: Vec<Type>,
}

impl GenericKey {
    pub fn in_package(package: PackageId, name: impl Into<String>, args: Vec<Type>) -> Self {
        Self {
            package,
            name: name.into(),
            args,
        }
    }
}

/// 类型参数环境：模板类型参数名到具体类型的映射。
#[derive(Clone, Default, Debug)]
pub struct TypeEnv {
    params: Vec<(String, Type)>,
}

impl TypeEnv {
    pub fn get(&self, name: &str) -> Option<Type> {
        self.params
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, ty)| ty.clone())
    }

    pub fn bind(&mut self, name: &str, ty: Type) {
        if !self.params.iter().any(|(key, _)| key == name) {
            self.params.push((name.to_string(), ty));
        }
    }
}

pub struct StructTemplate {
    pub package: PackageId,
    pub source_id: usize,
    pub name_span: Span,
    pub type_params: Vec<String>,
    /// 类型参数约束：类型参数名 -> 限定 trait 名。
    pub bounds: Vec<(String, String)>,
    pub fields: Vec<FieldDecl>,
    pub extern_c: bool,
}

pub struct EnumTemplate {
    pub package: PackageId,
    pub source_id: usize,
    pub name_span: Span,
    pub type_params: Vec<String>,
    /// 类型参数约束：类型参数名 -> 限定 trait 名。
    pub bounds: Vec<(String, String)>,
    pub variants: Vec<VariantDecl>,
}

pub struct FunctionTemplate {
    pub package: PackageId,
    pub module: String,
    pub source_id: usize,
    pub function: ast::Function,
    /// 类型参数约束：类型参数名 -> 限定 trait 名。
    pub bounds: Vec<(String, String)>,
}

impl FunctionTemplate {
    pub fn type_params(&self) -> &[ast::TypeParamDecl] {
        &self.function.type_params
    }
}

/// 方法模板：固有方法或 trait impl 方法，键为 `Type::method`。
pub struct MethodTemplate {
    pub package: PackageId,
    /// impl 块所在模块（字段可见性判定）。
    pub module: String,
    /// 目标类型限定名。
    pub type_name: String,
    /// 关联的 trait 限定名（固有方法为 `None`）。
    pub trait_name: Option<String>,
    /// impl 块声明的类型参数名。
    pub type_params: Vec<String>,
    /// `type Item = X;` 关联类型绑定。
    pub associated_types: Vec<(String, TypeRef)>,
    pub function: ast::Function,
}

/// trait 声明模板（只有签名与关联类型）。
pub struct TraitTemplate {
    pub associated_types: Vec<String>,
    pub methods: Vec<ast::Function>,
}

/// 一个 trait impl 的静态信息，用于类型参数约束与关联类型解析。
pub struct TraitImplTemplate {
    pub type_params: Vec<String>,
    pub associated_types: Vec<(String, TypeRef)>,
}

/// 所有泛型/普通模板，按限定名索引。
#[derive(Default)]
pub struct TemplateTables {
    pub structs: HashMap<String, StructTemplate>,
    pub enums: HashMap<String, EnumTemplate>,
    pub functions: HashMap<String, FunctionTemplate>,
    /// `Type::method` -> 方法模板（固有与 trait impl 合并）。
    pub methods: HashMap<String, MethodTemplate>,
    /// trait 限定名 -> trait 声明。
    pub traits: HashMap<String, TraitTemplate>,
    /// `(trait, type)` -> impl 静态信息。
    pub trait_impls: HashMap<(String, String), TraitImplTemplate>,
}

impl TemplateTables {
    /// 按实例名取函数体 AST（普通函数或方法）。
    pub fn function_ast(&self, name: &str) -> Option<&ast::Function> {
        if let Some(template) = self.functions.get(name) {
            return Some(&template.function);
        }
        self.methods.get(name).map(|template| &template.function)
    }

    /// 定义所在包身份（当前均为根包；标准库接入后返回 `STD`）。
    pub fn package_of(&self, name: &str) -> PackageId {
        if let Some(template) = self.structs.get(name) {
            return template.package;
        }
        if let Some(template) = self.enums.get(name) {
            return template.package;
        }
        if let Some(template) = self.functions.get(name) {
            return template.package;
        }
        if let Some(template) = self.methods.get(name) {
            return template.package;
        }
        PackageId::ROOT
    }
}

/// 单态化上下文：模板与源码（诊断需要）。
pub struct MonoContext<'a> {
    pub sources: &'a [SourceFile],
    pub tables: &'a TemplateTables,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InstanceStatus {
    Queued,
    InProgress,
    Done,
}

pub struct InstanceInfo {
    pub name: String,
    /// 实例定义所在模块（字段可见性判定）。
    pub module: String,
    pub env: TypeEnv,
    pub parameters: Vec<Type>,
    pub return_type: Type,
    pub depth: usize,
    pub status: InstanceStatus,
    /// 实例化模板所在源码文件（用于函数体 lowering 的诊断）。
    pub source_id: usize,
    /// 是否为 extern "C" 声明（无函数体）。
    pub extern_c: bool,
    pub link_name: Option<String>,
}

/// 单态化状态：类型表、函数实例表与工作队列。
#[derive(Default)]
pub struct MonoState {
    pub types: Vec<TypeDef>,
    pub type_keys: Vec<GenericKey>,
    pub type_spans: Vec<Span>,
    pub type_ids: HashMap<GenericKey, TypeId>,
    pub instances: Vec<InstanceInfo>,
    pub instance_ids: HashMap<GenericKey, FunctionId>,
    pub pending: Vec<FunctionId>,
    pub lowered: Vec<Option<ir::Function>>,
    pub main: Option<FunctionId>,
    in_progress_types: Vec<GenericKey>,
}

impl MonoState {
    pub fn instance(&self, id: FunctionId) -> &InstanceInfo {
        &self.instances[id.0]
    }

    pub fn instance_signature(&self, id: FunctionId) -> Signature {
        let instance = self.instance(id);
        Signature {
            id,
            parameters: instance.parameters.clone(),
            return_type: instance.return_type.clone(),
        }
    }

    pub fn type_of(&self, id: TypeId) -> Type {
        match &self.types[id.0] {
            TypeDef::Struct { .. } => Type::Struct(id),
            TypeDef::Enum { .. } => Type::Enum(id),
        }
    }

    pub fn struct_fields(&self, id: TypeId) -> &[StructField] {
        match &self.types[id.0] {
            TypeDef::Struct { fields, .. } => fields,
            _ => unreachable!("struct_fields resolves a struct type"),
        }
    }

    pub fn enum_variants(&self, id: TypeId) -> &[EnumVariant] {
        match &self.types[id.0] {
            TypeDef::Enum { variants } => variants,
            _ => unreachable!("enum_variants resolves an enum type"),
        }
    }

    pub fn is_enum(&self, id: TypeId) -> bool {
        matches!(&self.types[id.0], TypeDef::Enum { .. })
    }

    pub fn is_extern_struct(&self, ty: &Type) -> bool {
        matches!(
            ty,
            Type::Struct(id)
                if matches!(&self.types[id.0], TypeDef::Struct { extern_c: true, .. })
        )
    }

    /// 非泛型类型是否已实例化（用于构造与 match 的快速判断）。
    pub fn concrete_type_id(&self, tables: &TemplateTables, name: &str) -> Option<TypeId> {
        let package = tables.package_of(name);
        self.type_ids
            .get(&GenericKey::in_package(package, name, Vec::new()))
            .copied()
    }

    /// 模板类型参数的实例化环境。
    fn build_env(
        type_params: &[String],
        args: &[Type],
        source: &SourceFile,
        span: Span,
    ) -> Result<TypeEnv, Diagnostic> {
        if type_params.len() != args.len() {
            return Err(Diagnostic::at(
                source,
                span,
                format!(
                    "type expects {} type arguments but {} were provided",
                    type_params.len(),
                    args.len()
                ),
            ));
        }
        let mut env = TypeEnv::default();
        for (name, ty) in type_params.iter().zip(args.iter()) {
            env.bind(name, ty.clone());
        }
        Ok(env)
    }

    fn template_type_params(tables: &TemplateTables, name: &str) -> Option<Vec<String>> {
        if let Some(template) = tables.structs.get(name) {
            return Some(template.type_params.clone());
        }
        tables
            .enums
            .get(name)
            .map(|template| template.type_params.clone())
    }

    /// 解析类型引用为具体类型；泛型具名类型会触发实例化。
    pub fn resolve_type(
        &mut self,
        ctx: &MonoContext,
        source: &SourceFile,
        typeref: &TypeRef,
        env: &TypeEnv,
    ) -> Result<Type, Diagnostic> {
        self.resolve_type_at(ctx, source, typeref, env, 0)
    }

    /// 带实例化深度的类型解析；深度用于阻断无限展开的泛型类型。
    fn resolve_type_at(
        &mut self,
        ctx: &MonoContext,
        source: &SourceFile,
        typeref: &TypeRef,
        env: &TypeEnv,
        depth: usize,
    ) -> Result<Type, Diagnostic> {
        if depth > MAX_INSTANCE_DEPTH {
            return Err(Diagnostic::at(
                source,
                typeref.span,
                "type instantiation is too deep",
            ));
        }
        match &typeref.kind {
            TypeRefKind::Name { name, arguments } => {
                if arguments.is_empty()
                    && let Some(ty) = env.get(name)
                {
                    return Ok(ty);
                }
                if let Some(scalar) = builtin_type(name) {
                    if !arguments.is_empty() {
                        return Err(Diagnostic::at(
                            source,
                            typeref.span,
                            format!("type `{name}` does not accept type arguments"),
                        ));
                    }
                    return Ok(scalar);
                }
                if name == "Self" {
                    return Err(Diagnostic::at(
                        source,
                        typeref.span,
                        "`Self` can only be used inside an `impl` block",
                    ));
                }
                let Some(type_params) = Self::template_type_params(ctx.tables, name) else {
                    return Err(Diagnostic::at(
                        source,
                        typeref.span,
                        format!("unknown type `{name}`"),
                    ));
                };
                let mut args = Vec::with_capacity(arguments.len());
                for argument in arguments {
                    args.push(self.resolve_type_at(ctx, source, argument, env, depth + 1)?);
                }
                self.instantiate_named_at(
                    ctx,
                    source,
                    name,
                    &type_params,
                    args,
                    typeref.span,
                    depth,
                )
            }
            TypeRefKind::Array { element, length } => {
                if *length == 0 {
                    return Err(Diagnostic::at(
                        source,
                        typeref.span,
                        "array length must be greater than zero",
                    ));
                }
                if *length > i32::MAX as usize {
                    return Err(Diagnostic::at(
                        source,
                        typeref.span,
                        "array length is too large",
                    ));
                }
                let element_type = self.resolve_type_at(ctx, source, element, env, depth + 1)?;
                let scalar = element_type.as_scalar().ok_or_else(|| {
                    Diagnostic::at(
                        source,
                        element.span,
                        "nested arrays are not implemented yet",
                    )
                })?;
                if layout::array_byte_size(scalar, *length, layout::POINTER_BYTES)
                    > layout::MAX_AGGREGATE_BYTES
                {
                    return Err(Diagnostic::at(
                        source,
                        typeref.span,
                        "array is too large for the target",
                    ));
                }
                Ok(Type::Array {
                    element: scalar,
                    length: *length,
                })
            }
            TypeRefKind::Ptr { pointee, mutable } => {
                let pointee = self.resolve_type_at(ctx, source, pointee, env, depth + 1)?;
                Ok(Type::Ptr {
                    pointee: Box::new(pointee),
                    mutable: *mutable,
                })
            }
            TypeRefKind::Slice { element, mutable } => {
                let element = self.resolve_type_at(ctx, source, element, env, depth + 1)?;
                Ok(Type::Slice {
                    element: Box::new(element),
                    mutable: *mutable,
                })
            }
        }
    }

    /// 按名字与具体实参实例化结构体/枚举类型（幂等）。
    pub fn instantiate_named(
        &mut self,
        ctx: &MonoContext,
        source: &SourceFile,
        name: &str,
        type_params: &[String],
        args: Vec<Type>,
        span: Span,
    ) -> Result<Type, Diagnostic> {
        self.instantiate_named_at(ctx, source, name, type_params, args, span, 0)
    }

    #[allow(clippy::too_many_arguments)]
    fn instantiate_named_at(
        &mut self,
        ctx: &MonoContext,
        source: &SourceFile,
        name: &str,
        type_params: &[String],
        args: Vec<Type>,
        span: Span,
        depth: usize,
    ) -> Result<Type, Diagnostic> {
        if depth > MAX_INSTANCE_DEPTH {
            return Err(Diagnostic::at(
                source,
                span,
                "type instantiation is too deep",
            ));
        }
        let package = ctx.tables.package_of(name);
        let key = GenericKey::in_package(package, name, args.clone());
        if let Some(id) = self.type_ids.get(&key) {
            return Ok(self.type_of(*id));
        }
        if self.types.len() >= MAX_INSTANCES {
            return Err(Diagnostic::at(
                source,
                span,
                "too many monomorphized types in one build",
            ));
        }
        let mut env = Self::build_env(type_params, &args, source, span)?;
        let id = TypeId(self.types.len());
        // 先登记占位，支持经指针的自引用；字段稍后填充。
        if let Some(template) = ctx.tables.structs.get(name) {
            self.types.push(TypeDef::Struct {
                fields: Vec::new(),
                extern_c: template.extern_c,
            });
            self.type_spans.push(template.name_span);
        } else if let Some(template) = ctx.tables.enums.get(name) {
            let _ = template;
            self.types.push(TypeDef::Enum {
                variants: Vec::new(),
            });
            self.type_spans.push(ctx.tables.enums[name].name_span);
        } else {
            return Err(Diagnostic::at(
                source,
                span,
                format!("unknown type `{name}`"),
            ));
        }
        self.type_keys.push(key.clone());
        self.type_ids.insert(key.clone(), id);
        self.in_progress_types.push(key.clone());

        // 具名类型实例化的公共入口：先校验定义处声明的类型参数约束，并在
        // 成功时绑定关联类型（`T::Item`），再进行字段/variant 类型解析。
        // 占位类型已登记，允许约束关联类型经指针/递归引用当前实例。
        let bounds = ctx
            .tables
            .structs
            .get(name)
            .map(|template| template.bounds.clone())
            .or_else(|| {
                ctx.tables
                    .enums
                    .get(name)
                    .map(|template| template.bounds.clone())
            })
            .unwrap_or_default();
        self.bind_param_bounds(ctx, source, &bounds, type_params, &mut env, span)?;

        if let Some(template) = ctx.tables.structs.get(name) {
            let template_source = &ctx.sources[template.source_id];
            let mut fields = Vec::with_capacity(template.fields.len());
            let mut seen = HashSet::new();
            for field in &template.fields {
                if !seen.insert(field.name.clone()) {
                    return Err(Diagnostic::at(
                        template_source,
                        field.name_span,
                        format!("field `{}` is already defined", field.name),
                    ));
                }
            }
            for field in &template.fields {
                let ty = self.resolve_type_at(ctx, template_source, &field.ty, &env, depth + 1)?;
                if template.extern_c {
                    self.validate_extern_struct_field(template_source, &ty, field.ty.span)?;
                }
                fields.push(StructField {
                    name: field.name.clone(),
                    ty,
                    public: field.public,
                });
            }
            let extern_c = template.extern_c;
            self.types[id.0] = TypeDef::Struct { fields, extern_c };
        } else {
            let template = &ctx.tables.enums[name];
            let template_source = &ctx.sources[template.source_id];
            let mut seen = HashSet::new();
            for variant in &template.variants {
                if !seen.insert(variant.name.clone()) {
                    return Err(Diagnostic::at(
                        template_source,
                        variant.name_span,
                        format!("variant `{}` is already defined", variant.name),
                    ));
                }
            }
            let mut variants = Vec::with_capacity(template.variants.len());
            for variant in &template.variants {
                let mut fields = Vec::with_capacity(variant.fields.len());
                for field in &variant.fields {
                    fields.push(self.resolve_type_at(
                        ctx,
                        template_source,
                        field,
                        &env,
                        depth + 1,
                    )?);
                }
                variants.push(EnumVariant {
                    name: variant.name.clone(),
                    fields,
                });
            }
            self.types[id.0] = TypeDef::Enum { variants };
        }

        self.in_progress_types.pop();
        self.ensure_acyclic(id)?;
        self.validate_aggregate_size(source, span, name, id)?;
        Ok(self.type_of(id))
    }

    /// 拒绝超过聚合预算的类型，避免布局尺寸在 u32 中静默饱和成可分配大小。
    fn validate_aggregate_size(
        &self,
        source: &SourceFile,
        span: Span,
        name: &str,
        id: TypeId,
    ) -> Result<(), Diagnostic> {
        let size = layout::checked_size_of(&self.type_of(id), &self.types, layout::POINTER_BYTES);
        if size > layout::MAX_AGGREGATE_BYTES {
            return Err(Diagnostic::at(
                source,
                span,
                format!("type `{name}` is too large for the target"),
            ));
        }
        Ok(())
    }

    /// 检测从 `root` 出发按值包含是否回到自身。
    fn ensure_acyclic(&self, root: TypeId) -> Result<(), Diagnostic> {
        let mut stack = vec![root];
        let mut seen = HashSet::new();
        seen.insert(root.0);
        while let Some(id) = stack.pop() {
            for dependency in self.contained_user_types(&self.types[id.0]) {
                if dependency == root {
                    return Err(Diagnostic::plain(format!(
                        "recursive layout: type `{}` contains itself by value",
                        self.type_display(root)
                    )));
                }
                if seen.insert(dependency.0) {
                    stack.push(dependency);
                }
            }
        }
        Ok(())
    }

    fn type_display(&self, id: TypeId) -> String {
        let key = &self.type_keys[id.0];
        if key.args.is_empty() {
            key.name.clone()
        } else {
            let args: Vec<String> = key.args.iter().map(|ty| ty.to_string()).collect();
            format!("{}<{}>", key.name, args.join(", "))
        }
    }

    /// 一个类型按值包含的用户自定义类型（指针与切片是间接边，不展开）。
    pub fn contained_user_types(&self, def: &TypeDef) -> Vec<TypeId> {
        let collect = |ty: &Type| match ty {
            Type::Struct(id) | Type::Enum(id) => Some(*id),
            _ => None,
        };
        match def {
            TypeDef::Struct { fields, .. } => fields
                .iter()
                .filter_map(|field| collect(&field.ty))
                .collect(),
            TypeDef::Enum { variants } => variants
                .iter()
                .flat_map(|variant| variant.fields.iter())
                .filter_map(collect)
                .collect(),
        }
    }

    /// 全局按值布局环检测（实例化完成后作为兜底）。
    pub fn check_layout_cycles(&self) -> Result<(), Diagnostic> {
        let mut state = vec![0u8; self.types.len()];
        for id in 0..self.types.len() {
            if state[id] == 0 {
                self.visit_layout(TypeId(id), &mut state)?;
            }
        }
        Ok(())
    }

    fn visit_layout(&self, id: TypeId, state: &mut Vec<u8>) -> Result<(), Diagnostic> {
        state[id.0] = 1;
        for dependency in self.contained_user_types(&self.types[id.0]) {
            if state[dependency.0] == 1 {
                return Err(Diagnostic::plain(format!(
                    "recursive layout: type `{}` contains itself by value",
                    self.type_display(id)
                )));
            }
            if state[dependency.0] == 0 {
                self.visit_layout(dependency, state)?;
            }
        }
        state[id.0] = 2;
        Ok(())
    }

    /// 是否为可在 C 签名中按值出现的类型：C 标量或指针。
    pub fn is_c_value_type(&self, ty: &Type) -> bool {
        match ty {
            ty if ty.is_integer() || ty.is_float() => true,
            Type::Ptr { pointee, .. } => self.is_c_pointee(pointee),
            _ => false,
        }
    }

    fn is_c_pointee(&self, ty: &Type) -> bool {
        if ty.is_integer() || ty.is_float() || *ty == Type::Unit || self.is_extern_struct(ty) {
            return true;
        }
        match ty {
            Type::Ptr { pointee, .. } => self.is_c_pointee(pointee),
            Type::Array { element, .. } => self.is_c_pointee(&element.as_type()),
            _ => false,
        }
    }

    fn validate_extern_struct_field(
        &self,
        source: &SourceFile,
        ty: &Type,
        span: Span,
    ) -> Result<(), Diagnostic> {
        if ty.is_integer() || ty.is_float() || self.is_extern_struct(ty) {
            return Ok(());
        }
        if let Type::Ptr { pointee, .. } = ty
            && self.is_c_pointee(pointee)
        {
            return Ok(());
        }
        if let Type::Array { element, .. } = ty
            && (element.as_type().is_integer() || element.as_type().is_float())
        {
            return Ok(());
        }
        Err(Diagnostic::at(
            source,
            span,
            format!("type `{ty}` cannot be used in an `extern struct` field"),
        ))
    }

    /// 登记并返回泛型函数实例 id（幂等）。已登记但尚未展开的实例直接复用。
    pub fn instantiate_function(
        &mut self,
        ctx: &MonoContext,
        source: &SourceFile,
        name: &str,
        args: Vec<Type>,
        span: Span,
        caller_depth: usize,
    ) -> Result<FunctionId, Diagnostic> {
        let Some(template) = ctx.tables.functions.get(name) else {
            return Err(Diagnostic::at(
                source,
                span,
                format!("unknown function `{name}`"),
            ));
        };
        let key = GenericKey::in_package(template.package, name, args.clone());
        if let Some(id) = self.instance_ids.get(&key) {
            return Ok(*id);
        }
        if template.type_params().len() != args.len() {
            return Err(Diagnostic::at(
                source,
                span,
                format!(
                    "function `{name}` expects {} type arguments but {} were provided",
                    template.type_params().len(),
                    args.len()
                ),
            ));
        }
        if self.instances.len() >= MAX_INSTANCES {
            return Err(Diagnostic::at(
                source,
                span,
                "too many monomorphized function instances in one build",
            ));
        }
        let depth = caller_depth + 1;
        if depth > MAX_INSTANCE_DEPTH {
            return Err(Diagnostic::at(
                source,
                span,
                format!(
                    "monomorphization depth limit ({MAX_INSTANCE_DEPTH}) exceeded while instantiating `{name}`"
                ),
            ));
        }
        let param_names: Vec<String> = template
            .type_params()
            .iter()
            .map(|param| param.name.clone())
            .collect();
        let mut env = Self::build_env(&param_names, &args, source, span)?;
        self.bind_param_bounds(ctx, source, &template.bounds, &param_names, &mut env, span)?;
        let template_source = &ctx.sources[template.source_id];
        let mut parameters = Vec::with_capacity(template.function.parameters.len());
        for parameter in &template.function.parameters {
            let ty = self.resolve_type(ctx, template_source, &parameter.ty, &env)?;
            if template.function.extern_c && !self.is_c_value_type(&ty) {
                return Err(Diagnostic::at(
                    template_source,
                    parameter.ty.span,
                    format!(
                        "type `{ty}` cannot be passed by value to a C function; use a C scalar, pointer, or `*Unit`"
                    ),
                ));
            }
            parameters.push(ty);
        }
        let return_type = match &template.function.return_type {
            Some(ty) => {
                let ty = self.resolve_type(ctx, template_source, ty, &env)?;
                if template.function.extern_c && ty != Type::Unit && !self.is_c_value_type(&ty) {
                    return Err(Diagnostic::at(
                        template_source,
                        template.function.return_type.as_ref().unwrap().span,
                        format!("type `{ty}` cannot be returned by value from a C function"),
                    ));
                }
                ty
            }
            None if name == "main" => Type::I32,
            None => Type::Unit,
        };
        let id = FunctionId(self.instances.len());
        let external = template.function.extern_c;
        self.instances.push(InstanceInfo {
            name: name.to_string(),
            module: template.module.clone(),
            env,
            parameters,
            return_type,
            depth,
            status: InstanceStatus::Queued,
            source_id: template.source_id,
            extern_c: external,
            link_name: template.function.link_name.clone(),
        });
        self.instance_ids.insert(key, id);
        self.pending.push(id);
        self.lowered.push(None);
        Ok(id)
    }

    /// 登记并返回方法实例 id（幂等）。`Self` 与关联类型绑定注入实例环境。
    pub fn instantiate_method(
        &mut self,
        ctx: &MonoContext,
        source: &SourceFile,
        method_key: &str,
        type_args: Vec<Type>,
        span: Span,
        caller_depth: usize,
    ) -> Result<FunctionId, Diagnostic> {
        let Some(template) = ctx.tables.methods.get(method_key) else {
            return Err(Diagnostic::at(
                source,
                span,
                format!("unknown method `{method_key}`"),
            ));
        };
        let key = GenericKey::in_package(template.package, method_key, type_args.clone());
        if let Some(id) = self.instance_ids.get(&key) {
            return Ok(*id);
        }
        if template.type_params.len() != type_args.len() {
            return Err(Diagnostic::at(
                source,
                span,
                format!(
                    "method `{method_key}` expects {} type arguments but {} were provided",
                    template.type_params.len(),
                    type_args.len()
                ),
            ));
        }
        if self.instances.len() >= MAX_INSTANCES {
            return Err(Diagnostic::at(
                source,
                span,
                "too many monomorphized function instances in one build",
            ));
        }
        let depth = caller_depth + 1;
        if depth > MAX_INSTANCE_DEPTH {
            return Err(Diagnostic::at(
                source,
                span,
                format!(
                    "monomorphization depth limit ({MAX_INSTANCE_DEPTH}) exceeded while instantiating `{method_key}`"
                ),
            ));
        }
        // 类型实例必须按声明处的参数名建环境（impl 参数允许改名）；
        // 否则字段里的声明参数名无法解析，约束也会被跳过。
        let declaration_params = ctx
            .tables
            .structs
            .get(&template.type_name)
            .map(|template| template.type_params.clone())
            .or_else(|| {
                ctx.tables
                    .enums
                    .get(&template.type_name)
                    .map(|template| template.type_params.clone())
            })
            .unwrap_or_else(|| template.type_params.clone());
        let self_type = self.instantiate_named(
            ctx,
            source,
            &template.type_name,
            &declaration_params,
            type_args.clone(),
            span,
        )?;
        let mut env = TypeEnv::default();
        for (name, ty) in template.type_params.iter().zip(type_args.iter()) {
            env.bind(name, ty.clone());
        }
        env.bind("Self", self_type);
        let template_source = &ctx.sources[template.function.source_id];
        for (name, typeref) in &template.associated_types {
            let resolved = self.resolve_type(ctx, template_source, typeref, &env)?;
            env.bind(name, resolved.clone());
            env.bind(&format!("Self::{name}"), resolved);
        }
        let mut parameters = Vec::with_capacity(template.function.parameters.len());
        for parameter in &template.function.parameters {
            parameters.push(self.resolve_type(ctx, template_source, &parameter.ty, &env)?);
        }
        let return_type = match &template.function.return_type {
            Some(ty) => self.resolve_type(ctx, template_source, ty, &env)?,
            None => Type::Unit,
        };
        let id = FunctionId(self.instances.len());
        self.instances.push(InstanceInfo {
            name: method_key.to_string(),
            module: template.module.clone(),
            env,
            parameters,
            return_type,
            depth,
            status: InstanceStatus::Queued,
            source_id: template.function.source_id,
            extern_c: false,
            link_name: None,
        });
        self.instance_ids.insert(key, id);
        self.pending.push(id);
        self.lowered.push(None);
        Ok(id)
    }

    /// 为带 trait 约束的类型参数绑定关联类型（`C::Element` 等）。
    ///
    /// 约束要求实现类型找得到对应 trait impl；关联类型绑定在 impl 环境
    /// （impl 类型参数 -> 具体实参、`Self` -> 实现类型）下解析。
    fn bind_param_bounds(
        &mut self,
        ctx: &MonoContext,
        source: &SourceFile,
        bounds: &[(String, String)],
        param_names: &[String],
        env: &mut TypeEnv,
        span: Span,
    ) -> Result<(), Diagnostic> {
        for param in param_names {
            let Some(trait_name) = bounds
                .iter()
                .find(|(name, _)| name == param)
                .map(|(_, trait_name)| trait_name.clone())
            else {
                continue;
            };
            let Some(concrete) = env.get(param) else {
                continue;
            };
            let (type_name, type_args) = match &concrete {
                Type::Struct(id) | Type::Enum(id) => (
                    self.type_keys[id.0].name.clone(),
                    self.type_keys[id.0].args.clone(),
                ),
                other => {
                    return Err(Diagnostic::at(
                        source,
                        span,
                        format!("type `{other}` cannot implement trait `{trait_name}`"),
                    ));
                }
            };
            let Some((impl_params, associated)) = ctx
                .tables
                .trait_impls
                .get(&(trait_name.clone(), type_name.clone()))
                .map(|template| {
                    (
                        template.type_params.clone(),
                        template.associated_types.clone(),
                    )
                })
            else {
                return Err(Diagnostic::at(
                    source,
                    span,
                    format!("type `{type_name}` does not implement trait `{trait_name}`"),
                ));
            };
            let mut impl_env = TypeEnv::default();
            for (name, ty) in impl_params.iter().zip(type_args.iter()) {
                impl_env.bind(name, ty.clone());
            }
            impl_env.bind("Self", concrete.clone());
            for (name, typeref) in &associated {
                let resolved = self.resolve_type(ctx, source, typeref, &impl_env)?;
                env.bind(&format!("{param}::{name}"), resolved);
            }
        }
        Ok(())
    }

    /// 用实参类型推断泛型函数的类型实参（§2.1）。
    pub fn infer_type_args(
        &mut self,
        ctx: &MonoContext,
        source: &SourceFile,
        template: &FunctionTemplate,
        actuals: &[Type],
        expected_return: Option<&Type>,
        span: Span,
    ) -> Result<Vec<Type>, Diagnostic> {
        let param_names: Vec<String> = template
            .type_params()
            .iter()
            .map(|param| param.name.clone())
            .collect();
        let mut bindings = TypeEnv::default();
        for (parameter, actual) in template.function.parameters.iter().zip(actuals) {
            self.unify(
                ctx,
                source,
                &parameter.ty,
                actual,
                &param_names,
                &mut bindings,
            )?;
        }
        if let Some(expected) = expected_return
            && let Some(ret) = &template.function.return_type
        {
            let _ = self.unify(ctx, source, ret, expected, &param_names, &mut bindings);
        }
        self.finish_bindings(&param_names, &bindings, source, span)
    }

    /// 由构造实参的类型推断结构体/枚举的类型实参。
    pub fn infer_from_field_types(
        &mut self,
        ctx: &MonoContext,
        source: &SourceFile,
        param_names: &[String],
        field_types: &[TypeRef],
        actuals: &[Type],
        span: Span,
    ) -> Result<Vec<Type>, Diagnostic> {
        let mut bindings = TypeEnv::default();
        for (field, actual) in field_types.iter().zip(actuals) {
            self.unify(ctx, source, field, actual, param_names, &mut bindings)?;
        }
        self.finish_bindings(param_names, &bindings, source, span)
    }

    fn finish_bindings(
        &self,
        param_names: &[String],
        bindings: &TypeEnv,
        source: &SourceFile,
        span: Span,
    ) -> Result<Vec<Type>, Diagnostic> {
        let mut result = Vec::with_capacity(param_names.len());
        for name in param_names {
            match bindings.get(name) {
                Some(ty) => result.push(ty),
                None => {
                    return Err(Diagnostic::at(
                        source,
                        span,
                        format!(
                            "cannot infer type argument `{name}`; specify it explicitly, e.g. `f<...>(...)`"
                        ),
                    ));
                }
            }
        }
        Ok(result)
    }

    #[allow(clippy::too_many_arguments)]
    fn unify(
        &mut self,
        ctx: &MonoContext,
        source: &SourceFile,
        typeref: &TypeRef,
        concrete: &Type,
        param_names: &[String],
        bindings: &mut TypeEnv,
    ) -> Result<(), Diagnostic> {
        match &typeref.kind {
            TypeRefKind::Name { name, arguments } => {
                if arguments.is_empty() && param_names.iter().any(|param| param == name) {
                    if let Some(existing) = bindings.get(name) {
                        if &existing != concrete {
                            return Err(Diagnostic::at(
                                source,
                                typeref.span,
                                format!(
                                    "type parameter `{name}` inferred as both `{existing}` and `{concrete}`"
                                ),
                            ));
                        }
                    } else {
                        bindings.bind(name, concrete.clone());
                    }
                    return Ok(());
                }
                if !arguments.is_empty()
                    && let Some(type_params) = Self::template_type_params(ctx.tables, name)
                {
                    let (key_name, key_args) = match concrete {
                        Type::Struct(id) | Type::Enum(id) => (
                            self.type_keys[id.0].name.clone(),
                            self.type_keys[id.0].args.clone(),
                        ),
                        _ => {
                            return Err(Diagnostic::at(
                                source,
                                typeref.span,
                                format!("expected `{name}`, found `{concrete}`"),
                            ));
                        }
                    };
                    if key_name == *name && key_args.len() == arguments.len() {
                        for (argument, value) in arguments.iter().zip(key_args.iter()) {
                            self.unify(ctx, source, argument, value, param_names, bindings)?;
                        }
                        return Ok(());
                    }
                    let _ = type_params;
                }
                let resolved = self.resolve_type(ctx, source, typeref, bindings)?;
                if &resolved != concrete && !(resolved == Type::Null) {
                    return Err(Diagnostic::at(
                        source,
                        typeref.span,
                        format!("expected `{resolved}`, found `{concrete}`"),
                    ));
                }
                Ok(())
            }
            TypeRefKind::Ptr { pointee, .. } => match concrete {
                Type::Ptr {
                    pointee: actual, ..
                } => self.unify(ctx, source, pointee, actual, param_names, bindings),
                _ => Err(Diagnostic::at(
                    source,
                    typeref.span,
                    format!("expected a pointer, found `{concrete}`"),
                )),
            },
            TypeRefKind::Slice { element, .. } => match concrete {
                Type::Slice {
                    element: actual, ..
                } => self.unify(ctx, source, element, actual, param_names, bindings),
                _ => Err(Diagnostic::at(
                    source,
                    typeref.span,
                    format!("expected a slice, found `{concrete}`"),
                )),
            },
            TypeRefKind::Array { element, .. } => match concrete {
                Type::Array {
                    element: actual, ..
                } => self.unify(
                    ctx,
                    source,
                    element,
                    &actual.as_type(),
                    param_names,
                    bindings,
                ),
                _ => Err(Diagnostic::at(
                    source,
                    typeref.span,
                    format!("expected an array, found `{concrete}`"),
                )),
            },
        }
    }
}

/// 函数实例签名（与旧 `FunctionSignature` 等价）。
#[derive(Clone)]
pub struct Signature {
    pub id: FunctionId,
    pub parameters: Vec<Type>,
    pub return_type: Type,
}

/// 内建标量类型名。
fn builtin_type(name: &str) -> Option<Type> {
    Some(match name {
        "i8" => Type::I8,
        "i16" => Type::I16,
        "i32" => Type::I32,
        "i64" => Type::I64,
        "u8" => Type::U8,
        "u16" => Type::U16,
        "u32" => Type::U32,
        "u64" => Type::U64,
        "usize" => Type::Usize,
        "isize" => Type::Isize,
        "f32" => Type::F32,
        "f64" => Type::F64,
        "char" => Type::Char,
        "bool" => Type::Bool,
        "string" => Type::String,
        "c_int" => Type::I32,
        "c_uint" => Type::U32,
        "c_char" => Type::I8,
        "c_long" => {
            if layout::host_c_long_bytes() == 8 {
                Type::I64
            } else {
                Type::I32
            }
        }
        "c_ulong" => {
            if layout::host_c_long_bytes() == 8 {
                Type::U64
            } else {
                Type::U32
            }
        }
        "Unit" => Type::Unit,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use dolphin_ir::ir::Type;
    use dolphin_package::package::PackageId;

    #[test]
    fn package_identity_separates_instance_keys() {
        let root = GenericKey::in_package(PackageId::ROOT, "m.f", vec![Type::I32]);
        let stdlib = GenericKey::in_package(PackageId::STD, "m.f", vec![Type::I32]);
        assert_ne!(root, stdlib);
        assert_eq!(
            root,
            GenericKey::in_package(PackageId::ROOT, "m.f", vec![Type::I32])
        );
        assert_ne!(
            GenericKey::in_package(PackageId::ROOT, "m.f", vec![Type::I32]),
            GenericKey::in_package(PackageId::ROOT, "m.f", vec![Type::U8])
        );
    }
}
