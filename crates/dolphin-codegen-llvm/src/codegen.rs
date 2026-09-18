//! LLVM 代码生成后端（M16）。
//!
//! ABI 与可观察行为对齐 `dolphin-codegen-cranelift`：函数签名、sret 约定、统一
//! 枚举布局、检查算术与 trap、字符串字面量等都在此以 inkwell 重建。

use std::collections::{HashMap, HashSet};
use std::path::Path;

use dolphin_ir::ir::{
    self, Expr, ExprKind, Instruction, MatchPattern, Place, PlaceKind, PrintPart, ScalarType,
    Terminator, Type, TypeDef,
};
use dolphin_ir::layout;
use dolphin_platform::platform::TargetPlatform;
use dolphin_source::diagnostic::Diagnostic;
use dolphin_syntax::ast::{BinaryOperator, UnaryOperator};

use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::debug_info::{
    AsDIScope, DICompileUnit, DIFile, DIFlags, DIFlagsConstants, DISubprogram, DWARFEmissionKind,
    DWARFSourceLanguage, DebugInfoBuilder,
};
use inkwell::intrinsics::Intrinsic;
use inkwell::module::{FlagBehavior, Linkage, Module};
use inkwell::passes::PassBuilderOptions;
use inkwell::targets::{
    CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetTriple,
};
use inkwell::types::{BasicMetadataTypeEnum, BasicTypeEnum, FloatType, FunctionType, IntType};
use inkwell::values::{
    BasicValue, BasicValueEnum, FunctionValue, GlobalValue, IntValue, PointerValue, ValueKind,
};
use inkwell::{AddressSpace, FloatPredicate, IntPredicate, OptimizationLevel};

pub fn emit_program_optimized(
    program: &ir::Program,
    output: &Path,
    optimize: bool,
    platform: &dyn TargetPlatform,
) -> Result<(), Diagnostic> {
    Target::initialize_native(&InitializationConfig::default())
        .map_err(|error| Diagnostic::plain(format!("could not initialize LLVM: {error}")))?;
    let config = InitializationConfig::default();
    Target::initialize_x86(&config);
    Target::initialize_aarch64(&config);

    let context = Context::create();
    let module = context.create_module("dolphin");
    let triple_text = platform.triple().to_string();
    let triple = TargetTriple::create(&triple_text);
    let target = Target::from_triple(&triple).map_err(|error| {
        Diagnostic::plain(format!("unsupported target `{triple_text}`: {error}"))
    })?;
    let level = if optimize {
        OptimizationLevel::Aggressive
    } else {
        OptimizationLevel::None
    };
    let target_machine = target
        .create_target_machine(&triple, "", "", level, RelocMode::PIC, CodeModel::Default)
        .ok_or_else(|| {
            Diagnostic::plain(format!(
                "could not create target machine for `{triple_text}`"
            ))
        })?;
    module.set_triple(&triple);
    module.set_data_layout(&target_machine.get_target_data().get_data_layout());

    let debug = if optimize {
        None
    } else {
        Some(build_debug_context(&context, &module, program))
    };

    let user_functions = declare_user_functions(&context, &module, program, platform);
    let runtime = declare_runtime_functions(&context, &module, platform);
    let strings = declare_strings(&context, &module, program)?;
    let trap_function = declare_trap(&module)?;

    for function in &program.functions {
        if function.external_link_name.is_some() {
            // extern 函数只有声明：不生成函数体。
            continue;
        }
        define_function(
            &context,
            &module,
            program,
            function,
            &user_functions,
            &runtime,
            &strings,
            trap_function,
            debug.as_ref(),
        )?;
    }

    if let Some(debug) = &debug {
        debug.builder.finalize();
    }
    verify_module(&module, "before optimization")?;

    if optimize {
        module
            .run_passes("default<O2>", &target_machine, PassBuilderOptions::create())
            .map_err(|error| Diagnostic::plain(format!("could not run LLVM passes: {error}")))?;
        verify_module(&module, "after optimization")?;
    }

    target_machine
        .write_to_file(&module, FileType::Object, output)
        .map_err(|error| {
            Diagnostic::plain(format!(
                "could not write object file `{}`: {error}",
                output.display()
            ))
        })?;
    Ok(())
}

/// 把限定名转成链接器安全的符号片段：非字母数字下划线替换为 `_`。
fn sanitize_symbol_component(name: &str) -> String {
    name.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn declare_trap<'ctx>(module: &Module<'ctx>) -> Result<FunctionValue<'ctx>, Diagnostic> {
    Intrinsic::find("llvm.trap")
        .and_then(|intrinsic| intrinsic.get_declaration(module, &[]))
        .ok_or_else(|| Diagnostic::plain("could not declare `llvm.trap`"))
}

/// Debug 构建的 DWARF 上下文；Release 不构建。
struct DebugContext<'ctx> {
    builder: DebugInfoBuilder<'ctx>,
    compile_unit: DICompileUnit<'ctx>,
    files: Vec<DIFile<'ctx>>,
}

fn split_source_path(path: Option<&Path>) -> (String, String) {
    let filename = path
        .and_then(|path| path.file_name())
        .and_then(|name| name.to_str())
        .unwrap_or("unknown")
        .to_string();
    let directory = path
        .and_then(|path| path.parent())
        .and_then(|dir| dir.to_str())
        .filter(|dir| !dir.is_empty())
        .unwrap_or(".")
        .to_string();
    (filename, directory)
}

fn build_debug_context<'ctx>(
    context: &'ctx Context,
    module: &Module<'ctx>,
    program: &ir::Program,
) -> DebugContext<'ctx> {
    let info_version = context.i32_type().const_int(3, false);
    module.add_basic_value_flag("Debug Info Version", FlagBehavior::Warning, info_version);
    let dwarf_version = context.i32_type().const_int(5, false);
    module.add_basic_value_flag("Dwarf Version", FlagBehavior::Warning, dwarf_version);

    let (filename, directory) =
        split_source_path(program.sources.first().map(|path| path.as_path()));
    let (builder, compile_unit) = module.create_debug_info_builder(
        true,
        DWARFSourceLanguage::C99,
        &filename,
        &directory,
        "dolphin-compiler",
        false,
        "",
        0,
        "",
        DWARFEmissionKind::Full,
        0,
        false,
        false,
        "",
        "",
    );
    let files = program
        .sources
        .iter()
        .map(|path| {
            let (filename, directory) = split_source_path(Some(path.as_path()));
            builder.create_file(&filename, &directory)
        })
        .collect();
    DebugContext {
        builder,
        compile_unit,
        files,
    }
}

fn create_subprogram<'ctx>(
    debug: &DebugContext<'ctx>,
    function_value: FunctionValue<'ctx>,
    function: &ir::Function,
) -> DISubprogram<'ctx> {
    let file = debug
        .files
        .get(function.source as usize)
        .copied()
        .unwrap_or_else(|| debug.compile_unit.get_file());
    let line = function.location.line.max(1);
    let subroutine = debug
        .builder
        .create_subroutine_type(file, None, &[], DIFlags::ZERO);
    let linkage = function_value.get_name().to_string_lossy().into_owned();
    let subprogram = debug.builder.create_function(
        debug.compile_unit.as_debug_info_scope(),
        &function.name,
        Some(&linkage),
        file,
        line,
        subroutine,
        false,
        true,
        line,
        DIFlags::ZERO,
        false,
    );
    function_value.set_subprogram(subprogram);
    subprogram
}

fn set_debug_location<'ctx>(
    context: &'ctx Context,
    builder: &Builder<'ctx>,
    debug: &DebugContext<'ctx>,
    subprogram: DISubprogram<'ctx>,
    location: ir::Location,
) {
    if location.line == 0 {
        return;
    }
    let location = debug.builder.create_debug_location(
        context,
        location.line,
        location.column,
        subprogram.as_debug_info_scope(),
        None,
    );
    builder.set_current_debug_location(location);
}

fn declare_user_functions<'ctx>(
    context: &'ctx Context,
    module: &Module<'ctx>,
    program: &ir::Program,
    platform: &dyn TargetPlatform,
) -> Vec<FunctionValue<'ctx>> {
    program
        .functions
        .iter()
        .map(|function| {
            let is_main = program.main == Some(function.id);
            let signature = function_type(context, function, is_main, &program.types);
            let symbol = if is_main {
                // `main` 跨越 C 边界：CRT 按平台 C 符号规则查找入口。
                platform.c_symbol("main")
            } else if let Some(link_name) = &function.external_link_name {
                // extern "C" 函数按原符号导入，不加 Dolphin 包 mangling。
                platform.c_symbol(link_name)
            } else {
                format!(
                    "__dolphin_fn_{}_{}",
                    function.id.0,
                    sanitize_symbol_component(&function.name)
                )
            };
            module.add_function(&symbol, signature, Some(Linkage::External))
        })
        .collect()
}

struct RuntimeRefs<'ctx> {
    print_i32: FunctionValue<'ctx>,
    print_i64: FunctionValue<'ctx>,
    print_u64: FunctionValue<'ctx>,
    print_f32: FunctionValue<'ctx>,
    print_f64: FunctionValue<'ctx>,
    print_char: FunctionValue<'ctx>,
    print_bool: FunctionValue<'ctx>,
    print_string: FunctionValue<'ctx>,
    string_equal: FunctionValue<'ctx>,
    alloc: FunctionValue<'ctx>,
    free: FunctionValue<'ctx>,
    copy: FunctionValue<'ctx>,
    is_valid_utf8: FunctionValue<'ctx>,
    check_utf8: FunctionValue<'ctx>,
    check_align: FunctionValue<'ctx>,
    check_view: FunctionValue<'ctx>,
    finish: FunctionValue<'ctx>,
}

fn declare_runtime_functions<'ctx>(
    context: &'ctx Context,
    module: &Module<'ctx>,
    platform: &dyn TargetPlatform,
) -> RuntimeRefs<'ctx> {
    let void = context.void_type();
    let i8_type = context.i8_type();
    let i32_type = context.i32_type();
    let i64_type = context.i64_type();
    let f32_type = context.f32_type();
    let f64_type = context.f64_type();
    let pointer = context.ptr_type(AddressSpace::default());
    // 运行时函数跨越 C 边界：`size_t` / `uintptr_t` 按 64 位整数传参。
    let void0 = void.fn_type(&[], false);
    let void1_i32 = void.fn_type(&[i32_type.into()], false);
    let void1_i64 = void.fn_type(&[i64_type.into()], false);
    let void1_f32 = void.fn_type(&[f32_type.into()], false);
    let void1_f64 = void.fn_type(&[f64_type.into()], false);
    let void1_i8 = void.fn_type(&[i8_type.into()], false);
    let void2_ptr_i64 = void.fn_type(&[pointer.into(), i64_type.into()], false);
    let string_equal = i8_type.fn_type(
        &[
            pointer.into(),
            i64_type.into(),
            pointer.into(),
            i64_type.into(),
        ],
        false,
    );
    let alloc = pointer.fn_type(&[i64_type.into(), i64_type.into(), i64_type.into()], false);
    let is_valid_utf8 = i8_type.fn_type(&[pointer.into(), i64_type.into()], false);
    let declare = |name: &str, signature: FunctionType<'ctx>| {
        module.add_function(&platform.c_symbol(name), signature, Some(Linkage::External))
    };
    RuntimeRefs {
        print_i32: declare("dolphin_print_i32", void1_i32),
        print_i64: declare("dolphin_print_i64", void1_i64),
        print_u64: declare("dolphin_print_u64", void1_i64),
        print_f32: declare("dolphin_print_f32", void1_f32),
        print_f64: declare("dolphin_print_f64", void1_f64),
        print_char: declare("dolphin_print_char", void1_i32),
        print_bool: declare("dolphin_print_bool", void1_i8),
        print_string: declare("dolphin_print_string", void2_ptr_i64),
        string_equal: declare("dolphin_string_equal", string_equal),
        alloc: declare("dolphin_alloc", alloc),
        free: declare(
            "dolphin_free",
            void.fn_type(&[pointer.into(), i64_type.into()], false),
        ),
        copy: declare(
            "dolphin_copy",
            void.fn_type(&[pointer.into(), pointer.into(), i64_type.into()], false),
        ),
        is_valid_utf8: declare("dolphin_is_valid_utf8", is_valid_utf8),
        check_utf8: declare("dolphin_check_utf8", void2_ptr_i64),
        check_align: declare("dolphin_check_align", void2_ptr_i64),
        check_view: declare("dolphin_check_view", void2_ptr_i64),
        finish: declare("dolphin_runtime_finish", void0),
    }
}

fn declare_strings<'ctx>(
    context: &'ctx Context,
    module: &Module<'ctx>,
    program: &ir::Program,
) -> Result<HashMap<String, GlobalValue<'ctx>>, Diagnostic> {
    let mut values = HashSet::new();
    for function in &program.functions {
        for block in &function.blocks {
            for instruction in &block.instructions {
                match instruction {
                    Instruction::SetLocal { value, .. } | Instruction::Evaluate(value) => {
                        collect_expr_strings(value, &mut values)
                    }
                    Instruction::SetFieldAt { place, value, .. } => {
                        collect_place_strings(place, &mut values);
                        collect_expr_strings(value, &mut values);
                    }
                    Instruction::SetIndexAt { place, value } => {
                        collect_place_strings(place, &mut values);
                        collect_expr_strings(value, &mut values);
                    }
                    Instruction::Print(parts) => {
                        for part in parts {
                            match part {
                                PrintPart::Text(text) => {
                                    values.insert(text.clone());
                                }
                                PrintPart::Value(value) => collect_expr_strings(value, &mut values),
                            }
                        }
                    }
                }
            }
            match &block.terminator {
                Terminator::Branch { condition, .. } => {
                    collect_expr_strings(condition, &mut values)
                }
                Terminator::Return(Some(value)) => collect_expr_strings(value, &mut values),
                Terminator::Jump(_) | Terminator::Return(None) => {}
            }
        }
    }

    let mut strings = HashMap::new();
    for (index, value) in values.into_iter().enumerate() {
        let bytes = if value.is_empty() {
            vec![0]
        } else {
            value.as_bytes().to_vec()
        };
        let initializer = context.const_string(&bytes, false);
        let global = module.add_global(
            initializer.get_type(),
            None,
            &format!("__dolphin_string_{index}"),
        );
        global.set_initializer(&initializer);
        global.set_constant(true);
        global.set_linkage(Linkage::Private);
        global.set_unnamed_addr(true);
        global.set_alignment(1);
        strings.insert(value, global);
    }
    Ok(strings)
}

fn collect_expr_strings(expression: &Expr, values: &mut HashSet<String>) {
    match &expression.kind {
        ExprKind::String(value) => {
            values.insert(value.clone());
        }
        ExprKind::Call { arguments, .. } => {
            for argument in arguments {
                collect_expr_strings(argument, values);
            }
        }
        ExprKind::Unary { operand, .. } => collect_expr_strings(operand, values),
        ExprKind::Array(elements) => {
            for element in elements {
                collect_expr_strings(element, values);
            }
        }
        ExprKind::RepeatArray { value, .. } => collect_expr_strings(value, values),
        ExprKind::Index { array, index } => {
            collect_expr_strings(array, values);
            collect_expr_strings(index, values);
        }
        ExprKind::Binary { left, right, .. } => {
            collect_expr_strings(left, values);
            collect_expr_strings(right, values);
        }
        ExprKind::Cast { value, .. } => collect_expr_strings(value, values),
        ExprKind::StringLength(value) => collect_expr_strings(value, values),
        ExprKind::StructInit { fields } => {
            for field in fields {
                collect_expr_strings(field, values);
            }
        }
        ExprKind::EnumInit { arguments, .. } => {
            for argument in arguments {
                collect_expr_strings(argument, values);
            }
        }
        ExprKind::Field { base, .. } => collect_expr_strings(base, values),
        ExprKind::AddressOf { place } => collect_place_strings(place, values),
        ExprKind::Deref { pointer } => collect_expr_strings(pointer, values),
        ExprKind::SlicePtr { base } | ExprKind::SliceLen { base } => {
            collect_expr_strings(base, values)
        }
        ExprKind::SliceRange { base, start, end } => {
            collect_expr_strings(base, values);
            collect_expr_strings(start, values);
            collect_expr_strings(end, values);
        }
        ExprKind::MemAlloc { count, .. } => collect_expr_strings(count, values),
        ExprKind::MemFree { buffer, .. } => collect_expr_strings(buffer, values),
        ExprKind::MemCreate { value, .. } => collect_expr_strings(value, values),
        ExprKind::MemDestroy { pointer, .. } => collect_expr_strings(pointer, values),
        ExprKind::MemCopy { dst, src, .. } => {
            collect_expr_strings(dst, values);
            collect_expr_strings(src, values);
        }
        ExprKind::MemIsValidUtf8 { bytes } => collect_expr_strings(bytes, values),
        ExprKind::MemView { pointer, len, .. } => {
            collect_expr_strings(pointer, values);
            collect_expr_strings(len, values);
        }
        ExprKind::MemCast { pointer } => collect_expr_strings(pointer, values),
        ExprKind::StringBytes { base } => collect_expr_strings(base, values),
        ExprKind::StringFromBytes { bytes } => collect_expr_strings(bytes, values),
        ExprKind::Match { value, arms } => {
            collect_expr_strings(value, values);
            for arm in arms {
                collect_expr_strings(&arm.body, values);
            }
        }
        ExprKind::EnumIsVariant { value, .. } | ExprKind::EnumPayload { value, .. } => {
            collect_expr_strings(value, values)
        }
        ExprKind::Integer(_)
        | ExprKind::Float(_)
        | ExprKind::Char(_)
        | ExprKind::Bool(_)
        | ExprKind::Null
        | ExprKind::Local(_) => {}
    }
}

fn collect_place_strings(place: &Place, values: &mut HashSet<String>) {
    match &place.kind {
        PlaceKind::Local(_) => {}
        PlaceKind::Field { base, .. } => collect_place_strings(base, values),
        PlaceKind::Index { base, index } => {
            collect_place_strings(base, values);
            collect_expr_strings(index, values);
        }
        PlaceKind::Deref { pointer } => collect_expr_strings(pointer, values),
    }
}

/// 统一布局中的一个运行时分量：LLVM 标量类型与字节偏移。
#[derive(Clone, Copy, PartialEq, Eq)]
enum Scalar {
    I8,
    I16,
    I32,
    I64,
    F32,
    F64,
    Ptr,
}

impl Scalar {
    fn basic<'ctx>(self, context: &'ctx Context) -> BasicTypeEnum<'ctx> {
        match self {
            Scalar::I8 => context.i8_type().into(),
            Scalar::I16 => context.i16_type().into(),
            Scalar::I32 => context.i32_type().into(),
            Scalar::I64 => context.i64_type().into(),
            Scalar::F32 => context.f32_type().into(),
            Scalar::F64 => context.f64_type().into(),
            Scalar::Ptr => context.ptr_type(AddressSpace::default()).into(),
        }
    }

    fn bytes(self) -> u32 {
        match self {
            Scalar::I8 => 1,
            Scalar::I16 => 2,
            Scalar::I32 | Scalar::F32 => 4,
            Scalar::I64 | Scalar::F64 | Scalar::Ptr => 8,
        }
    }

    fn bits(self) -> u32 {
        self.bytes() * 8
    }

    fn int_type<'ctx>(self, context: &'ctx Context) -> IntType<'ctx> {
        match self {
            Scalar::I8 => context.i8_type(),
            Scalar::I16 => context.i16_type(),
            Scalar::I32 => context.i32_type(),
            Scalar::I64 | Scalar::Ptr => context.i64_type(),
            _ => unreachable!("float scalar has no integer type"),
        }
    }

    fn float_type<'ctx>(self, context: &'ctx Context) -> FloatType<'ctx> {
        match self {
            Scalar::F32 => context.f32_type(),
            Scalar::F64 => context.f64_type(),
            _ => unreachable!("integer scalar has no float type"),
        }
    }
}

fn scalar_of(ty: &Type) -> Scalar {
    match ty {
        Type::I8 | Type::U8 | Type::Bool => Scalar::I8,
        Type::I16 | Type::U16 => Scalar::I16,
        Type::I32 | Type::U32 | Type::Char => Scalar::I32,
        Type::I64 | Type::U64 | Type::Usize | Type::Isize => Scalar::I64,
        Type::F32 => Scalar::F32,
        Type::F64 => Scalar::F64,
        Type::String | Type::Ptr { .. } | Type::Slice { .. } | Type::Null => Scalar::Ptr,
        Type::Unit | Type::Array { .. } | Type::Struct(_) | Type::Enum(_) => {
            unreachable!("type has no single runtime value")
        }
    }
}

/// 类型扁平展开后的分量布局（与 Cranelift 后端一致），字面量长度用 I64 分量。
fn component_layout(ty: &Type, types: &[TypeDef]) -> Vec<(Scalar, u32)> {
    match ty {
        Type::Struct(id) => {
            let mut layout = Vec::new();
            if let TypeDef::Struct { fields, .. } = &types[id.0] {
                for (index, field) in fields.iter().enumerate() {
                    let base = layout::field_offset(fields, index, types, layout::POINTER_BYTES);
                    for (component_type, component_offset) in component_layout(&field.ty, types) {
                        layout.push((component_type, base + component_offset));
                    }
                }
            }
            layout
        }
        Type::Enum(id) => {
            // 枚举：一个 I32 标签 + payload 区，payload 统一用 I64 分量。
            // 偏移量来自公共布局，不在后端保留 `4 + ...` 常量。
            let enum_layout = layout::enum_layout_of(types, *id, layout::POINTER_BYTES);
            let mut layout = vec![(Scalar::I32, enum_layout.tag_offset)];
            for index in 0..enum_layout.payload_components {
                let offset =
                    enum_layout.payload_offset + index * layout::ENUM_PAYLOAD_COMPONENT_BYTES;
                layout.push((Scalar::I64, offset));
            }
            layout
        }
        Type::Ptr { .. } => vec![(Scalar::Ptr, 0)],
        Type::Slice { .. } => vec![(Scalar::Ptr, 0), (Scalar::I64, layout::POINTER_BYTES)],
        Type::Null => vec![(Scalar::Ptr, 0)],
        _ => scalar_component_layout(ty),
    }
}

fn scalar_component_layout(ty: &Type) -> Vec<(Scalar, u32)> {
    let (element, length) = match ty {
        ty if ty.is_integer() || ty.is_float() || *ty == Type::Char => (ty.as_scalar().unwrap(), 1),
        Type::Bool => (ScalarType::Bool, 1),
        Type::String => (ScalarType::String, 1),
        Type::Array { element, length } => (*element, *length),
        Type::Unit => unreachable!("Unit has no runtime value"),
        Type::Struct(_) | Type::Enum(_) => {
            unreachable!("user types handled by component_layout")
        }
        _ => unreachable!("all scalar types are covered"),
    };
    let pointer_bytes = layout::POINTER_BYTES;
    let (component_types, stride): (&[Scalar], u32) = match element {
        scalar
            if scalar.as_type().is_integer()
                || scalar.as_type().is_float()
                || scalar == ScalarType::Char =>
        {
            let ty = scalar_of(&scalar.as_type());
            let stride = ty.bytes().max(1);
            return (0..length)
                .map(|index| (ty, index as u32 * stride))
                .collect();
        }
        ScalarType::Bool => (&[Scalar::I8], 1),
        ScalarType::String => (&[Scalar::Ptr, Scalar::I64], pointer_bytes * 2),
        _ => unreachable!("all scalar types are covered"),
    };
    let mut layout = Vec::with_capacity(component_types.len() * length);
    for index in 0..length {
        for (component, component_type) in component_types.iter().enumerate() {
            let offset = index as u32 * stride + component as u32 * pointer_bytes;
            layout.push((*component_type, offset));
        }
    }
    layout
}

/// 计算一个类型扁平展开后的组件个数。
fn abi_width(ty: &Type, types: &[TypeDef]) -> usize {
    layout::component_count(ty, types, layout::POINTER_BYTES) as usize
}

/// ABI 分量展开（与 Cranelift 的 `append_abi_type` 一致）。
fn abi_components<'ctx>(
    context: &'ctx Context,
    ty: &Type,
    types: &[TypeDef],
) -> Vec<BasicTypeEnum<'ctx>> {
    let pointer = context.ptr_type(AddressSpace::default());
    match ty {
        Type::Unit => Vec::new(),
        ty if ty.is_integer() || ty.is_float() || *ty == Type::Char => {
            vec![scalar_of(ty).basic(context)]
        }
        Type::Bool => vec![context.i8_type().into()],
        Type::String => vec![pointer.into(), context.i64_type().into()],
        Type::Array { element, length } => {
            let element = abi_components(context, &element.as_type(), types);
            let mut components = Vec::with_capacity(element.len() * length);
            for _ in 0..*length {
                components.extend(element.iter().copied());
            }
            components
        }
        Type::Struct(_) | Type::Enum(_) | Type::Ptr { .. } | Type::Slice { .. } => {
            component_layout(ty, types)
                .into_iter()
                .map(|(component_type, _)| component_type.basic(context))
                .collect()
        }
        _ => unreachable!("all scalar ABI types are covered by the guard"),
    }
}

#[derive(Clone, Copy)]
struct FunctionShape {
    /// 返回类型是否走 sret 指针（而非寄存器）。
    uses_sret: bool,
}

fn function_shape(function: &ir::Function, is_main: bool, types: &[TypeDef]) -> FunctionShape {
    // main 返回 i32，永不使用 sret。
    let uses_sret = !is_main && abi_width(&function.return_type, types) > 2;
    FunctionShape { uses_sret }
}

fn basic_fn_type<'ctx>(
    return_type: BasicTypeEnum<'ctx>,
    params: &[BasicMetadataTypeEnum<'ctx>],
) -> FunctionType<'ctx> {
    match return_type {
        BasicTypeEnum::ArrayType(ty) => ty.fn_type(params, false),
        BasicTypeEnum::FloatType(ty) => ty.fn_type(params, false),
        BasicTypeEnum::IntType(ty) => ty.fn_type(params, false),
        BasicTypeEnum::PointerType(ty) => ty.fn_type(params, false),
        BasicTypeEnum::StructType(ty) => ty.fn_type(params, false),
        BasicTypeEnum::VectorType(ty) => ty.fn_type(params, false),
        BasicTypeEnum::ScalableVectorType(ty) => ty.fn_type(params, false),
    }
}

fn function_type<'ctx>(
    context: &'ctx Context,
    function: &ir::Function,
    is_main: bool,
    types: &[TypeDef],
) -> FunctionType<'ctx> {
    let pointer = context.ptr_type(AddressSpace::default());
    if is_main {
        return context
            .i32_type()
            .fn_type(&[context.i32_type().into(), pointer.into()], false);
    }
    let shape = function_shape(function, is_main, types);
    let mut params: Vec<BasicMetadataTypeEnum> = Vec::new();
    if shape.uses_sret {
        // 隐藏的 sret 指针参数（第 0 个）；此时函数返回 void。
        params.push(pointer.into());
    }
    for parameter in &function.parameters {
        for component in abi_components(context, &function.locals[parameter.0], types) {
            params.push(component.into());
        }
    }
    if shape.uses_sret {
        context.void_type().fn_type(&params, false)
    } else {
        match abi_width(&function.return_type, types) {
            0 => context.void_type().fn_type(&params, false),
            1 => {
                let component = abi_components(context, &function.return_type, types)[0];
                basic_fn_type(component, &params)
            }
            2 => {
                let components = abi_components(context, &function.return_type, types);
                let result = context.struct_type(&components, false);
                result.fn_type(&params, false)
            }
            _ => unreachable!("sret covers return widths above two"),
        }
    }
}

#[derive(Clone)]
struct LocalSlot<'ctx> {
    slot: PointerValue<'ctx>,
    ty: Type,
}

#[derive(Clone)]
struct RuntimeValue<'ctx> {
    ty: Type,
    values: Vec<BasicValueEnum<'ctx>>,
}

impl<'ctx> RuntimeValue<'ctx> {
    fn scalar(ty: Type, value: BasicValueEnum<'ctx>) -> Self {
        Self {
            ty,
            values: vec![value],
        }
    }

    fn from_slice(ty: Type, values: &[BasicValueEnum<'ctx>]) -> Self {
        Self {
            ty,
            values: values.to_vec(),
        }
    }

    fn one(&self) -> BasicValueEnum<'ctx> {
        debug_assert_eq!(self.values.len(), 1);
        self.values[0]
    }
}

fn builder_ok<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
    result.expect("LLVM builder operation failed on an internal invariant")
}

fn size_of_type(ty: &Type, types: &[TypeDef]) -> u32 {
    layout::layout_of(ty, types, layout::POINTER_BYTES).size
}

fn create_alloca<'ctx>(
    context: &'ctx Context,
    builder: &Builder<'ctx>,
    size: u32,
    align: u32,
    name: &str,
) -> PointerValue<'ctx> {
    let element = context.i8_type().array_type(size.max(1));
    let pointer = builder_ok(builder.build_alloca(element, name));
    if let Some(instruction) = pointer.as_instruction() {
        builder_ok(instruction.set_alignment(align.max(1)));
    }
    pointer
}

/// 每个调用点一个入口块静态槽：避免循环内 alloca 随迭代累积栈。
///
/// 重新定位 builder 会清空当前调试位置（否则调用指令会缺 `!dbg`），
/// 因此这里先缓存并临时清除位置，构建后恢复插入点与调试位置。
fn create_entry_alloca<'ctx>(
    context: &'ctx Context,
    builder: &Builder<'ctx>,
    function: FunctionValue<'ctx>,
    size: u32,
    align: u32,
    name: &str,
) -> PointerValue<'ctx> {
    let saved_block = builder.get_insert_block();
    let saved_location = builder.get_current_debug_location();
    builder.unset_current_debug_location();
    let entry = function
        .get_first_basic_block()
        .expect("defined function has an entry block");
    match entry.get_first_instruction() {
        Some(instruction) => builder.position_before(&instruction),
        None => builder.position_at_end(entry),
    }
    let pointer = create_alloca(context, builder, size, align, name);
    if let Some(block) = saved_block {
        builder.position_at_end(block);
    }
    if let Some(location) = saved_location {
        builder.set_current_debug_location(location);
    }
    pointer
}

fn create_local_slot<'ctx>(
    context: &'ctx Context,
    builder: &Builder<'ctx>,
    ty: &Type,
    types: &[TypeDef],
) -> LocalSlot<'ctx> {
    // 使用统一布局的字节大小（含结构体末尾对齐填充），而非分量最大偏移。
    let layout = layout::layout_of(ty, types, layout::POINTER_BYTES);
    LocalSlot {
        slot: create_alloca(context, builder, layout.size, layout.align, "slot"),
        ty: ty.clone(),
    }
}

fn gep<'ctx>(
    context: &'ctx Context,
    builder: &Builder<'ctx>,
    pointer: PointerValue<'ctx>,
    offset: u32,
) -> PointerValue<'ctx> {
    let index = context.i64_type().const_int(offset as u64, false);
    unsafe { builder.build_gep(context.i8_type(), pointer, &[index], "gep") }
        .expect("LLVM builder operation failed on an internal invariant")
}

fn gep_dynamic<'ctx>(
    context: &'ctx Context,
    builder: &Builder<'ctx>,
    pointer: PointerValue<'ctx>,
    offset: IntValue<'ctx>,
) -> PointerValue<'ctx> {
    let offset = if offset.get_type().get_bit_width() == 64 {
        offset
    } else {
        builder_ok(builder.build_int_z_extend(offset, context.i64_type(), "offset"))
    };
    unsafe { builder.build_gep(context.i8_type(), pointer, &[offset], "gep") }
        .expect("LLVM builder operation failed on an internal invariant")
}

fn store_local<'ctx>(
    context: &'ctx Context,
    builder: &Builder<'ctx>,
    slot: &LocalSlot<'ctx>,
    value: RuntimeValue<'ctx>,
    types: &[TypeDef],
) {
    debug_assert_eq!(slot.ty, value.ty);
    let layout = component_layout(&slot.ty, types);
    debug_assert_eq!(layout.len(), value.values.len());
    for ((_, offset), component) in layout.into_iter().zip(value.values) {
        let address = gep(context, builder, slot.slot, offset);
        builder_ok(builder.build_store(address, component));
    }
}

fn load_local<'ctx>(
    context: &'ctx Context,
    builder: &Builder<'ctx>,
    slot: &LocalSlot<'ctx>,
    types: &[TypeDef],
) -> RuntimeValue<'ctx> {
    let values = component_layout(&slot.ty, types)
        .into_iter()
        .map(|(component_type, offset)| {
            let address = gep(context, builder, slot.slot, offset);
            builder_ok(builder.build_load(component_type.basic(context), address, "load"))
        })
        .collect();
    RuntimeValue {
        ty: slot.ty.clone(),
        values,
    }
}

#[allow(clippy::too_many_arguments)]
fn define_function<'ctx>(
    context: &'ctx Context,
    module: &Module<'ctx>,
    program: &ir::Program,
    function: &ir::Function,
    user_functions: &[FunctionValue<'ctx>],
    runtime: &RuntimeRefs<'ctx>,
    strings: &HashMap<String, GlobalValue<'ctx>>,
    trap_function: FunctionValue<'ctx>,
    debug: Option<&DebugContext<'ctx>>,
) -> Result<(), Diagnostic> {
    let is_main = program.main == Some(function.id);
    let function_value = user_functions[function.id.0];
    let builder = context.create_builder();

    // 第一个基本块用于存放 alloca，随后跳转到 IR 入口块。
    let alloca_block = context.append_basic_block(function_value, "alloca");
    let trap_block = context.append_basic_block(function_value, "trap");
    let blocks: Vec<inkwell::basic_block::BasicBlock> = function
        .blocks
        .iter()
        .map(|_| context.append_basic_block(function_value, "bb"))
        .collect();

    let subprogram = debug.map(|debug| create_subprogram(debug, function_value, function));

    builder.position_at_end(alloca_block);
    let slots: Vec<LocalSlot> = function
        .locals
        .iter()
        .map(|ty| create_local_slot(context, &builder, ty, &program.types))
        .collect();
    builder_ok(builder.build_unconditional_branch(blocks[function.entry.0]));

    let entry = blocks[function.entry.0];
    builder.position_at_end(entry);
    let sret_param = initialize_parameters(
        context,
        &builder,
        function_value,
        function,
        &slots,
        &program.types,
    );

    let emitter = Emitter {
        context,
        module,
        builder: &builder,
        function: function_value,
        trap_block,
        slots: &slots,
        user_functions,
        runtime,
        strings,
        sret_param,
        types: &program.types,
        is_main,
    };

    for (index, ir_block) in function.blocks.iter().enumerate() {
        builder.position_at_end(blocks[index]);
        for (instruction_index, instruction) in ir_block.instructions.iter().enumerate() {
            if let (Some(debug), Some(subprogram)) = (debug, subprogram) {
                let location = ir_block
                    .locations
                    .get(instruction_index)
                    .copied()
                    .unwrap_or(ir_block.location);
                set_debug_location(context, &builder, debug, subprogram, location);
            }
            emitter.emit_instruction(instruction);
        }
        if let (Some(debug), Some(subprogram)) = (debug, subprogram) {
            set_debug_location(
                context,
                &builder,
                debug,
                subprogram,
                ir_block.terminator_location,
            );
        }
        emitter.emit_terminator(&ir_block.terminator, &blocks);
    }

    builder.position_at_end(trap_block);
    emitter.call(trap_function, &[]);
    builder_ok(builder.build_unreachable());
    Ok(())
}

fn initialize_parameters<'ctx>(
    context: &'ctx Context,
    builder: &Builder<'ctx>,
    function_value: FunctionValue<'ctx>,
    function: &ir::Function,
    slots: &[LocalSlot<'ctx>],
    types: &[TypeDef],
) -> Option<PointerValue<'ctx>> {
    let values: Vec<BasicValueEnum> = (0..function_value.count_params())
        .map(|index| {
            function_value
                .get_nth_param(index)
                .expect("parameter index is in range")
        })
        .collect();
    let shape = function_shape(function, false, types);
    let mut index = 0;
    // sret 模式下第 0 个参数是隐藏的返回缓冲区指针，不属于任何局部变量。
    let sret_param = if shape.uses_sret {
        let value = values[index].into_pointer_value();
        index += 1;
        Some(value)
    } else {
        None
    };
    for local in &function.parameters {
        let width = abi_width(&slots[local.0].ty, types);
        let value =
            RuntimeValue::from_slice(slots[local.0].ty.clone(), &values[index..index + width]);
        store_local(context, builder, &slots[local.0], value, types);
        index += width;
    }
    sret_param
}

enum CheckedOp {
    Add,
    Subtract,
    Multiply,
}

struct Emitter<'ctx, 'a> {
    context: &'ctx Context,
    module: &'a Module<'ctx>,
    builder: &'a Builder<'ctx>,
    function: FunctionValue<'ctx>,
    trap_block: inkwell::basic_block::BasicBlock<'ctx>,
    slots: &'a [LocalSlot<'ctx>],
    user_functions: &'a [FunctionValue<'ctx>],
    runtime: &'a RuntimeRefs<'ctx>,
    strings: &'a HashMap<String, GlobalValue<'ctx>>,
    sret_param: Option<PointerValue<'ctx>>,
    types: &'a [TypeDef],
    is_main: bool,
}

impl<'ctx> Emitter<'ctx, '_> {
    fn append_block(&self, name: &str) -> inkwell::basic_block::BasicBlock<'ctx> {
        self.context.append_basic_block(self.function, name)
    }

    fn i8_to_bool(&self, value: IntValue<'ctx>) -> IntValue<'ctx> {
        let zero = value.get_type().const_zero();
        builder_ok(
            self.builder
                .build_int_compare(IntPredicate::NE, value, zero, "to_bool"),
        )
    }

    fn bool_to_i8(&self, value: IntValue<'ctx>) -> IntValue<'ctx> {
        builder_ok(
            self.builder
                .build_int_z_extend(value, self.context.i8_type(), "to_i8"),
        )
    }

    fn trap_if(&self, condition: IntValue<'ctx>) {
        let cont = self.append_block("cont");
        builder_ok(
            self.builder
                .build_conditional_branch(condition, self.trap_block, cont),
        );
        self.builder.position_at_end(cont);
    }

    fn call(
        &self,
        function: FunctionValue<'ctx>,
        arguments: &[BasicValueEnum<'ctx>],
    ) -> Option<BasicValueEnum<'ctx>> {
        let arguments: Vec<inkwell::values::BasicMetadataValueEnum> =
            arguments.iter().map(|value| (*value).into()).collect();
        let call = builder_ok(self.builder.build_call(function, &arguments, "call"));
        match call.try_as_basic_value() {
            ValueKind::Basic(value) => Some(value),
            ValueKind::Instruction(_) => None,
        }
    }

    fn emit_runtime_finish(&self) {
        if self.is_main {
            self.call(self.runtime.finish, &[]);
        }
    }

    fn emit_instruction(&self, instruction: &Instruction) {
        match instruction {
            Instruction::SetLocal { local, value } => {
                let value = self.emit_expr(value);
                store_local(
                    self.context,
                    self.builder,
                    &self.slots[local.0],
                    value,
                    self.types,
                );
            }
            Instruction::SetFieldAt {
                place,
                field,
                value,
            } => {
                let base_address = self.emit_place_address(place);
                let Type::Struct(id) = &place.ty else {
                    unreachable!("SetFieldAt targets a struct place")
                };
                let TypeDef::Struct { fields, .. } = &self.types[id.0] else {
                    unreachable!("field write resolves a struct type");
                };
                let offset =
                    layout::field_offset(fields, *field, self.types, layout::POINTER_BYTES);
                let value = self.emit_expr(value);
                let field_ty = fields[*field].ty.clone();
                for ((_, component_offset), component) in component_layout(&field_ty, self.types)
                    .into_iter()
                    .zip(value.values)
                {
                    let address = gep(
                        self.context,
                        self.builder,
                        base_address,
                        offset + component_offset,
                    );
                    builder_ok(self.builder.build_store(address, component));
                }
            }
            Instruction::SetIndexAt { place, value } => {
                let address = self.emit_place_address(place);
                let value = self.emit_expr(value);
                for ((_, offset), component) in component_layout(&place.ty, self.types)
                    .into_iter()
                    .zip(value.values)
                {
                    let component_address = gep(self.context, self.builder, address, offset);
                    builder_ok(self.builder.build_store(component_address, component));
                }
            }
            Instruction::Evaluate(value) => {
                self.emit_expr(value);
            }
            Instruction::Print(parts) => {
                for part in parts {
                    let value = match part {
                        PrintPart::Text(text) => self.emit_string(text),
                        PrintPart::Value(value) => self.emit_expr(value),
                    };
                    let (function, arguments) = if value.ty == Type::I32 {
                        (self.runtime.print_i32, value.values)
                    } else if value.ty.is_signed_integer() {
                        let extended = self.extend_integer(
                            value.one().into_int_value(),
                            true,
                            self.context.i64_type(),
                        );
                        (self.runtime.print_i64, vec![extended.into()])
                    } else if value.ty.is_integer() {
                        let extended = self.extend_integer(
                            value.one().into_int_value(),
                            false,
                            self.context.i64_type(),
                        );
                        (self.runtime.print_u64, vec![extended.into()])
                    } else {
                        match value.ty {
                            Type::F32 => (self.runtime.print_f32, value.values),
                            Type::F64 => (self.runtime.print_f64, value.values),
                            Type::Char => (self.runtime.print_char, value.values),
                            Type::Bool => (self.runtime.print_bool, value.values),
                            Type::String => (self.runtime.print_string, value.values),
                            Type::Unit | Type::Array { .. } => {
                                unreachable!("value cannot be printed")
                            }
                            _ => unreachable!(),
                        }
                    };
                    self.call(function, &arguments);
                }
            }
        }
    }

    fn emit_terminator(
        &self,
        terminator: &Terminator,
        blocks: &[inkwell::basic_block::BasicBlock<'ctx>],
    ) {
        match terminator {
            Terminator::Jump(target) => {
                builder_ok(self.builder.build_unconditional_branch(blocks[target.0]));
            }
            Terminator::Branch {
                condition,
                then_block,
                else_block,
            } => {
                let condition = self.emit_expr(condition).one().into_int_value();
                let condition = self.i8_to_bool(condition);
                builder_ok(self.builder.build_conditional_branch(
                    condition,
                    blocks[then_block.0],
                    blocks[else_block.0],
                ));
            }
            Terminator::Return(value) => {
                if let Some(sret) = self.sret_param {
                    // sret 模式：把返回值分量写进返回缓冲区。
                    let runtime_value = value
                        .as_ref()
                        .map(|value| self.emit_expr(value))
                        .unwrap_or_else(|| RuntimeValue {
                            ty: Type::Unit,
                            values: Vec::new(),
                        });
                    for ((_, offset), component) in component_layout(&runtime_value.ty, self.types)
                        .into_iter()
                        .zip(runtime_value.values)
                    {
                        let address = gep(self.context, self.builder, sret, offset);
                        builder_ok(self.builder.build_store(address, component));
                    }
                    self.emit_runtime_finish();
                    builder_ok(self.builder.build_return(None));
                } else {
                    let runtime_value = value
                        .as_ref()
                        .map(|value| self.emit_expr(value))
                        .unwrap_or_else(|| RuntimeValue {
                            ty: Type::Unit,
                            values: Vec::new(),
                        });
                    self.emit_runtime_finish();
                    self.build_return(runtime_value);
                }
            }
        }
    }

    fn build_return(&self, value: RuntimeValue<'ctx>) {
        match value.values.len() {
            0 => {
                builder_ok(self.builder.build_return(None));
            }
            1 => {
                builder_ok(self.builder.build_return(Some(&value.values[0])));
            }
            2 => {
                let components = abi_components(self.context, &value.ty, self.types);
                let struct_type = self.context.struct_type(&components, false);
                let mut aggregate = struct_type.get_undef();
                for (index, component) in value.values.into_iter().enumerate() {
                    aggregate = builder_ok(self.builder.build_insert_value(
                        aggregate,
                        component,
                        index as u32,
                        "ret",
                    ))
                    .into_struct_value();
                }
                builder_ok(self.builder.build_return(Some(&aggregate)));
            }
            _ => unreachable!("sret covers return widths above two"),
        }
    }

    fn emit_expr(&self, expression: &Expr) -> RuntimeValue<'ctx> {
        match &expression.kind {
            ExprKind::Integer(value) => RuntimeValue::scalar(
                expression.ty.clone(),
                scalar_of(&expression.ty)
                    .int_type(self.context)
                    .const_int(*value, false)
                    .into(),
            ),
            ExprKind::Float(value) => RuntimeValue::scalar(
                expression.ty.clone(),
                if expression.ty == Type::F32 {
                    self.context.f32_type().const_float(*value).into()
                } else {
                    self.context.f64_type().const_float(*value).into()
                },
            ),
            ExprKind::Char(value) => RuntimeValue::scalar(
                Type::Char,
                self.context
                    .i32_type()
                    .const_int(*value as u32 as u64, false)
                    .into(),
            ),
            ExprKind::Bool(value) => RuntimeValue::scalar(
                Type::Bool,
                self.context
                    .i8_type()
                    .const_int(u64::from(*value), false)
                    .into(),
            ),
            ExprKind::String(value) => self.emit_string(value),
            ExprKind::StringLength(value) => {
                let value = self.emit_expr(value);
                RuntimeValue::scalar(Type::Usize, value.values[1])
            }
            ExprKind::Array(elements) => {
                let mut values = Vec::with_capacity(abi_width(&expression.ty, self.types));
                for element in elements {
                    values.extend(self.emit_expr(element).values);
                }
                RuntimeValue {
                    ty: expression.ty.clone(),
                    values,
                }
            }
            ExprKind::RepeatArray { value, length } => {
                let value = self.emit_expr(value);
                let mut values = Vec::with_capacity(value.values.len() * length);
                for _ in 0..*length {
                    values.extend(value.values.iter().copied());
                }
                RuntimeValue {
                    ty: expression.ty.clone(),
                    values,
                }
            }
            ExprKind::Index { array, index } => {
                let array = self.emit_expr(array);
                let index = self.emit_expr(index).one().into_int_value();
                match array.ty.clone() {
                    Type::Array { element, length } => {
                        self.bounds_check(index, length);
                        let width = abi_width(&element.as_type(), self.types);
                        let mut values = Vec::with_capacity(width);
                        for component in 0..width {
                            let mut selected = array.values[component];
                            for array_index in 1..length {
                                let expected =
                                    index.get_type().const_int(array_index as u64, false);
                                let matches = builder_ok(self.builder.build_int_compare(
                                    IntPredicate::EQ,
                                    index,
                                    expected,
                                    "index",
                                ));
                                selected = builder_ok(self.builder.build_select(
                                    matches,
                                    array.values[array_index * width + component],
                                    selected,
                                    "element",
                                ));
                            }
                            values.push(selected);
                        }
                        RuntimeValue {
                            ty: element.as_type(),
                            values,
                        }
                    }
                    Type::Slice { element, .. } => {
                        let pointer = array.values[0].into_pointer_value();
                        let length = array.values[1].into_int_value();
                        self.bounds_check_dynamic(index, length);
                        let stride = size_of_type(&element, self.types);
                        let offset = builder_ok(self.builder.build_int_mul(
                            index,
                            index.get_type().const_int(stride as u64, false),
                            "offset",
                        ));
                        let address = gep_dynamic(self.context, self.builder, pointer, offset);
                        self.emit_load_from_address(address, &element)
                    }
                    other => unreachable!("index expression targets array or slice, got {other}"),
                }
            }
            ExprKind::Local(local) => {
                let value =
                    load_local(self.context, self.builder, &self.slots[local.0], self.types);
                RuntimeValue {
                    ty: expression.ty.clone(),
                    values: value.values,
                }
            }
            ExprKind::Call {
                function,
                arguments,
            } => self.emit_user_call(expression, *function, arguments),
            ExprKind::Unary { operator, operand } => {
                let operand = self.emit_expr(operand).one();
                let value = match operator {
                    UnaryOperator::Negate if expression.ty.is_float() => builder_ok(
                        self.builder
                            .build_float_neg(operand.into_float_value(), "neg"),
                    )
                    .into(),
                    UnaryOperator::Negate => {
                        let zero = scalar_of(&expression.ty)
                            .int_type(self.context)
                            .const_zero();
                        self.checked_binary(
                            zero,
                            operand.into_int_value(),
                            CheckedOp::Subtract,
                            true,
                        )
                        .into()
                    }
                    UnaryOperator::Not => builder_ok(self.builder.build_xor(
                        operand.into_int_value(),
                        self.context.i8_type().const_int(1, false),
                        "not",
                    ))
                    .into(),
                };
                RuntimeValue::scalar(expression.ty.clone(), value)
            }
            ExprKind::Binary {
                operator: BinaryOperator::And,
                left,
                right,
            } => self.emit_short_circuit(left, right, false),
            ExprKind::Binary {
                operator: BinaryOperator::Or,
                left,
                right,
            } => self.emit_short_circuit(left, right, true),
            ExprKind::Binary {
                operator,
                left,
                right,
            } => {
                let left_value = self.emit_expr(left);
                let right_value = self.emit_expr(right);
                if left.ty == Type::String {
                    let mut arguments = left_value.values;
                    arguments.extend(right_value.values);
                    let mut value = self
                        .call(self.runtime.string_equal, &arguments)
                        .expect("string equality returns a value")
                        .into_int_value();
                    if matches!(operator, BinaryOperator::NotEqual) {
                        value = builder_ok(self.builder.build_xor(
                            value,
                            self.context.i8_type().const_int(1, false),
                            "not",
                        ));
                    }
                    return RuntimeValue::scalar(Type::Bool, value.into());
                }
                let left = left_value.one();
                let right = right_value.one();
                if left_value.ty.is_float() {
                    let value = self.emit_float_binary(operator, left, right);
                    return RuntimeValue::scalar(expression.ty.clone(), value);
                }
                if matches!(left_value.ty, Type::Ptr { .. } | Type::Null) {
                    // 指针只支持相等性比较：按整数地址比较，行为与 Cranelift 一致。
                    let predicate = match operator {
                        BinaryOperator::Equal => IntPredicate::EQ,
                        BinaryOperator::NotEqual => IntPredicate::NE,
                        _ => unreachable!("pointer comparison supports only `==` and `!=`"),
                    };
                    let left = self.pointer_to_int(left);
                    let right = self.pointer_to_int(right);
                    let value = builder_ok(
                        self.builder
                            .build_int_compare(predicate, left, right, "cmp"),
                    );
                    return RuntimeValue::scalar(Type::Bool, self.bool_to_i8(value).into());
                }
                let signed = left_value.ty.is_signed_integer();
                let value = match operator {
                    BinaryOperator::Add => self
                        .checked_binary(
                            left.into_int_value(),
                            right.into_int_value(),
                            CheckedOp::Add,
                            signed,
                        )
                        .into(),
                    BinaryOperator::Subtract => self
                        .checked_binary(
                            left.into_int_value(),
                            right.into_int_value(),
                            CheckedOp::Subtract,
                            signed,
                        )
                        .into(),
                    BinaryOperator::Multiply => self
                        .checked_binary(
                            left.into_int_value(),
                            right.into_int_value(),
                            CheckedOp::Multiply,
                            signed,
                        )
                        .into(),
                    BinaryOperator::Divide => self.emit_division(left, right, signed, true).into(),
                    BinaryOperator::Remainder => {
                        self.emit_division(left, right, signed, false).into()
                    }
                    BinaryOperator::Equal => self.emit_int_compare(IntPredicate::EQ, left, right),
                    BinaryOperator::NotEqual => {
                        self.emit_int_compare(IntPredicate::NE, left, right)
                    }
                    BinaryOperator::Less => self.emit_int_compare(
                        if signed {
                            IntPredicate::SLT
                        } else {
                            IntPredicate::ULT
                        },
                        left,
                        right,
                    ),
                    BinaryOperator::LessEqual => self.emit_int_compare(
                        if signed {
                            IntPredicate::SLE
                        } else {
                            IntPredicate::ULE
                        },
                        left,
                        right,
                    ),
                    BinaryOperator::Greater => self.emit_int_compare(
                        if signed {
                            IntPredicate::SGT
                        } else {
                            IntPredicate::UGT
                        },
                        left,
                        right,
                    ),
                    BinaryOperator::GreaterEqual => self.emit_int_compare(
                        if signed {
                            IntPredicate::SGE
                        } else {
                            IntPredicate::UGE
                        },
                        left,
                        right,
                    ),
                    BinaryOperator::And | BinaryOperator::Or => unreachable!(),
                };
                RuntimeValue::scalar(expression.ty.clone(), value)
            }
            ExprKind::Cast { value, to } => {
                let from = value.ty.clone();
                let value = self.emit_expr(value).one();
                RuntimeValue::scalar(to.clone(), self.emit_cast(value, &from, to))
            }
            ExprKind::StructInit { fields } => {
                let mut values = Vec::new();
                for field in fields {
                    values.extend(self.emit_expr(field).values);
                }
                RuntimeValue {
                    ty: expression.ty.clone(),
                    values,
                }
            }
            ExprKind::EnumInit { variant, arguments } => {
                let tag = self.context.i32_type().const_int(*variant as u64, false);
                let mut values: Vec<BasicValueEnum> = vec![tag.into()];
                for argument in arguments {
                    let arg_value = self.emit_expr(argument);
                    let component_types: Vec<Scalar> = component_layout(&arg_value.ty, self.types)
                        .into_iter()
                        .map(|(component_type, _)| component_type)
                        .collect();
                    values.extend(
                        self.uniform_encode(&arg_value.values, &component_types)
                            .into_iter()
                            .map(BasicValueEnum::from),
                    );
                }
                let layout = component_layout(&expression.ty, self.types);
                while values.len() < layout.len() {
                    values.push(self.context.i64_type().const_zero().into());
                }
                RuntimeValue {
                    ty: expression.ty.clone(),
                    values,
                }
            }
            ExprKind::Field { base, field } => {
                let base_value = self.emit_expr(base);
                let Type::Struct(id) = base_value.ty.clone() else {
                    unreachable!("field access targets a struct");
                };
                let TypeDef::Struct { fields, .. } = &self.types[id.0] else {
                    unreachable!("field access resolves a struct type");
                };
                let mut start = 0;
                for (index, field_def) in fields.iter().enumerate() {
                    let width = component_layout(&field_def.ty, self.types).len();
                    if index == *field {
                        let values = base_value.values[start..start + width].to_vec();
                        return RuntimeValue {
                            ty: field_def.ty.clone(),
                            values,
                        };
                    }
                    start += width;
                }
                unreachable!("field index was validated during lowering");
            }
            ExprKind::AddressOf { place } => {
                let address = self.emit_place_address(place);
                RuntimeValue::scalar(expression.ty.clone(), address.into())
            }
            ExprKind::Deref { pointer } => {
                let pointer_value = self.emit_expr(pointer).one().into_pointer_value();
                self.emit_load_from_address(pointer_value, &expression.ty)
            }
            ExprKind::Null => RuntimeValue::scalar(
                expression.ty.clone(),
                self.context
                    .ptr_type(AddressSpace::default())
                    .const_zero()
                    .into(),
            ),
            ExprKind::SlicePtr { base } => {
                let base_value = self.emit_expr(base);
                RuntimeValue::scalar(expression.ty.clone(), base_value.values[0])
            }
            ExprKind::SliceLen { base } => {
                let base_value = self.emit_expr(base);
                RuntimeValue::scalar(expression.ty.clone(), base_value.values[1])
            }
            ExprKind::SliceRange { base, start, end } => {
                let base_value = self.emit_expr(base);
                let pointer = base_value.values[0].into_pointer_value();
                let length = base_value.values[1].into_int_value();
                let start_value = self.emit_expr(start).one().into_int_value();
                let end_value = self.emit_expr(end).one().into_int_value();
                // 检查 0 <= start <= end <= len。
                let start_gt_end = builder_ok(self.builder.build_int_compare(
                    IntPredicate::UGT,
                    start_value,
                    end_value,
                    "range",
                ));
                self.trap_if(start_gt_end);
                let end_gt_len = builder_ok(self.builder.build_int_compare(
                    IntPredicate::UGT,
                    end_value,
                    length,
                    "range",
                ));
                self.trap_if(end_gt_len);
                let stride = match &expression.ty {
                    Type::Slice { element, .. } => size_of_type(element, self.types),
                    _ => unreachable!("slice range produces a slice"),
                };
                let offset = builder_ok(self.builder.build_int_mul(
                    start_value,
                    start_value.get_type().const_int(stride as u64, false),
                    "offset",
                ));
                let new_pointer = gep_dynamic(self.context, self.builder, pointer, offset);
                let new_length =
                    builder_ok(self.builder.build_int_sub(end_value, start_value, "len"));
                RuntimeValue {
                    ty: expression.ty.clone(),
                    values: vec![new_pointer.into(), new_length.into()],
                }
            }
            ExprKind::MemAlloc { element, count } => {
                let count = self.emit_expr(count).one().into_int_value();
                let element_layout = layout::layout_of(element, self.types, layout::POINTER_BYTES);
                let element_size = self
                    .context
                    .i64_type()
                    .const_int(u64::from(element_layout.size), false);
                let align = self
                    .context
                    .i64_type()
                    .const_int(u64::from(element_layout.align), false);
                let pointer = self
                    .call(
                        self.runtime.alloc,
                        &[count.into(), element_size.into(), align.into()],
                    )
                    .expect("alloc returns a pointer");
                RuntimeValue {
                    ty: expression.ty.clone(),
                    values: vec![pointer, count.into()],
                }
            }
            ExprKind::MemFree { element, buffer } => {
                let buffer = self.emit_expr(buffer);
                let size = u64::from(size_of_type(element, self.types));
                let total = builder_ok(self.builder.build_int_mul(
                    buffer.values[1].into_int_value(),
                    self.context.i64_type().const_int(size, false),
                    "bytes",
                ));
                self.call(self.runtime.free, &[buffer.values[0], total.into()]);
                RuntimeValue {
                    ty: Type::Unit,
                    values: Vec::new(),
                }
            }
            ExprKind::MemCreate { element, value } => {
                let value = self.emit_expr(value);
                let element_layout = layout::layout_of(element, self.types, layout::POINTER_BYTES);
                let one = self.context.i64_type().const_int(1, false);
                let element_size = self
                    .context
                    .i64_type()
                    .const_int(u64::from(element_layout.size), false);
                let align = self
                    .context
                    .i64_type()
                    .const_int(u64::from(element_layout.align), false);
                let pointer = self
                    .call(
                        self.runtime.alloc,
                        &[one.into(), element_size.into(), align.into()],
                    )
                    .expect("alloc returns a pointer")
                    .into_pointer_value();
                for ((_, offset), component) in component_layout(element, self.types)
                    .into_iter()
                    .zip(value.values)
                {
                    let address = gep(self.context, self.builder, pointer, offset);
                    builder_ok(self.builder.build_store(address, component));
                }
                RuntimeValue::scalar(expression.ty.clone(), pointer.into())
            }
            ExprKind::MemDestroy { element, pointer } => {
                let pointer = self.emit_expr(pointer).one();
                let size = u64::from(size_of_type(element, self.types));
                self.call(
                    self.runtime.free,
                    &[
                        pointer,
                        self.context.i64_type().const_int(size, false).into(),
                    ],
                );
                RuntimeValue {
                    ty: Type::Unit,
                    values: Vec::new(),
                }
            }
            ExprKind::MemCopy { element, dst, src } => {
                let dst = self.emit_expr(dst);
                let src = self.emit_expr(src);
                let mismatch = builder_ok(self.builder.build_int_compare(
                    IntPredicate::NE,
                    dst.values[1].into_int_value(),
                    src.values[1].into_int_value(),
                    "length",
                ));
                self.trap_if(mismatch);
                let size = u64::from(size_of_type(element, self.types));
                let bytes = builder_ok(self.builder.build_int_mul(
                    dst.values[1].into_int_value(),
                    self.context.i64_type().const_int(size, false),
                    "bytes",
                ));
                self.call(
                    self.runtime.copy,
                    &[dst.values[0], src.values[0], bytes.into()],
                );
                RuntimeValue {
                    ty: Type::Unit,
                    values: Vec::new(),
                }
            }
            ExprKind::MemIsValidUtf8 { bytes } => {
                let bytes = self.emit_expr(bytes);
                let value = self
                    .call(self.runtime.is_valid_utf8, &bytes.values)
                    .expect("UTF-8 check returns a value");
                RuntimeValue::scalar(Type::Bool, value)
            }
            ExprKind::MemView { pointer, len, .. } => {
                let pointer = self.emit_expr(pointer).one();
                let len = self.emit_expr(len).one();
                // `mem.view(null, 非零长度)` 在两种 profile 下都以 101 失败。
                self.call(self.runtime.check_view, &[pointer, len]);
                RuntimeValue {
                    ty: expression.ty.clone(),
                    values: vec![pointer, len],
                }
            }
            ExprKind::MemCast { pointer } => {
                let pointer = self.emit_expr(pointer).one();
                let Type::Ptr { pointee, .. } = &expression.ty else {
                    unreachable!("MemCast produces a pointer");
                };
                let align =
                    u64::from(layout::layout_of(pointee, self.types, layout::POINTER_BYTES).align);
                self.call(
                    self.runtime.check_align,
                    &[
                        pointer,
                        self.context.i64_type().const_int(align, false).into(),
                    ],
                );
                RuntimeValue::scalar(expression.ty.clone(), pointer)
            }
            ExprKind::StringBytes { base } => {
                let base = self.emit_expr(base);
                RuntimeValue {
                    ty: expression.ty.clone(),
                    values: vec![base.values[0], base.values[1]],
                }
            }
            ExprKind::StringFromBytes { bytes } => {
                let bytes = self.emit_expr(bytes);
                self.call(self.runtime.check_utf8, &bytes.values);
                RuntimeValue {
                    ty: Type::String,
                    values: vec![bytes.values[0], bytes.values[1]],
                }
            }
            ExprKind::Match { value, arms } => self.emit_match(expression.ty.clone(), value, arms),
            ExprKind::EnumIsVariant { value, variant } => {
                let value = self.emit_expr(value);
                let tag = value.values[0].into_int_value();
                let expected = self.context.i32_type().const_int(*variant as u64, false);
                let matches = builder_ok(self.builder.build_int_compare(
                    IntPredicate::EQ,
                    tag,
                    expected,
                    "variant",
                ));
                RuntimeValue {
                    ty: Type::Bool,
                    values: vec![self.bool_to_i8(matches).into()],
                }
            }
            ExprKind::EnumPayload {
                value,
                variant,
                field,
            } => {
                let value = self.emit_expr(value);
                let Type::Enum(id) = value.ty.clone() else {
                    unreachable!("enum payload targets an enum");
                };
                let TypeDef::Enum { variants } = &self.types[id.0] else {
                    unreachable!("enum payload resolves an enum type");
                };
                // 统一布局：分量 0 是 tag，payload 从分量 1 顺序排布。
                let mut offset = 1;
                for index in 0..*field {
                    offset += component_layout(&variants[*variant].fields[index], self.types).len();
                }
                let field_ty = variants[*variant].fields[*field].clone();
                let component_types: Vec<Scalar> = component_layout(&field_ty, self.types)
                    .into_iter()
                    .map(|(component_type, _)| component_type)
                    .collect();
                let width = component_types.len();
                let uniform = &value.values[offset..offset + width];
                let values = self.uniform_decode(uniform, &component_types);
                RuntimeValue {
                    ty: field_ty,
                    values,
                }
            }
        }
    }

    fn emit_user_call(
        &self,
        expression: &Expr,
        function: ir::FunctionId,
        arguments: &[Expr],
    ) -> RuntimeValue<'ctx> {
        let uses_sret = abi_width(&expression.ty, self.types) > 2;
        let mut call_arguments: Vec<BasicValueEnum> = Vec::new();
        let sret_pointer = if uses_sret {
            let layout = layout::layout_of(&expression.ty, self.types, layout::POINTER_BYTES);
            // 每个调用点在入口块分配一个静态 sret 缓冲区：若在循环体内 alloca，
            // Debug（无优化）下会随迭代累积栈直到溢出。
            let slot = create_entry_alloca(
                self.context,
                self.builder,
                self.function,
                layout.size,
                layout.align,
                "sret",
            );
            call_arguments.push(slot.into());
            Some(slot)
        } else {
            None
        };
        for argument in arguments {
            call_arguments.extend(self.emit_expr(argument).values);
        }
        let result = self.call(self.user_functions[function.0], &call_arguments);
        if let Some(sret_pointer) = sret_pointer {
            let mut values = Vec::new();
            for (component_type, offset) in component_layout(&expression.ty, self.types) {
                let address = gep(self.context, self.builder, sret_pointer, offset);
                values.push(builder_ok(self.builder.build_load(
                    component_type.basic(self.context),
                    address,
                    "result",
                )));
            }
            RuntimeValue {
                ty: expression.ty.clone(),
                values,
            }
        } else {
            match abi_width(&expression.ty, self.types) {
                0 => RuntimeValue {
                    ty: expression.ty.clone(),
                    values: Vec::new(),
                },
                1 => RuntimeValue::scalar(
                    expression.ty.clone(),
                    result.expect("call returns a value"),
                ),
                2 => {
                    let aggregate = result.expect("call returns a value").into_struct_value();
                    let first =
                        builder_ok(self.builder.build_extract_value(aggregate, 0, "result"));
                    let second =
                        builder_ok(self.builder.build_extract_value(aggregate, 1, "result"));
                    RuntimeValue {
                        ty: expression.ty.clone(),
                        values: vec![first, second],
                    }
                }
                _ => unreachable!("sret covers return widths above two"),
            }
        }
    }

    fn emit_float_binary(
        &self,
        operator: &BinaryOperator,
        left: BasicValueEnum<'ctx>,
        right: BasicValueEnum<'ctx>,
    ) -> BasicValueEnum<'ctx> {
        let left = left.into_float_value();
        let right = right.into_float_value();
        match operator {
            BinaryOperator::Add => {
                builder_ok(self.builder.build_float_add(left, right, "add")).into()
            }
            BinaryOperator::Subtract => {
                builder_ok(self.builder.build_float_sub(left, right, "sub")).into()
            }
            BinaryOperator::Multiply => {
                builder_ok(self.builder.build_float_mul(left, right, "mul")).into()
            }
            BinaryOperator::Divide => {
                builder_ok(self.builder.build_float_div(left, right, "div")).into()
            }
            BinaryOperator::Equal => self
                .bool_to_i8(builder_ok(self.builder.build_float_compare(
                    FloatPredicate::OEQ,
                    left,
                    right,
                    "eq",
                )))
                .into(),
            BinaryOperator::NotEqual => self
                .bool_to_i8(builder_ok(self.builder.build_float_compare(
                    FloatPredicate::UNE,
                    left,
                    right,
                    "ne",
                )))
                .into(),
            BinaryOperator::Less => self
                .bool_to_i8(builder_ok(self.builder.build_float_compare(
                    FloatPredicate::OLT,
                    left,
                    right,
                    "lt",
                )))
                .into(),
            BinaryOperator::LessEqual => self
                .bool_to_i8(builder_ok(self.builder.build_float_compare(
                    FloatPredicate::OLE,
                    left,
                    right,
                    "le",
                )))
                .into(),
            BinaryOperator::Greater => self
                .bool_to_i8(builder_ok(self.builder.build_float_compare(
                    FloatPredicate::OGT,
                    left,
                    right,
                    "gt",
                )))
                .into(),
            BinaryOperator::GreaterEqual => self
                .bool_to_i8(builder_ok(self.builder.build_float_compare(
                    FloatPredicate::OGE,
                    left,
                    right,
                    "ge",
                )))
                .into(),
            BinaryOperator::Remainder | BinaryOperator::And | BinaryOperator::Or => unreachable!(),
        }
    }

    fn pointer_to_int(&self, value: BasicValueEnum<'ctx>) -> IntValue<'ctx> {
        builder_ok(self.builder.build_ptr_to_int(
            value.into_pointer_value(),
            self.context.i64_type(),
            "address",
        ))
    }

    fn emit_int_compare(
        &self,
        predicate: IntPredicate,
        left: BasicValueEnum<'ctx>,
        right: BasicValueEnum<'ctx>,
    ) -> BasicValueEnum<'ctx> {
        let left = left.into_int_value();
        let right = right.into_int_value();
        let value = builder_ok(
            self.builder
                .build_int_compare(predicate, left, right, "cmp"),
        );
        self.bool_to_i8(value).into()
    }

    fn emit_division(
        &self,
        left: BasicValueEnum<'ctx>,
        right: BasicValueEnum<'ctx>,
        signed: bool,
        is_division: bool,
    ) -> IntValue<'ctx> {
        let left = left.into_int_value();
        let right = right.into_int_value();
        let zero = right.get_type().const_zero();
        let divisor_is_zero =
            builder_ok(
                self.builder
                    .build_int_compare(IntPredicate::EQ, right, zero, "zero"),
            );
        self.trap_if(divisor_is_zero);
        if signed {
            // 有符号除法的 INT_MIN / -1 在硬件上会触发 trap，这里显式复现。
            let min = right
                .get_type()
                .const_int(1u64 << (right.get_type().get_bit_width() - 1), false);
            let minus_one = right.get_type().const_int(u64::MAX, false);
            let is_min =
                builder_ok(
                    self.builder
                        .build_int_compare(IntPredicate::EQ, left, min, "min"),
                );
            let is_minus_one = builder_ok(self.builder.build_int_compare(
                IntPredicate::EQ,
                right,
                minus_one,
                "minus_one",
            ));
            let overflow = builder_ok(self.builder.build_and(is_min, is_minus_one, "overflow"));
            self.trap_if(overflow);
        }
        let value = if signed && is_division {
            self.builder.build_int_signed_div(left, right, "div")
        } else if signed {
            self.builder.build_int_signed_rem(left, right, "rem")
        } else if is_division {
            self.builder.build_int_unsigned_div(left, right, "div")
        } else {
            self.builder.build_int_unsigned_rem(left, right, "rem")
        };
        builder_ok(value)
    }

    fn checked_binary(
        &self,
        left: IntValue<'ctx>,
        right: IntValue<'ctx>,
        operation: CheckedOp,
        signed: bool,
    ) -> IntValue<'ctx> {
        let name = match (operation, signed) {
            (CheckedOp::Add, true) => "llvm.sadd.with.overflow",
            (CheckedOp::Subtract, true) => "llvm.ssub.with.overflow",
            (CheckedOp::Multiply, true) => "llvm.smul.with.overflow",
            (CheckedOp::Add, false) => "llvm.uadd.with.overflow",
            (CheckedOp::Subtract, false) => "llvm.usub.with.overflow",
            (CheckedOp::Multiply, false) => "llvm.umul.with.overflow",
        };
        let operand_type = left.get_type();
        let intrinsic = Intrinsic::find(name).expect("LLVM overflow intrinsic exists");
        let function = intrinsic
            .get_declaration(self.module, &[operand_type.into()])
            .expect("overflow intrinsic declaration resolves");
        let call = self
            .call(function, &[left.into(), right.into()])
            .expect("overflow intrinsic returns a pair");
        let aggregate = call.into_struct_value();
        let value = builder_ok(self.builder.build_extract_value(aggregate, 0, "value"));
        let overflow =
            builder_ok(self.builder.build_extract_value(aggregate, 1, "overflow")).into_int_value();
        self.trap_if(overflow);
        value.into_int_value()
    }

    fn uniform_encode(
        &self,
        values: &[BasicValueEnum<'ctx>],
        component_types: &[Scalar],
    ) -> Vec<IntValue<'ctx>> {
        values
            .iter()
            .zip(component_types)
            .map(|(value, component_type)| match component_type {
                Scalar::F32 => {
                    let bits = builder_ok(self.builder.build_bit_cast(
                        *value,
                        self.context.i32_type(),
                        "bits",
                    ))
                    .into_int_value();
                    builder_ok(self.builder.build_int_z_extend(
                        bits,
                        self.context.i64_type(),
                        "encode",
                    ))
                }
                Scalar::F64 => builder_ok(self.builder.build_bit_cast(
                    *value,
                    self.context.i64_type(),
                    "bits",
                ))
                .into_int_value(),
                Scalar::Ptr => builder_ok(self.builder.build_ptr_to_int(
                    value.into_pointer_value(),
                    self.context.i64_type(),
                    "encode",
                )),
                scalar => {
                    let value = value.into_int_value();
                    if scalar.bits() < 64 {
                        builder_ok(self.builder.build_int_z_extend(
                            value,
                            self.context.i64_type(),
                            "encode",
                        ))
                    } else {
                        value
                    }
                }
            })
            .collect()
    }

    fn uniform_decode(
        &self,
        values: &[BasicValueEnum<'ctx>],
        component_types: &[Scalar],
    ) -> Vec<BasicValueEnum<'ctx>> {
        values
            .iter()
            .zip(component_types)
            .map(|(value, component_type)| {
                let value = value.into_int_value();
                match component_type {
                    Scalar::F32 => {
                        let bits = builder_ok(self.builder.build_int_truncate(
                            value,
                            self.context.i32_type(),
                            "bits",
                        ));
                        builder_ok(self.builder.build_bit_cast(
                            bits,
                            self.context.f32_type(),
                            "decode",
                        ))
                    }
                    Scalar::F64 => builder_ok(self.builder.build_bit_cast(
                        value,
                        self.context.f64_type(),
                        "decode",
                    )),
                    Scalar::Ptr => builder_ok(self.builder.build_int_to_ptr(
                        value,
                        self.context.ptr_type(AddressSpace::default()),
                        "decode",
                    ))
                    .into(),
                    scalar => {
                        if scalar.bits() < 64 {
                            builder_ok(self.builder.build_int_truncate(
                                value,
                                scalar.int_type(self.context),
                                "decode",
                            ))
                            .into()
                        } else {
                            value.into()
                        }
                    }
                }
            })
            .collect()
    }

    fn emit_string(&self, value: &str) -> RuntimeValue<'ctx> {
        let reference = self.strings[value];
        let pointer = reference.as_pointer_value();
        let length = self.context.i64_type().const_int(value.len() as u64, false);
        RuntimeValue {
            ty: Type::String,
            values: vec![pointer.into(), length.into()],
        }
    }

    fn emit_place_address(&self, place: &Place) -> PointerValue<'ctx> {
        match &place.kind {
            PlaceKind::Local(local) => self.slots[local.0].slot,
            PlaceKind::Field { base, field } => {
                let base_address = self.emit_place_address(base);
                let Type::Struct(id) = &base.ty else {
                    unreachable!("field place targets a struct");
                };
                let TypeDef::Struct { fields, .. } = &self.types[id.0] else {
                    unreachable!("field place resolves a struct type");
                };
                let offset =
                    layout::field_offset(fields, *field, self.types, layout::POINTER_BYTES);
                gep(self.context, self.builder, base_address, offset)
            }
            PlaceKind::Index { base, index } => {
                let base_address = self.emit_place_address(base);
                let index_value = self.emit_expr(index).one().into_int_value();
                match &base.ty {
                    Type::Slice { element, .. } => {
                        // 切片：读 ptr/len 分量，边界检查后按元素步长偏移。
                        let pointer = builder_ok(self.builder.build_load(
                            self.context.ptr_type(AddressSpace::default()),
                            base_address,
                            "ptr",
                        ))
                        .into_pointer_value();
                        let length_address = gep(self.context, self.builder, base_address, 8);
                        let length = builder_ok(self.builder.build_load(
                            self.context.i64_type(),
                            length_address,
                            "len",
                        ))
                        .into_int_value();
                        self.bounds_check_dynamic(index_value, length);
                        let stride = size_of_type(element, self.types);
                        let offset = builder_ok(self.builder.build_int_mul(
                            index_value,
                            index_value.get_type().const_int(stride as u64, false),
                            "offset",
                        ));
                        gep_dynamic(self.context, self.builder, pointer, offset)
                    }
                    Type::Array { element, length } => {
                        self.bounds_check(index_value, *length);
                        let index_value =
                            self.extend_integer(index_value, false, self.context.i64_type());
                        let stride = size_of_type(&element.as_type(), self.types);
                        let offset = builder_ok(self.builder.build_int_mul(
                            index_value,
                            index_value.get_type().const_int(stride as u64, false),
                            "offset",
                        ));
                        gep_dynamic(self.context, self.builder, base_address, offset)
                    }
                    _ => unreachable!("index place targets an array or slice"),
                }
            }
            PlaceKind::Deref { pointer } => self.emit_expr(pointer).one().into_pointer_value(),
        }
    }

    fn emit_load_from_address(&self, address: PointerValue<'ctx>, ty: &Type) -> RuntimeValue<'ctx> {
        let values = component_layout(ty, self.types)
            .into_iter()
            .map(|(component_type, offset)| {
                let component_address = gep(self.context, self.builder, address, offset);
                builder_ok(self.builder.build_load(
                    component_type.basic(self.context),
                    component_address,
                    "load",
                ))
            })
            .collect();
        RuntimeValue {
            ty: ty.clone(),
            values,
        }
    }

    fn bounds_check(&self, index: IntValue<'ctx>, length: usize) {
        let length = index.get_type().const_int(length as u64, false);
        let outside =
            builder_ok(
                self.builder
                    .build_int_compare(IntPredicate::UGE, index, length, "bounds"),
            );
        self.trap_if(outside);
    }

    fn bounds_check_dynamic(&self, index: IntValue<'ctx>, length: IntValue<'ctx>) {
        let outside =
            builder_ok(
                self.builder
                    .build_int_compare(IntPredicate::UGE, index, length, "bounds"),
            );
        self.trap_if(outside);
    }

    fn extend_integer(
        &self,
        value: IntValue<'ctx>,
        signed: bool,
        to: IntType<'ctx>,
    ) -> IntValue<'ctx> {
        let from = value.get_type();
        if from == to {
            value
        } else if signed {
            builder_ok(self.builder.build_int_s_extend(value, to, "extend"))
        } else {
            builder_ok(self.builder.build_int_z_extend(value, to, "extend"))
        }
    }

    fn emit_cast(
        &self,
        value: BasicValueEnum<'ctx>,
        from: &Type,
        to: &Type,
    ) -> BasicValueEnum<'ctx> {
        let from_pointer = matches!(from, Type::Ptr { .. } | Type::Null);
        let to_pointer = matches!(to, Type::Ptr { .. } | Type::Null);
        if from.is_float() && to.is_float() {
            return builder_ok(self.builder.build_float_cast(
                value.into_float_value(),
                scalar_of(to).float_type(self.context),
                "cast",
            ))
            .into();
        }
        if from.is_float() {
            let float = value.into_float_value();
            let destination = scalar_of(to).int_type(self.context);
            let intrinsic_name = if to.is_signed_integer() {
                "llvm.fptosi.sat"
            } else {
                "llvm.fptoui.sat"
            };
            let intrinsic = Intrinsic::find(intrinsic_name).unwrap_or_else(|| {
                unreachable!("LLVM must provide `{intrinsic_name}` for saturating casts")
            });
            // 重载类型顺序为返回类型在前、操作数类型在后。
            let function = intrinsic
                .get_declaration(self.module, &[destination.into(), float.get_type().into()])
                .unwrap_or_else(|| {
                    unreachable!("could not declare `{intrinsic_name}` for this type pair")
                });
            let call = builder_ok(self.builder.build_call(function, &[float.into()], "cast"));
            return match call.try_as_basic_value() {
                ValueKind::Basic(value) => value,
                ValueKind::Instruction(_) => {
                    unreachable!("saturating conversion returns a value")
                }
            };
        }
        if to.is_float() {
            let integer = value.into_int_value();
            return if from.is_signed_integer() {
                builder_ok(self.builder.build_signed_int_to_float(
                    integer,
                    scalar_of(to).float_type(self.context),
                    "cast",
                ))
                .into()
            } else {
                builder_ok(self.builder.build_unsigned_int_to_float(
                    integer,
                    scalar_of(to).float_type(self.context),
                    "cast",
                ))
                .into()
            };
        }
        if from_pointer && to_pointer {
            return value;
        }
        if from_pointer {
            return builder_ok(self.builder.build_ptr_to_int(
                value.into_pointer_value(),
                scalar_of(to).int_type(self.context),
                "cast",
            ))
            .into();
        }
        if to_pointer {
            return builder_ok(self.builder.build_int_to_ptr(
                value.into_int_value(),
                self.context.ptr_type(AddressSpace::default()),
                "cast",
            ))
            .into();
        }
        let from_type = scalar_of(from);
        let to_type = scalar_of(to);
        if from_type == to_type {
            value
        } else if from_type.bits() < to_type.bits() {
            let integer = value.into_int_value();
            if from.is_signed_integer() {
                builder_ok(self.builder.build_int_s_extend(
                    integer,
                    to_type.int_type(self.context),
                    "cast",
                ))
                .into()
            } else {
                builder_ok(self.builder.build_int_z_extend(
                    integer,
                    to_type.int_type(self.context),
                    "cast",
                ))
                .into()
            }
        } else {
            builder_ok(self.builder.build_int_truncate(
                value.into_int_value(),
                to_type.int_type(self.context),
                "cast",
            ))
            .into()
        }
    }

    fn emit_short_circuit(
        &self,
        left: &Expr,
        right: &Expr,
        short_circuit_value: bool,
    ) -> RuntimeValue<'ctx> {
        let left = self.i8_to_bool(self.emit_expr(left).one().into_int_value());
        let current = self
            .builder
            .get_insert_block()
            .expect("builder is positioned");
        let right_block = self.append_block("short_right");
        let merge_block = self.append_block("short_merge");
        let short_value = self
            .context
            .i8_type()
            .const_int(u64::from(short_circuit_value), false);
        if short_circuit_value {
            builder_ok(
                self.builder
                    .build_conditional_branch(left, merge_block, right_block),
            );
        } else {
            builder_ok(
                self.builder
                    .build_conditional_branch(left, right_block, merge_block),
            );
        }
        self.builder.position_at_end(right_block);
        let right = self.emit_expr(right).one().into_int_value();
        let right_end = self
            .builder
            .get_insert_block()
            .expect("builder is positioned");
        builder_ok(self.builder.build_unconditional_branch(merge_block));
        self.builder.position_at_end(merge_block);
        let phi = builder_ok(self.builder.build_phi(self.context.i8_type(), "short"));
        phi.add_incoming(&[
            (&short_value as &dyn BasicValue, current),
            (&right as &dyn BasicValue, right_end),
        ]);
        RuntimeValue::scalar(Type::Bool, phi.as_basic_value())
    }

    fn emit_match(
        &self,
        result_ty: Type,
        value: &Expr,
        arms: &[ir::MatchArm],
    ) -> RuntimeValue<'ctx> {
        let value = self.emit_expr(value);
        let tag = value.values[0].into_int_value();
        let Type::Enum(id) = value.ty.clone() else {
            unreachable!("match targets an enum");
        };
        let TypeDef::Enum { variants } = &self.types[id.0] else {
            unreachable!("match resolves an enum type");
        };

        let merge_block = self.append_block("match_merge");
        let result_width = abi_width(&result_ty, self.types);
        let mut incoming: Vec<(
            Vec<BasicValueEnum<'ctx>>,
            inkwell::basic_block::BasicBlock<'ctx>,
        )> = Vec::new();

        // 线性链式比较：每个 variant arm 生成一个比较，匹配则进入 body，否则继续下一个比较。
        for (arm_index, arm) in arms.iter().enumerate() {
            let is_last = arm_index + 1 == arms.len();
            match &arm.pattern {
                MatchPattern::Variant { variant, bindings } => {
                    let arm_body = self.append_block("match_arm");
                    let expected = self.context.i32_type().const_int(*variant as u64, false);
                    let matches = builder_ok(self.builder.build_int_compare(
                        IntPredicate::EQ,
                        tag,
                        expected,
                        "variant",
                    ));
                    let next_check = if is_last {
                        self.trap_block
                    } else {
                        self.append_block("match_next")
                    };
                    builder_ok(
                        self.builder
                            .build_conditional_branch(matches, arm_body, next_check),
                    );
                    self.builder.position_at_end(arm_body);
                    self.emit_arm_body(*variant, bindings, &value, variants);
                    let body_value = self.emit_expr(&arm.body);
                    let branch_block = self
                        .builder
                        .get_insert_block()
                        .expect("builder is positioned");
                    incoming.push((body_value.values.clone(), branch_block));
                    builder_ok(self.builder.build_unconditional_branch(merge_block));
                    if !is_last {
                        self.builder.position_at_end(next_check);
                    }
                }
                MatchPattern::Wildcard => {
                    let body_value = self.emit_expr(&arm.body);
                    let branch_block = self
                        .builder
                        .get_insert_block()
                        .expect("builder is positioned");
                    incoming.push((body_value.values, branch_block));
                    builder_ok(self.builder.build_unconditional_branch(merge_block));
                    break;
                }
            }
        }

        self.builder.position_at_end(merge_block);
        let mut result_values = Vec::new();
        if result_width > 0 {
            let component_types: Vec<Scalar> = component_layout(&result_ty, self.types)
                .into_iter()
                .map(|(component_type, _)| component_type)
                .collect();
            for component in 0..result_width {
                let phi = builder_ok(
                    self.builder
                        .build_phi(component_types[component].basic(self.context), "match"),
                );
                let references: Vec<(&dyn BasicValue, inkwell::basic_block::BasicBlock)> = incoming
                    .iter()
                    .map(|(values, block)| (&values[component] as &dyn BasicValue, *block))
                    .collect();
                phi.add_incoming(&references);
                result_values.push(phi.as_basic_value());
            }
        }
        RuntimeValue {
            ty: result_ty,
            values: result_values,
        }
    }

    fn emit_arm_body(
        &self,
        variant: usize,
        bindings: &[ir::LocalId],
        value: &RuntimeValue<'ctx>,
        variants: &[ir::EnumVariant],
    ) {
        if bindings.is_empty() {
            return;
        }
        // 统一布局下，所有 variant 的字段都从 tag 之后（分量 1）开始。
        let mut offset = 1;
        for (local, field) in bindings.iter().zip(&variants[variant].fields) {
            let component_types: Vec<Scalar> = component_layout(field, self.types)
                .into_iter()
                .map(|(component_type, _)| component_type)
                .collect();
            let width = component_types.len();
            let uniform_values = &value.values[offset..offset + width];
            let field_values = self.uniform_decode(uniform_values, &component_types);
            store_local(
                self.context,
                self.builder,
                &self.slots[local.0],
                RuntimeValue {
                    ty: field.clone(),
                    values: field_values,
                },
                self.types,
            );
            offset += width;
        }
    }
}

/// 在指定阶段运行 LLVM verifier；返回包含阶段与原始 LLVM 信息的诊断。
fn verify_module(module: &Module<'_>, stage: &str) -> Result<(), Diagnostic> {
    module.verify().map_err(|error| {
        Diagnostic::plain(format!("LLVM module verification failed {stage}: {error}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use inkwell::context::Context;

    #[test]
    fn verifier_reports_stage_and_raw_error() {
        let context = Context::create();
        let module = context.create_module("invalid");
        let function = module.add_function("bad", context.i32_type().fn_type(&[], false), None);
        let entry = context.append_basic_block(function, "entry");
        let dead = context.append_basic_block(function, "dead");
        let builder = context.create_builder();
        builder.position_at_end(entry);
        builder_ok(builder.build_unconditional_branch(dead));
        // `dead` 没有终结指令，LLVM verifier 必须拒绝。
        let error = verify_module(&module, "before optimization").unwrap_err();
        let text = error.to_string();
        assert!(text.contains("before optimization"), "{text}");
        assert!(text.contains("LLVM module verification failed"), "{text}");
    }
}
