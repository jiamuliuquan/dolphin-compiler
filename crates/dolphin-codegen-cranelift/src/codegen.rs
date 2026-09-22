use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use cranelift_codegen::ir::{
    self as clif, AbiParam, ArgumentPurpose, Block, FuncRef, GlobalValue, InstBuilder,
    MemFlagsData, StackSlot, StackSlotData, StackSlotKind, TrapCode, Value,
    condcodes::{FloatCC, IntCC},
    types,
};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{DataDescription, DataId, FuncId, Linkage, Module, default_libcall_names};
use cranelift_object::{ObjectBuilder, ObjectModule};
use dolphin_ir::ir::{
    self, Expr, ExprKind, Instruction, Place, PlaceKind, PrintPart, ScalarType, StructField,
    Terminator, Type, TypeDef,
};
use dolphin_ir::layout;
use dolphin_platform::platform::TargetPlatform;
use dolphin_source::diagnostic::Diagnostic;
use dolphin_syntax::ast::{BinaryOperator, UnaryOperator};

pub fn emit_program_optimized(
    program: &ir::Program,
    output: &Path,
    release: bool,
    platform: &dyn TargetPlatform,
) -> Result<(), Diagnostic> {
    let mut flag_builder = settings::builder();
    flag_builder
        .enable("is_pic")
        .map_err(|error| Diagnostic::plain(format!("could not enable PIC: {error}")))?;
    flag_builder
        .set("opt_level", if release { "speed" } else { "none" })
        .map_err(|error| Diagnostic::plain(format!("could not set optimization level: {error}")))?;
    let flags = settings::Flags::new(flag_builder);
    let isa_builder = cranelift_codegen::isa::lookup(platform.triple())
        .map_err(|error| Diagnostic::plain(format!("unsupported target: {error}")))?;
    let isa = isa_builder
        .finish(flags)
        .map_err(|error| Diagnostic::plain(format!("could not configure target: {error}")))?;
    let object_builder = ObjectBuilder::new(isa, "dolphin", default_libcall_names())
        .map_err(|error| Diagnostic::plain(format!("could not create object module: {error}")))?;
    let mut module = ObjectModule::new(object_builder);

    let function_ids = declare_user_functions(&mut module, program, platform)?;
    let runtime_ids = declare_runtime_functions(&mut module, platform)?;
    let strings = declare_strings(&mut module, program)?;

    for function in &program.functions {
        if function.external_link_name.is_some() {
            // extern 函数只有声明：不生成函数体。
            continue;
        }
        define_function(
            &mut module,
            program,
            function,
            &function_ids,
            &runtime_ids,
            &strings,
        )?;
    }

    let object = module
        .finish()
        .emit()
        .map_err(|error| Diagnostic::plain(format!("could not emit object file: {error}")))?;
    fs::write(output, object).map_err(|error| {
        Diagnostic::plain(format!(
            "could not write object file `{}`: {error}",
            output.display()
        ))
    })?;
    Ok(())
}

/// Cranelift 代码生成后端：实现后端无关的 [`dolphin_backend::CodegenBackend`]。
pub struct CraneliftBackend;

impl dolphin_backend::CodegenBackend for CraneliftBackend {
    fn emit_program(
        &self,
        program: &ir::Program,
        object: &Path,
        optimize: bool,
        target: &dyn TargetPlatform,
    ) -> Result<(), Diagnostic> {
        emit_program_optimized(program, object, optimize, target)
    }
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

fn declare_user_functions(
    module: &mut ObjectModule,
    program: &ir::Program,
    platform: &dyn TargetPlatform,
) -> Result<Vec<FuncId>, Diagnostic> {
    let mut ids = Vec::with_capacity(program.functions.len());
    for function in &program.functions {
        let signature = function_signature(
            module,
            function,
            program.main == Some(function.id),
            &program.types,
        );
        let symbol = if program.main == Some(function.id) {
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
        let linkage = if function.external_link_name.is_some() {
            Linkage::Import
        } else {
            Linkage::Export
        };
        ids.push(
            module
                .declare_function(&symbol, linkage, &signature)
                .map_err(|error| {
                    Diagnostic::plain(format!(
                        "could not declare function `{}`: {error}",
                        function.name
                    ))
                })?,
        );
    }
    Ok(ids)
}

#[derive(Clone)]
struct RuntimeIds {
    print_i32: FuncId,
    print_i64: FuncId,
    print_u64: FuncId,
    print_f32: FuncId,
    print_f64: FuncId,
    print_char: FuncId,
    print_bool: FuncId,
    print_string: FuncId,
    string_equal: FuncId,
    alloc: FuncId,
    free: FuncId,
    copy: FuncId,
    is_valid_utf8: FuncId,
    check_utf8: FuncId,
    check_align: FuncId,
    check_view: FuncId,
    init_args: FuncId,
    finish: FuncId,
}

fn declare_runtime_functions(
    module: &mut ObjectModule,
    platform: &dyn TargetPlatform,
) -> Result<RuntimeIds, Diagnostic> {
    let pointer = module.target_config().pointer_type();
    // 运行时函数跨越 C 边界：导入名必须与平台 C 编译器导出的符号一致。
    let declare = |module: &mut ObjectModule,
                   platform: &dyn TargetPlatform,
                   name: &str,
                   params: &[clif::Type]| {
        let mut signature = module.make_signature();
        signature
            .params
            .extend(params.iter().copied().map(AbiParam::new));
        module
            .declare_function(&platform.c_symbol(name), Linkage::Import, &signature)
            .map_err(|error| {
                Diagnostic::plain(format!(
                    "could not declare runtime function `{name}`: {error}"
                ))
            })
    };
    Ok(RuntimeIds {
        print_i32: declare(module, platform, "dolphin_print_i32", &[types::I32])?,
        print_i64: declare(module, platform, "dolphin_print_i64", &[types::I64])?,
        print_u64: declare(module, platform, "dolphin_print_u64", &[types::I64])?,
        print_f32: declare(module, platform, "dolphin_print_f32", &[types::F32])?,
        print_f64: declare(module, platform, "dolphin_print_f64", &[types::F64])?,
        print_char: declare(module, platform, "dolphin_print_char", &[types::I32])?,
        print_bool: declare(module, platform, "dolphin_print_bool", &[types::I8])?,
        print_string: declare(
            module,
            platform,
            "dolphin_print_string",
            &[pointer, pointer],
        )?,
        string_equal: {
            let mut signature = module.make_signature();
            signature
                .params
                .extend([pointer, pointer, pointer, pointer].map(AbiParam::new));
            signature.returns.push(AbiParam::new(types::I8));
            module
                .declare_function(
                    &platform.c_symbol("dolphin_string_equal"),
                    Linkage::Import,
                    &signature,
                )
                .map_err(|error| {
                    Diagnostic::plain(format!(
                        "could not declare string equality runtime: {error}"
                    ))
                })?
        },
        alloc: {
            let mut signature = module.make_signature();
            signature
                .params
                .extend([pointer, pointer, pointer].map(AbiParam::new));
            signature.returns.push(AbiParam::new(pointer));
            module
                .declare_function(
                    &platform.c_symbol("dolphin_alloc"),
                    Linkage::Import,
                    &signature,
                )
                .map_err(|error| {
                    Diagnostic::plain(format!("could not declare alloc runtime: {error}"))
                })?
        },
        free: declare(module, platform, "dolphin_free", &[pointer, pointer])?,
        copy: declare(
            module,
            platform,
            "dolphin_copy",
            &[pointer, pointer, pointer],
        )?,
        is_valid_utf8: {
            let mut signature = module.make_signature();
            signature
                .params
                .extend([pointer, pointer].map(AbiParam::new));
            signature.returns.push(AbiParam::new(types::I8));
            module
                .declare_function(
                    &platform.c_symbol("dolphin_is_valid_utf8"),
                    Linkage::Import,
                    &signature,
                )
                .map_err(|error| {
                    Diagnostic::plain(format!("could not declare UTF-8 runtime: {error}"))
                })?
        },
        check_utf8: declare(module, platform, "dolphin_check_utf8", &[pointer, pointer])?,
        check_align: declare(module, platform, "dolphin_check_align", &[pointer, pointer])?,
        check_view: declare(module, platform, "dolphin_check_view", &[pointer, pointer])?,
        init_args: declare(
            module,
            platform,
            "dolphin_init_args",
            &[types::I32, pointer],
        )?,
        finish: declare(module, platform, "dolphin_runtime_finish", &[])?,
    })
}

fn declare_strings(
    module: &mut ObjectModule,
    program: &ir::Program,
) -> Result<HashMap<String, DataId>, Diagnostic> {
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
        let id = module
            .declare_data(
                &format!("__dolphin_string_{index}"),
                Linkage::Local,
                false,
                false,
            )
            .map_err(|error| Diagnostic::plain(format!("could not declare string: {error}")))?;
        let mut description = DataDescription::new();
        let bytes = if value.is_empty() {
            vec![0]
        } else {
            value.as_bytes().to_vec()
        };
        description.define(bytes.into_boxed_slice());
        module
            .define_data(id, &description)
            .map_err(|error| Diagnostic::plain(format!("could not define string: {error}")))?;
        strings.insert(value, id);
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

fn define_function(
    module: &mut ObjectModule,
    program: &ir::Program,
    function: &ir::Function,
    function_ids: &[FuncId],
    runtime_ids: &RuntimeIds,
    strings: &HashMap<String, DataId>,
) -> Result<(), Diagnostic> {
    let target_config = module.target_config();
    let pointer_type = target_config.pointer_type();
    let mut context = module.make_context();
    context.func.signature = function_signature(
        module,
        function,
        program.main == Some(function.id),
        &program.types,
    );

    let user_refs: Vec<FuncRef> = function_ids
        .iter()
        .map(|id| module.declare_func_in_func(*id, &mut context.func))
        .collect();
    let runtime_refs = RuntimeRefs {
        print_i32: module.declare_func_in_func(runtime_ids.print_i32, &mut context.func),
        print_i64: module.declare_func_in_func(runtime_ids.print_i64, &mut context.func),
        print_u64: module.declare_func_in_func(runtime_ids.print_u64, &mut context.func),
        print_f32: module.declare_func_in_func(runtime_ids.print_f32, &mut context.func),
        print_f64: module.declare_func_in_func(runtime_ids.print_f64, &mut context.func),
        print_char: module.declare_func_in_func(runtime_ids.print_char, &mut context.func),
        print_bool: module.declare_func_in_func(runtime_ids.print_bool, &mut context.func),
        print_string: module.declare_func_in_func(runtime_ids.print_string, &mut context.func),
        string_equal: module.declare_func_in_func(runtime_ids.string_equal, &mut context.func),
        alloc: module.declare_func_in_func(runtime_ids.alloc, &mut context.func),
        free: module.declare_func_in_func(runtime_ids.free, &mut context.func),
        copy: module.declare_func_in_func(runtime_ids.copy, &mut context.func),
        is_valid_utf8: module.declare_func_in_func(runtime_ids.is_valid_utf8, &mut context.func),
        check_utf8: module.declare_func_in_func(runtime_ids.check_utf8, &mut context.func),
        check_align: module.declare_func_in_func(runtime_ids.check_align, &mut context.func),
        check_view: module.declare_func_in_func(runtime_ids.check_view, &mut context.func),
        init_args: module.declare_func_in_func(runtime_ids.init_args, &mut context.func),
        finish: module.declare_func_in_func(runtime_ids.finish, &mut context.func),
    };
    let string_refs: HashMap<String, GlobalValue> = strings
        .iter()
        .map(|(value, id)| {
            (
                value.clone(),
                module.declare_data_in_func(*id, &mut context.func),
            )
        })
        .collect();

    let mut function_builder_context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut context.func, &mut function_builder_context);
        let blocks: Vec<Block> = function
            .blocks
            .iter()
            .map(|_| builder.create_block())
            .collect();
        let entry = blocks[function.entry.0];
        builder.append_block_params_for_function_params(entry);

        let slots: Vec<LocalSlot> = function
            .locals
            .iter()
            .cloned()
            .map(|ty| create_local_slot(&mut builder, ty, &program.types, pointer_type))
            .collect();

        builder.switch_to_block(entry);
        let shape = FunctionShape::of(
            function,
            program.main == Some(function.id),
            &program.types,
            pointer_type,
        );
        let sret_param = initialize_parameters(
            &mut builder,
            function,
            &slots,
            &program.types,
            pointer_type,
            shape,
        );

        if program.main == Some(function.id) {
            // 入口是 C 的 `main(i32 argc, void *argv)`：把原始 argc/argv 交给
            // 运行时，供 `std.process` 读取（Unix 直接保存字节 argv）。
            let values = builder.block_params(entry).to_vec();
            builder
                .ins()
                .call(runtime_refs.init_args, &[values[0], values[1]]);
        }

        for (index, ir_block) in function.blocks.iter().enumerate() {
            if index != function.entry.0 {
                builder.switch_to_block(blocks[index]);
            }
            let mut emitter = Emitter {
                builder: &mut builder,
                slots: &slots,
                pointer_type,
                user_refs: &user_refs,
                runtime_refs: &runtime_refs,
                string_refs: &string_refs,
                sret_param,
                types: &program.types,
                is_main: program.main == Some(function.id),
            };
            for instruction in &ir_block.instructions {
                emitter.emit_instruction(instruction);
            }
            emitter.emit_terminator(&ir_block.terminator, &blocks);
        }

        builder.seal_all_blocks();
        builder.finalize(target_config);
    }

    module
        .define_function(function_ids[function.id.0], &mut context)
        .map_err(|error| {
            Diagnostic::plain(format!(
                "could not generate function `{}`: {error}",
                function.name
            ))
        })?;
    module.clear_context(&mut context);
    Ok(())
}

/// 描述一个用户函数的 ABI 形状：是否需要用结构返回（sret）指针。
///
/// 当返回类型展开后超过 ABI 的寄存器返回上限（System V x86_64 为 2）时，
/// 函数改用 sret：调用方分配返回缓冲区并把指针作为隐藏的第一个参数传入，
/// 被调方把结果写入该缓冲区，而非用寄存器返回。
#[derive(Clone, Copy)]
struct FunctionShape {
    /// 返回类型是否走 sret 指针（而非寄存器）。
    uses_sret: bool,
}

impl FunctionShape {
    fn of(function: &ir::Function, is_main: bool, types: &[TypeDef], pointer: clif::Type) -> Self {
        // main 返回 i32，永不使用 sret。
        let uses_sret = !is_main && abi_width(function.return_type.clone(), types, pointer) > 2;
        Self { uses_sret }
    }
}

fn function_signature(
    module: &ObjectModule,
    function: &ir::Function,
    is_main: bool,
    types: &[TypeDef],
) -> clif::Signature {
    let pointer = module.target_config().pointer_type();
    let mut signature = module.make_signature();
    if is_main {
        signature.params.push(AbiParam::new(types::I32));
        signature.params.push(AbiParam::new(pointer));
        signature.returns.push(AbiParam::new(types::I32));
        return signature;
    }
    let shape = FunctionShape::of(function, is_main, types, pointer);
    if shape.uses_sret {
        // 隐藏的 sret 指针参数（第 0 个）。按 Cranelift 约定，此时 returns 必须为空：
        // sret 指针由 lowering 自动作为返回值返回。
        signature
            .params
            .push(AbiParam::special(pointer, ArgumentPurpose::StructReturn));
    }
    for parameter in &function.parameters {
        append_abi_type(
            &mut signature.params,
            function.locals[parameter.0].clone(),
            types,
            pointer,
        );
    }
    if !shape.uses_sret {
        append_abi_type(
            &mut signature.returns,
            function.return_type.clone(),
            types,
            pointer,
        );
    }
    signature
}

fn append_abi_type(
    parameters: &mut Vec<AbiParam>,
    ty: Type,
    types: &[TypeDef],
    pointer: clif::Type,
) {
    match ty {
        Type::Unit => {}
        ty if ty.is_integer() || ty.is_float() || ty == Type::Char => {
            parameters.push(AbiParam::new(clif_type(ty, pointer)))
        }
        Type::Bool => parameters.push(AbiParam::new(types::I8)),
        Type::String => {
            parameters.push(AbiParam::new(pointer));
            parameters.push(AbiParam::new(pointer));
        }
        Type::Array { element, length } => {
            for _ in 0..length {
                append_abi_type(parameters, element.as_type(), types, pointer);
            }
        }
        Type::Struct(_) | Type::Enum(_) | Type::Ptr { .. } | Type::Slice { .. } => {
            for (component_type, _) in component_layout(ty, types, pointer) {
                parameters.push(AbiParam::new(component_type));
            }
        }
        _ => unreachable!("all scalar ABI types are covered by the guard"),
    }
}

#[derive(Clone)]
struct LocalSlot {
    slot: StackSlot,
    ty: Type,
}

fn create_local_slot(
    builder: &mut FunctionBuilder<'_>,
    ty: Type,
    types: &[TypeDef],
    pointer: clif::Type,
) -> LocalSlot {
    // 使用统一布局的字节大小（含结构体末尾对齐填充），而非分量最大偏移。
    let size = layout::layout_of(&ty, types, pointer.bytes()).size.max(1);
    let align = scalar_alignment(ty.clone(), types, pointer);
    LocalSlot {
        slot: builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            size,
            align,
        )),
        ty,
    }
}

fn initialize_parameters(
    builder: &mut FunctionBuilder<'_>,
    function: &ir::Function,
    slots: &[LocalSlot],
    types: &[TypeDef],
    pointer: clif::Type,
    shape: FunctionShape,
) -> Option<Value> {
    let block = builder.current_block().expect("entry block is selected");
    let values = builder.block_params(block).to_vec();
    let mut index = 0;
    // sret 模式下第 0 个参数是隐藏的返回缓冲区指针，不属于任何局部变量。
    let sret_param = if shape.uses_sret {
        let value = values[index];
        index += 1;
        Some(value)
    } else {
        None
    };
    for local in &function.parameters {
        let width = abi_width(slots[local.0].ty.clone(), types, pointer);
        let value =
            RuntimeValue::from_slice(slots[local.0].ty.clone(), &values[index..index + width]);
        store_local(builder, &slots[local.0], value, types, pointer);
        index += width;
    }
    sret_param
}

fn abi_width(ty: Type, types: &[TypeDef], pointer: clif::Type) -> usize {
    match ty {
        Type::Unit => 0,
        ty => component_layout(ty, types, pointer).len(),
    }
}

#[derive(Clone, Copy)]
struct RuntimeRefs {
    print_i32: FuncRef,
    print_i64: FuncRef,
    print_u64: FuncRef,
    print_f32: FuncRef,
    print_f64: FuncRef,
    print_char: FuncRef,
    print_bool: FuncRef,
    print_string: FuncRef,
    string_equal: FuncRef,
    alloc: FuncRef,
    free: FuncRef,
    copy: FuncRef,
    is_valid_utf8: FuncRef,
    check_utf8: FuncRef,
    check_align: FuncRef,
    check_view: FuncRef,
    init_args: FuncRef,
    finish: FuncRef,
}

#[derive(Clone)]
struct RuntimeValue {
    ty: Type,
    values: Vec<Value>,
}

impl RuntimeValue {
    fn scalar(ty: Type, value: Value) -> Self {
        Self {
            ty,
            values: vec![value],
        }
    }

    fn from_slice(ty: Type, values: &[Value]) -> Self {
        Self {
            ty,
            values: values.to_vec(),
        }
    }

    fn one(&self) -> Value {
        debug_assert_eq!(self.values.len(), 1);
        self.values[0]
    }
}

struct Emitter<'a, 'b> {
    builder: &'a mut FunctionBuilder<'b>,
    slots: &'a [LocalSlot],
    pointer_type: clif::Type,
    user_refs: &'a [FuncRef],
    runtime_refs: &'a RuntimeRefs,
    string_refs: &'a HashMap<String, GlobalValue>,
    sret_param: Option<Value>,
    types: &'a [TypeDef],
    /// 当前函数是否为 `main`：正常返回前执行运行时收尾（Debug 泄漏报告）。
    is_main: bool,
}

impl Emitter<'_, '_> {
    fn emit_instruction(&mut self, instruction: &Instruction) {
        match instruction {
            Instruction::SetLocal { local, value } => {
                let value = self.emit_expr(value);
                store_local(
                    self.builder,
                    &self.slots[local.0],
                    value,
                    self.types,
                    self.pointer_type,
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
                let offset = field_offset_in_struct(fields, *field, self.types, self.pointer_type);
                let field_ty = fields[*field].ty.clone();
                let value = self.emit_expr(value);
                for ((_, component_offset), component_value) in
                    component_layout(field_ty, self.types, self.pointer_type)
                        .into_iter()
                        .zip(value.values)
                {
                    self.builder.ins().store(
                        MemFlagsData::new(),
                        component_value,
                        base_address,
                        offset + component_offset,
                    );
                }
            }
            Instruction::SetIndexAt { place, value } => {
                let address = self.emit_place_address(place);
                let value = self.emit_expr(value);
                let layout = component_layout(place.ty.clone(), self.types, self.pointer_type);
                for ((_, offset), component) in layout.into_iter().zip(value.values) {
                    self.builder
                        .ins()
                        .store(MemFlagsData::new(), component, address, offset);
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
                        (self.runtime_refs.print_i32, value.values)
                    } else if value.ty.is_signed_integer() {
                        let extended = self.extend_integer(value.one(), value.ty, types::I64, true);
                        (self.runtime_refs.print_i64, vec![extended])
                    } else if value.ty.is_integer() {
                        let extended =
                            self.extend_integer(value.one(), value.ty, types::I64, false);
                        (self.runtime_refs.print_u64, vec![extended])
                    } else {
                        match value.ty {
                            Type::F32 => (self.runtime_refs.print_f32, value.values),
                            Type::F64 => (self.runtime_refs.print_f64, value.values),
                            Type::Char => (self.runtime_refs.print_char, value.values),
                            Type::Bool => (self.runtime_refs.print_bool, value.values),
                            Type::String => (self.runtime_refs.print_string, value.values),
                            Type::Unit | Type::Array { .. } => {
                                unreachable!("value cannot be printed")
                            }
                            _ => unreachable!(),
                        }
                    };
                    self.builder.ins().call(function, &arguments);
                }
            }
        }
    }

    fn emit_terminator(&mut self, terminator: &Terminator, blocks: &[Block]) {
        match terminator {
            Terminator::Jump(target) => {
                self.builder.ins().jump(blocks[target.0], &[]);
            }
            Terminator::Branch {
                condition,
                then_block,
                else_block,
            } => {
                let condition = self.emit_expr(condition).one();
                self.builder.ins().brif(
                    condition,
                    blocks[then_block.0],
                    &[],
                    blocks[else_block.0],
                    &[],
                );
            }
            Terminator::Return(value) => {
                if let Some(sret) = self.sret_param {
                    // sret 模式：把返回值分量写进返回缓冲区，并返回缓冲区指针。
                    let runtime_value = value
                        .as_ref()
                        .map(|value| self.emit_expr(value))
                        .unwrap_or_else(|| RuntimeValue {
                            ty: Type::Unit,
                            values: Vec::new(),
                        });
                    let layout = component_layout(runtime_value.ty, self.types, self.pointer_type);
                    for ((_, offset), component) in layout.into_iter().zip(runtime_value.values) {
                        self.builder
                            .ins()
                            .store(MemFlagsData::new(), component, sret, offset);
                    }
                    self.emit_runtime_finish();
                    // returns 为空，Cranelift lowering 会自动把 sret 指针作为返回值返回。
                    self.builder.ins().return_(&[]);
                } else {
                    let values = value
                        .as_ref()
                        .map(|value| self.emit_expr(value).values)
                        .unwrap_or_default();
                    self.emit_runtime_finish();
                    self.builder.ins().return_(&values);
                }
            }
        }
    }

    /// main 正常返回前执行运行时收尾：Debug 版报告未释放的分配，Release 版为
    /// 空操作。trap/OOM 不经过这里，因此不会误报。
    fn emit_runtime_finish(&mut self) {
        if self.is_main {
            self.builder.ins().call(self.runtime_refs.finish, &[]);
        }
    }

    fn emit_expr(&mut self, expression: &Expr) -> RuntimeValue {
        match &expression.kind {
            ExprKind::Integer(value) => RuntimeValue::scalar(
                expression.ty.clone(),
                self.builder.ins().iconst(
                    clif_type(expression.ty.clone(), self.pointer_type),
                    *value as i64,
                ),
            ),
            ExprKind::Float(value) => RuntimeValue::scalar(
                expression.ty.clone(),
                if expression.ty == Type::F32 {
                    self.builder.ins().f32const(*value as f32)
                } else {
                    self.builder.ins().f64const(*value)
                },
            ),
            ExprKind::Char(value) => RuntimeValue::scalar(
                Type::Char,
                self.builder.ins().iconst(types::I32, *value as u32 as i64),
            ),
            ExprKind::Bool(value) => RuntimeValue::scalar(
                Type::Bool,
                self.builder.ins().iconst(types::I8, i64::from(*value)),
            ),
            ExprKind::String(value) => self.emit_string(value),
            ExprKind::StringLength(value) => {
                let value = self.emit_expr(value);
                // `length` 统一返回 `usize`（目标指针宽度），不再按 i32 使用。
                RuntimeValue::scalar(Type::Usize, value.values[1])
            }
            ExprKind::Array(elements) => {
                let mut values = Vec::with_capacity(abi_width(
                    expression.ty.clone(),
                    self.types,
                    self.pointer_type,
                ));
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
                let index = self.emit_expr(index).one();
                match array.ty.clone() {
                    Type::Array { element, length } => {
                        self.bounds_check(index, length);
                        let width = abi_width(element.as_type(), self.types, self.pointer_type);
                        let mut values = Vec::with_capacity(width);
                        for component in 0..width {
                            let mut selected = array.values[component];
                            for array_index in 1..length {
                                let expected =
                                    self.builder.ins().iconst(types::I32, array_index as i64);
                                let matches =
                                    self.builder.ins().icmp(IntCC::Equal, index, expected);
                                selected = self.builder.ins().select(
                                    matches,
                                    array.values[array_index * width + component],
                                    selected,
                                );
                            }
                            values.push(selected);
                        }
                        RuntimeValue {
                            ty: element.as_type(),
                            values,
                        }
                    }
                    Type::Slice { element, .. } => {
                        let ptr = array.values[0];
                        let len = array.values[1];
                        self.bounds_check_dynamic(index, len);
                        let stride =
                            size_of_type((*element).clone(), self.types, self.pointer_type);
                        let offset = self.builder.ins().imul_imm_s(index, stride as i64);
                        let address = self.builder.ins().iadd(ptr, offset);
                        self.emit_load_from_address(address, &element)
                    }
                    other => unreachable!("index expression targets array or slice, got {other}"),
                }
            }
            ExprKind::Local(local) => {
                let value = load_local(
                    self.builder,
                    &self.slots[local.0],
                    self.types,
                    self.pointer_type,
                );
                // 使用表达式自身的类型：`[]T` 绑定到 `[]const T` 时槽类型与表达式
                // 类型只差只读限定，运行时分量相同。
                RuntimeValue {
                    ty: expression.ty.clone(),
                    values: value.values,
                }
            }
            ExprKind::Call {
                function,
                arguments,
            } => {
                let mut values = Vec::new();
                let uses_sret = abi_width(expression.ty.clone(), self.types, self.pointer_type) > 2;
                let sret_ptr = if uses_sret {
                    // sret 模式：分配返回缓冲区，把其地址作为隐藏的第一个参数。
                    let size =
                        layout::layout_of(&expression.ty, self.types, self.pointer_type.bytes())
                            .size
                            .max(1);
                    let align =
                        scalar_alignment(expression.ty.clone(), self.types, self.pointer_type);
                    let slot = self.builder.create_sized_stack_slot(StackSlotData::new(
                        StackSlotKind::ExplicitSlot,
                        size,
                        align,
                    ));
                    let sret_ptr = self.builder.ins().stack_addr(self.pointer_type, slot, 0);
                    values.push(sret_ptr);
                    Some(sret_ptr)
                } else {
                    None
                };
                for argument in arguments {
                    values.extend(self.emit_expr(argument).values);
                }
                let call = self.builder.ins().call(self.user_refs[function.0], &values);
                if let Some(sret_ptr) = sret_ptr {
                    // sret 模式：从自己分配的返回缓冲区读回结果分量。
                    let layout =
                        component_layout(expression.ty.clone(), self.types, self.pointer_type);
                    let mut components = Vec::with_capacity(layout.len());
                    for (component_type, offset) in layout {
                        components.push(self.builder.ins().load(
                            component_type,
                            MemFlagsData::new(),
                            sret_ptr,
                            offset,
                        ));
                    }
                    RuntimeValue {
                        ty: expression.ty.clone(),
                        values: components,
                    }
                } else {
                    let results = self.builder.inst_results(call).to_vec();
                    RuntimeValue::from_slice(expression.ty.clone(), &results)
                }
            }
            ExprKind::Unary { operator, operand } => {
                let operand = self.emit_expr(operand).one();
                let value = match operator {
                    UnaryOperator::Negate if expression.ty.is_float() => {
                        self.builder.ins().fneg(operand)
                    }
                    UnaryOperator::Negate => {
                        let zero = self
                            .builder
                            .ins()
                            .iconst(clif_type(expression.ty.clone(), self.pointer_type), 0);
                        let (value, overflow) = self.builder.ins().ssub_overflow(zero, operand);
                        self.builder
                            .ins()
                            .trapnz(overflow, TrapCode::INTEGER_OVERFLOW);
                        value
                    }
                    UnaryOperator::Not => self.builder.ins().bxor_imm_u(operand, 1),
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
                    let call = self
                        .builder
                        .ins()
                        .call(self.runtime_refs.string_equal, &arguments);
                    let mut value = self.builder.inst_results(call)[0];
                    if matches!(operator, BinaryOperator::NotEqual) {
                        value = self.builder.ins().bxor_imm_u(value, 1);
                    }
                    return RuntimeValue::scalar(Type::Bool, value);
                }
                let left = left_value.one();
                let right = right_value.one();
                if left_value.ty.is_float() {
                    let value = match operator {
                        BinaryOperator::Add => self.builder.ins().fadd(left, right),
                        BinaryOperator::Subtract => self.builder.ins().fsub(left, right),
                        BinaryOperator::Multiply => self.builder.ins().fmul(left, right),
                        BinaryOperator::Divide => self.builder.ins().fdiv(left, right),
                        BinaryOperator::Equal => {
                            self.builder.ins().fcmp(FloatCC::Equal, left, right)
                        }
                        BinaryOperator::NotEqual => {
                            self.builder.ins().fcmp(FloatCC::NotEqual, left, right)
                        }
                        BinaryOperator::Less => {
                            self.builder.ins().fcmp(FloatCC::LessThan, left, right)
                        }
                        BinaryOperator::LessEqual => {
                            self.builder
                                .ins()
                                .fcmp(FloatCC::LessThanOrEqual, left, right)
                        }
                        BinaryOperator::Greater => {
                            self.builder.ins().fcmp(FloatCC::GreaterThan, left, right)
                        }
                        BinaryOperator::GreaterEqual => {
                            self.builder
                                .ins()
                                .fcmp(FloatCC::GreaterThanOrEqual, left, right)
                        }
                        _ => unreachable!(),
                    };
                    return RuntimeValue::scalar(expression.ty.clone(), value);
                }
                let signed = left_value.ty.is_signed_integer();
                let value = match operator {
                    BinaryOperator::Add => {
                        checked_binary(self.builder, left, right, CheckedOp::Add, signed)
                    }
                    BinaryOperator::Subtract => {
                        checked_binary(self.builder, left, right, CheckedOp::Subtract, signed)
                    }
                    BinaryOperator::Multiply => {
                        checked_binary(self.builder, left, right, CheckedOp::Multiply, signed)
                    }
                    BinaryOperator::Divide if signed => self.builder.ins().sdiv(left, right),
                    BinaryOperator::Divide => self.builder.ins().udiv(left, right),
                    BinaryOperator::Remainder if signed => self.builder.ins().srem(left, right),
                    BinaryOperator::Remainder => self.builder.ins().urem(left, right),
                    BinaryOperator::Equal => self.builder.ins().icmp(IntCC::Equal, left, right),
                    BinaryOperator::NotEqual => {
                        self.builder.ins().icmp(IntCC::NotEqual, left, right)
                    }
                    BinaryOperator::Less => self.builder.ins().icmp(
                        if signed {
                            IntCC::SignedLessThan
                        } else {
                            IntCC::UnsignedLessThan
                        },
                        left,
                        right,
                    ),
                    BinaryOperator::LessEqual => self.builder.ins().icmp(
                        if signed {
                            IntCC::SignedLessThanOrEqual
                        } else {
                            IntCC::UnsignedLessThanOrEqual
                        },
                        left,
                        right,
                    ),
                    BinaryOperator::Greater => self.builder.ins().icmp(
                        if signed {
                            IntCC::SignedGreaterThan
                        } else {
                            IntCC::UnsignedGreaterThan
                        },
                        left,
                        right,
                    ),
                    BinaryOperator::GreaterEqual => self.builder.ins().icmp(
                        if signed {
                            IntCC::SignedGreaterThanOrEqual
                        } else {
                            IntCC::UnsignedGreaterThanOrEqual
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
                RuntimeValue::scalar(to.clone(), self.emit_cast(value, from, to.clone()))
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
                // 统一枚举运行时表示：I32 tag + 统一 I64 payload 分量。
                let tag = self.builder.ins().iconst(types::I32, *variant as i64);
                let mut values = vec![tag];
                for argument in arguments {
                    let arg_value = self.emit_expr(argument);
                    let component_types: Vec<clif::Type> =
                        component_layout(arg_value.ty.clone(), self.types, self.pointer_type)
                            .into_iter()
                            .map(|(ty, _)| ty)
                            .collect();
                    values.extend(self.uniform_encode(&arg_value.values, &component_types));
                }
                // 填充到完整 layout 宽度（I64 零值）。
                let layout = component_layout(expression.ty.clone(), self.types, self.pointer_type);
                while values.len() < layout.len() {
                    values.push(self.builder.ins().iconst(types::I64, 0));
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
                // 计算字段在扁平组件里的起始下标。
                let mut start = 0;
                for (index, field_def) in fields.iter().enumerate() {
                    let width = self.field_width(&field_def.ty);
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
                RuntimeValue::scalar(expression.ty.clone(), address)
            }
            ExprKind::Deref { pointer } => {
                let pointer_value = self.emit_expr(pointer).one();
                self.emit_load_from_address(pointer_value, &expression.ty)
            }
            ExprKind::Null => {
                let null = self.builder.ins().iconst(self.pointer_type, 0);
                RuntimeValue::scalar(expression.ty.clone(), null)
            }
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
                let ptr = base_value.values[0];
                let len = base_value.values[1];
                let start_value = self.emit_expr(start).one();
                let end_value = self.emit_expr(end).one();
                // 检查 0 <= start <= end <= len。
                let start_gt_end =
                    self.builder
                        .ins()
                        .icmp(IntCC::UnsignedGreaterThan, start_value, end_value);
                self.builder
                    .ins()
                    .trapnz(start_gt_end, TrapCode::unwrap_user(1));
                let end_gt_len =
                    self.builder
                        .ins()
                        .icmp(IntCC::UnsignedGreaterThan, end_value, len);
                self.builder
                    .ins()
                    .trapnz(end_gt_len, TrapCode::unwrap_user(1));
                let stride = match &expression.ty {
                    Type::Slice { element, .. } => {
                        size_of_type((**element).clone(), self.types, self.pointer_type)
                    }
                    _ => unreachable!("slice range produces a slice"),
                };
                let offset = self.builder.ins().imul_imm_s(start_value, stride as i64);
                let new_ptr = self.builder.ins().iadd(ptr, offset);
                let new_len = self.builder.ins().isub(end_value, start_value);
                RuntimeValue {
                    ty: expression.ty.clone(),
                    values: vec![new_ptr, new_len],
                }
            }
            ExprKind::MemAlloc { element, count } => {
                let count = self.emit_expr(count).one();
                let layout = layout::layout_of(element, self.types, self.pointer_type.bytes());
                let elem_size = self
                    .builder
                    .ins()
                    .iconst(self.pointer_type, i64::from(layout.size));
                let align = self
                    .builder
                    .ins()
                    .iconst(self.pointer_type, i64::from(layout.align));
                let call = self
                    .builder
                    .ins()
                    .call(self.runtime_refs.alloc, &[count, elem_size, align]);
                let pointer = self.builder.inst_results(call)[0];
                RuntimeValue {
                    ty: expression.ty.clone(),
                    values: vec![pointer, count],
                }
            }
            ExprKind::MemFree { element, buffer } => {
                let buffer = self.emit_expr(buffer);
                let size =
                    layout::layout_of(element, self.types, self.pointer_type.bytes()).size as i64;
                let total = self.builder.ins().imul_imm_s(buffer.values[1], size);
                self.builder
                    .ins()
                    .call(self.runtime_refs.free, &[buffer.values[0], total]);
                RuntimeValue {
                    ty: Type::Unit,
                    values: Vec::new(),
                }
            }
            ExprKind::MemCreate { element, value } => {
                let value = self.emit_expr(value);
                let layout = layout::layout_of(element, self.types, self.pointer_type.bytes());
                let one = self.builder.ins().iconst(self.pointer_type, 1);
                let elem_size = self
                    .builder
                    .ins()
                    .iconst(self.pointer_type, i64::from(layout.size));
                let align = self
                    .builder
                    .ins()
                    .iconst(self.pointer_type, i64::from(layout.align));
                let call = self
                    .builder
                    .ins()
                    .call(self.runtime_refs.alloc, &[one, elem_size, align]);
                let pointer = self.builder.inst_results(call)[0];
                // 把 value 分量写入新分配的对象。
                let components = component_layout(element.clone(), self.types, self.pointer_type);
                for ((_, offset), component) in components.into_iter().zip(value.values) {
                    self.builder
                        .ins()
                        .store(MemFlagsData::new(), component, pointer, offset);
                }
                RuntimeValue::scalar(expression.ty.clone(), pointer)
            }
            ExprKind::MemDestroy { element, pointer } => {
                let pointer = self.emit_expr(pointer).one();
                let size =
                    layout::layout_of(element, self.types, self.pointer_type.bytes()).size as i64;
                let total = self.builder.ins().iconst(self.pointer_type, size);
                self.builder
                    .ins()
                    .call(self.runtime_refs.free, &[pointer, total]);
                RuntimeValue {
                    ty: Type::Unit,
                    values: Vec::new(),
                }
            }
            ExprKind::MemCopy { element, dst, src } => {
                let dst = self.emit_expr(dst);
                let src = self.emit_expr(src);
                let mismatch =
                    self.builder
                        .ins()
                        .icmp(IntCC::NotEqual, dst.values[1], src.values[1]);
                self.builder
                    .ins()
                    .trapnz(mismatch, TrapCode::unwrap_user(1));
                let size =
                    layout::layout_of(element, self.types, self.pointer_type.bytes()).size as i64;
                let bytes = self.builder.ins().imul_imm_s(dst.values[1], size);
                self.builder.ins().call(
                    self.runtime_refs.copy,
                    &[dst.values[0], src.values[0], bytes],
                );
                RuntimeValue {
                    ty: Type::Unit,
                    values: Vec::new(),
                }
            }
            ExprKind::MemIsValidUtf8 { bytes } => {
                let bytes = self.emit_expr(bytes);
                let call = self
                    .builder
                    .ins()
                    .call(self.runtime_refs.is_valid_utf8, &bytes.values);
                let value = self.builder.inst_results(call)[0];
                RuntimeValue::scalar(Type::Bool, value)
            }
            ExprKind::MemView { pointer, len, .. } => {
                let pointer = self.emit_expr(pointer).one();
                let len = self.emit_expr(len).one();
                // `mem.view(null, 非零长度)` 在两种 profile 下都以 101 失败。
                self.builder
                    .ins()
                    .call(self.runtime_refs.check_view, &[pointer, len]);
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
                    layout::layout_of(pointee, self.types, self.pointer_type.bytes()).align as i64;
                let align = self.builder.ins().iconst(self.pointer_type, align);
                self.builder
                    .ins()
                    .call(self.runtime_refs.check_align, &[pointer, align]);
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
                self.builder
                    .ins()
                    .call(self.runtime_refs.check_utf8, &bytes.values);
                RuntimeValue {
                    ty: Type::String,
                    values: vec![bytes.values[0], bytes.values[1]],
                }
            }
            ExprKind::Match { value, arms } => self.emit_match(expression.ty.clone(), value, arms),
            ExprKind::EnumIsVariant { value, variant } => {
                let value = self.emit_expr(value);
                let tag = value.values[0];
                let expected = self.builder.ins().iconst(types::I32, *variant as i64);
                let matches = self.builder.ins().icmp(IntCC::Equal, tag, expected);
                RuntimeValue {
                    ty: Type::Bool,
                    values: vec![matches],
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
                    offset += component_layout(
                        variants[*variant].fields[index].clone(),
                        self.types,
                        self.pointer_type,
                    )
                    .len();
                }
                let field_ty = variants[*variant].fields[*field].clone();
                let component_types: Vec<clif::Type> =
                    component_layout(field_ty.clone(), self.types, self.pointer_type)
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

    /// 计算一个 place 的运行时地址（pointer 分量）。
    fn emit_place_address(&mut self, place: &Place) -> Value {
        match &place.kind {
            PlaceKind::Local(local) => {
                self.builder
                    .ins()
                    .stack_addr(self.pointer_type, self.slots[local.0].slot, 0)
            }
            PlaceKind::Field { base, field } => {
                let base_address = self.emit_place_address(base);
                let Type::Struct(id) = &base.ty else {
                    unreachable!("field place targets a struct");
                };
                let TypeDef::Struct { fields, .. } = &self.types[id.0] else {
                    unreachable!("field place resolves a struct type");
                };
                let offset = field_offset_in_struct(fields, *field, self.types, self.pointer_type);
                self.builder.ins().iadd_imm_s(base_address, offset as i64)
            }
            PlaceKind::Index { base, index } => {
                let base_address = self.emit_place_address(base);
                let index_value = self.emit_expr(index).one();
                match &base.ty {
                    Type::Slice { element, .. } => {
                        // 切片：读 ptr/len 分量，边界检查后按元素步长偏移。
                        let ptr = self.builder.ins().load(
                            self.pointer_type,
                            MemFlagsData::new(),
                            base_address,
                            0,
                        );
                        let len = self.builder.ins().load(
                            self.pointer_type,
                            MemFlagsData::new(),
                            base_address,
                            self.pointer_type.bytes() as i32,
                        );
                        self.bounds_check_dynamic(index_value, len);
                        let stride =
                            size_of_type((**element).clone(), self.types, self.pointer_type);
                        let offset = self.builder.ins().imul_imm_s(index_value, stride as i64);
                        self.builder.ins().iadd(ptr, offset)
                    }
                    Type::Array { element, length } => {
                        self.bounds_check(index_value, *length);
                        let index_value =
                            self.extend_integer(index_value, Type::I32, self.pointer_type, false);
                        let stride = size_of_type(element.as_type(), self.types, self.pointer_type);
                        let offset = self.builder.ins().imul_imm_s(index_value, stride as i64);
                        self.builder.ins().iadd(base_address, offset)
                    }
                    _ => unreachable!("index place targets an array or slice"),
                }
            }
            PlaceKind::Deref { pointer } => self.emit_expr(pointer).one(),
        }
    }

    /// 从地址加载一个完整值（按分量展开）。
    fn emit_load_from_address(&mut self, address: Value, ty: &Type) -> RuntimeValue {
        let layout = component_layout(ty.clone(), self.types, self.pointer_type);
        let values = layout
            .into_iter()
            .map(|(component_type, offset)| {
                self.builder
                    .ins()
                    .load(component_type, MemFlagsData::new(), address, offset)
            })
            .collect();
        RuntimeValue {
            ty: ty.clone(),
            values,
        }
    }

    /// 计算一个类型扁平展开后的组件个数。
    fn field_width(&self, ty: &Type) -> usize {
        component_layout(ty.clone(), self.types, self.pointer_type).len()
    }

    /// 把一组自然类型分量统一为 I64 分量（用于枚举 payload 的 bitcast 存储）。
    fn uniform_encode(&mut self, values: &[Value], component_types: &[clif::Type]) -> Vec<Value> {
        values
            .iter()
            .zip(component_types)
            .map(|(value, component_type)| {
                if *component_type == types::F32 {
                    let i32_value =
                        self.builder
                            .ins()
                            .bitcast(types::I32, MemFlagsData::new(), *value);
                    self.builder.ins().uextend(types::I64, i32_value)
                } else if *component_type == types::F64 {
                    self.builder
                        .ins()
                        .bitcast(types::I64, MemFlagsData::new(), *value)
                } else if component_type.bits() < 64 {
                    self.builder.ins().uextend(types::I64, *value)
                } else {
                    *value
                }
            })
            .collect()
    }

    /// 把统一 I64 分量恢复为自然类型分量。
    fn uniform_decode(&mut self, values: &[Value], component_types: &[clif::Type]) -> Vec<Value> {
        values
            .iter()
            .zip(component_types)
            .map(|(value, component_type)| {
                if *component_type == types::F32 {
                    let i32_value = self.builder.ins().ireduce(types::I32, *value);
                    self.builder
                        .ins()
                        .bitcast(types::F32, MemFlagsData::new(), i32_value)
                } else if *component_type == types::F64 {
                    self.builder
                        .ins()
                        .bitcast(types::F64, MemFlagsData::new(), *value)
                } else if component_type.bits() < 64 {
                    self.builder.ins().ireduce(*component_type, *value)
                } else {
                    *value
                }
            })
            .collect()
    }

    fn emit_string(&mut self, value: &str) -> RuntimeValue {
        let reference = self.string_refs[value];
        let pointer = self
            .builder
            .ins()
            .symbol_value(self.pointer_type, reference);
        let length = self
            .builder
            .ins()
            .iconst(self.pointer_type, value.len() as i64);
        RuntimeValue {
            ty: Type::String,
            values: vec![pointer, length],
        }
    }

    fn bounds_check(&mut self, index: Value, length: usize) {
        let length = self.builder.ins().iconst(types::I32, length as i64);
        let outside = self
            .builder
            .ins()
            .icmp(IntCC::UnsignedGreaterThanOrEqual, index, length);
        self.builder.ins().trapnz(outside, TrapCode::unwrap_user(1));
    }

    fn bounds_check_dynamic(&mut self, index: Value, length: Value) {
        let outside = self
            .builder
            .ins()
            .icmp(IntCC::UnsignedGreaterThanOrEqual, index, length);
        self.builder.ins().trapnz(outside, TrapCode::unwrap_user(1));
    }

    fn extend_integer(&mut self, value: Value, from: Type, to: clif::Type, signed: bool) -> Value {
        let from_type = clif_type(from, self.pointer_type);
        if from_type == to {
            value
        } else if signed {
            self.builder.ins().sextend(to, value)
        } else {
            self.builder.ins().uextend(to, value)
        }
    }

    fn emit_cast(&mut self, value: Value, from: Type, to: Type) -> Value {
        let from_type = clif_type(from.clone(), self.pointer_type);
        let to_type = clif_type(to.clone(), self.pointer_type);
        if from.is_float() && to.is_float() {
            return if from == Type::F32 && to == Type::F64 {
                self.builder.ins().fpromote(types::F64, value)
            } else if from == Type::F64 && to == Type::F32 {
                self.builder.ins().fdemote(types::F32, value)
            } else {
                value
            };
        }
        if from.is_float() {
            return self.emit_saturating_float_to_int(value, &to);
        }
        if to.is_float() {
            return if from.is_signed_integer() {
                self.builder.ins().fcvt_from_sint(to_type, value)
            } else {
                self.builder.ins().fcvt_from_uint(to_type, value)
            };
        }
        if from_type == to_type {
            value
        } else if from_type.bits() < to_type.bits() {
            if from.is_signed_integer() {
                self.builder.ins().sextend(to_type, value)
            } else {
                self.builder.ins().uextend(to_type, value)
            }
        } else {
            self.builder.ins().ireduce(to_type, value)
        }
    }

    /// 浮点到整数的饱和转换（截断、越界饱和、NaN→0）。
    ///
    /// Cranelift 0.134 的 x64 饱和序列只实现 32/64 位目标，直接对 I8/I16
    /// 发射 `fcvt_to_*_sat` 会在发射阶段触发内部 `unreachable`。窄目标先按
    /// I32 饱和，再在最终位宽上夹紧并窄化，结果仍由最终位宽决定，而不是
    /// 先转宽再截断。
    fn emit_saturating_float_to_int(&mut self, value: Value, to: &Type) -> Value {
        let bits = to.bits().unwrap_or(32);
        let signed = to.is_signed_integer();
        let target_type = clif_type(to.clone(), self.pointer_type);
        if bits >= 32 {
            return if signed {
                self.builder.ins().fcvt_to_sint_sat(target_type, value)
            } else {
                self.builder.ins().fcvt_to_uint_sat(target_type, value)
            };
        }
        let saturated = if signed {
            self.builder.ins().fcvt_to_sint_sat(types::I32, value)
        } else {
            self.builder.ins().fcvt_to_uint_sat(types::I32, value)
        };
        let clamped = if signed {
            let min = self.builder.ins().iconst(types::I32, -(1i64 << (bits - 1)));
            let max = self
                .builder
                .ins()
                .iconst(types::I32, (1i64 << (bits - 1)) - 1);
            let below = self
                .builder
                .ins()
                .icmp(IntCC::SignedLessThan, saturated, min);
            let above = self
                .builder
                .ins()
                .icmp(IntCC::SignedGreaterThan, saturated, max);
            let low = self.builder.ins().select(below, min, saturated);
            self.builder.ins().select(above, max, low)
        } else {
            let max = self.builder.ins().iconst(types::I32, (1i64 << bits) - 1);
            let above = self
                .builder
                .ins()
                .icmp(IntCC::UnsignedGreaterThan, saturated, max);
            self.builder.ins().select(above, max, saturated)
        };
        if target_type == types::I32 {
            clamped
        } else {
            self.builder.ins().ireduce(target_type, clamped)
        }
    }

    fn emit_short_circuit(
        &mut self,
        left: &Expr,
        right: &Expr,
        short_circuit_value: bool,
    ) -> RuntimeValue {
        let left = self.emit_expr(left).one();
        let right_block = self.builder.create_block();
        let merge_block = self.builder.create_block();
        let result = self.builder.append_block_param(merge_block, types::I8);
        let short_value = self
            .builder
            .ins()
            .iconst(types::I8, i64::from(short_circuit_value));
        if short_circuit_value {
            self.builder
                .ins()
                .brif(left, merge_block, &[short_value.into()], right_block, &[]);
        } else {
            self.builder
                .ins()
                .brif(left, right_block, &[], merge_block, &[short_value.into()]);
        }
        self.builder.switch_to_block(right_block);
        let right = self.emit_expr(right).one();
        self.builder.ins().jump(merge_block, &[right.into()]);
        self.builder.switch_to_block(merge_block);
        RuntimeValue::scalar(Type::Bool, result)
    }

    fn emit_match(&mut self, result_ty: Type, value: &Expr, arms: &[ir::MatchArm]) -> RuntimeValue {
        let value = self.emit_expr(value);
        let tag = value.values[0];
        let Type::Enum(id) = value.ty else {
            unreachable!("match targets an enum");
        };
        let TypeDef::Enum { variants, .. } = &self.types[id.0] else {
            unreachable!("match resolves an enum type");
        };

        let merge_block = self.builder.create_block();
        // 表达式式 match 结果分量（Unit 无分量）。
        let result_values: Vec<Value> = if result_ty == Type::Unit {
            Vec::new()
        } else {
            component_layout(result_ty.clone(), self.types, self.pointer_type)
                .iter()
                .map(|(component_type, _)| {
                    self.builder
                        .append_block_param(merge_block, *component_type)
                })
                .collect()
        };

        // 统一布局下，所有 variant 的字段都从 tag 之后（分量 1）开始。
        let variant_field_starts: Vec<usize> = variants.iter().map(|_| 1).collect();

        // 线性链式比较：每个 variant arm 生成一个比较，匹配则进入 body，否则继续下一个比较。
        for (arm_index, arm) in arms.iter().enumerate() {
            let is_last = arm_index + 1 == arms.len();
            match &arm.pattern {
                ir::MatchPattern::Variant { variant, bindings } => {
                    let arm_body = self.builder.create_block();
                    let next_check = self.builder.create_block();
                    let expected = self.builder.ins().iconst(types::I32, *variant as i64);
                    let matches = self.builder.ins().icmp(IntCC::Equal, tag, expected);
                    if is_last {
                        // 最后一个 variant：不匹配说明 tag 是未知值（穷尽性保证不可达），
                        // 这里 trap 以避免生成需要 merge 参数的不可达分支。
                        let unreachable_block = self.builder.create_block();
                        self.builder
                            .ins()
                            .brif(matches, arm_body, &[], unreachable_block, &[]);
                        self.builder.switch_to_block(unreachable_block);
                        self.builder.ins().trap(TrapCode::unwrap_user(2));
                    } else {
                        self.builder
                            .ins()
                            .brif(matches, arm_body, &[], next_check, &[]);
                    }

                    // body 块：解构绑定 + 执行 body。
                    self.builder.switch_to_block(arm_body);
                    self.emit_arm_body(
                        *variant,
                        bindings,
                        &arm.body,
                        &value,
                        variants,
                        &variant_field_starts,
                        &result_values,
                        merge_block,
                    );

                    self.builder.switch_to_block(next_check);
                }
                ir::MatchPattern::Wildcard => {
                    // 通配符作为 fallback，直接进入 body（匹配所有未命中的 tag）。
                    self.emit_arm_body(
                        0,
                        &[],
                        &arm.body,
                        &value,
                        variants,
                        &variant_field_starts,
                        &result_values,
                        merge_block,
                    );
                    break;
                }
            }
        }

        self.builder.switch_to_block(merge_block);
        RuntimeValue {
            ty: result_ty,
            values: result_values,
        }
    }

    /// 生成单个 match 分支体：解构绑定、执行 body、跳转到 merge。
    #[allow(clippy::too_many_arguments)]
    fn emit_arm_body(
        &mut self,
        variant: usize,
        bindings: &[ir::LocalId],
        body: &Expr,
        value: &RuntimeValue,
        variants: &[ir::EnumVariant],
        variant_field_starts: &[usize],
        result_values: &[Value],
        merge_block: Block,
    ) {
        if !bindings.is_empty() {
            let start = variant_field_starts[variant];
            let mut offset = start;
            for (local, field) in bindings.iter().zip(&variants[variant].fields) {
                let component_types: Vec<clif::Type> =
                    component_layout(field.clone(), self.types, self.pointer_type)
                        .into_iter()
                        .map(|(ty, _)| ty)
                        .collect();
                let width = component_types.len();
                let uniform_values = &value.values[offset..offset + width];
                let field_values = self.uniform_decode(uniform_values, &component_types);
                store_local(
                    self.builder,
                    &self.slots[local.0],
                    RuntimeValue {
                        ty: field.clone(),
                        values: field_values,
                    },
                    self.types,
                    self.pointer_type,
                );
                offset += width;
            }
        }
        let body_value = self.emit_expr(body);
        if result_values.is_empty() {
            self.builder.ins().jump(merge_block, &[]);
        } else {
            self.builder.ins().jump(
                merge_block,
                &body_value
                    .values
                    .iter()
                    .copied()
                    .map(Into::into)
                    .collect::<Vec<_>>(),
            );
        }
    }
}

fn store_local(
    builder: &mut FunctionBuilder<'_>,
    slot: &LocalSlot,
    value: RuntimeValue,
    types: &[TypeDef],
    pointer: clif::Type,
) {
    debug_assert_eq!(slot.ty, value.ty);
    let layout = component_layout(slot.ty.clone(), types, pointer);
    debug_assert_eq!(layout.len(), value.values.len());
    for ((_, offset), component) in layout.into_iter().zip(value.values) {
        builder
            .ins()
            .stack_store(pointer, component, slot.slot, offset);
    }
}

fn load_local(
    builder: &mut FunctionBuilder<'_>,
    slot: &LocalSlot,
    types: &[TypeDef],
    pointer: clif::Type,
) -> RuntimeValue {
    let values = component_layout(slot.ty.clone(), types, pointer)
        .into_iter()
        .map(|(component_type, offset)| {
            builder
                .ins()
                .stack_load(pointer, component_type, slot.slot, offset)
        })
        .collect();
    RuntimeValue {
        ty: slot.ty.clone(),
        values,
    }
}

enum CheckedOp {
    Add,
    Subtract,
    Multiply,
}

fn checked_binary(
    builder: &mut FunctionBuilder<'_>,
    left: Value,
    right: Value,
    operation: CheckedOp,
    signed: bool,
) -> Value {
    let (value, overflow) = match (operation, signed) {
        (CheckedOp::Add, true) => builder.ins().sadd_overflow(left, right),
        (CheckedOp::Subtract, true) => builder.ins().ssub_overflow(left, right),
        (CheckedOp::Multiply, true) => builder.ins().smul_overflow(left, right),
        (CheckedOp::Add, false) => builder.ins().uadd_overflow(left, right),
        (CheckedOp::Subtract, false) => builder.ins().usub_overflow(left, right),
        (CheckedOp::Multiply, false) => builder.ins().umul_overflow(left, right),
    };
    builder.ins().trapnz(overflow, TrapCode::INTEGER_OVERFLOW);
    value
}

fn component_layout(ty: Type, types: &[TypeDef], pointer: clif::Type) -> Vec<(clif::Type, i32)> {
    match ty {
        Type::Struct(id) => {
            // 结构体：字段按声明顺序排列，字段前补齐到其对齐（与 `layout` 一致）。
            let mut layout = Vec::new();
            if let TypeDef::Struct { fields, .. } = &types[id.0] {
                for (index, field) in fields.iter().enumerate() {
                    let base = layout::field_offset(fields, index, types, pointer.bytes());
                    for (component_type, component_offset) in
                        component_layout(field.ty.clone(), types, pointer)
                    {
                        layout.push((component_type, base as i32 + component_offset));
                    }
                }
            }
            layout
        }
        Type::Enum(id) => {
            // 枚举：一个 I32 标签 + payload 区，payload 统一用 I64 分量（bitcast 存储），
            // 偏移量来自公共布局，不在后端保留 `4 + ...` 常量。
            let enum_layout = layout::enum_layout_of(types, id, pointer.bytes());
            let mut layout = vec![(types::I32, enum_layout.tag_offset as i32)];
            for index in 0..enum_layout.payload_components {
                let offset =
                    enum_layout.payload_offset + index * layout::ENUM_PAYLOAD_COMPONENT_BYTES;
                layout.push((types::I64, offset as i32));
            }
            layout
        }
        Type::Ptr { .. } => vec![(pointer, 0)],
        Type::Slice { .. } => vec![(pointer, 0), (pointer, pointer.bytes() as i32)],
        Type::Null => vec![(pointer, 0)],
        _ => scalar_component_layout(ty, pointer),
    }
}

/// 标量与数组的组件布局（原 component_layout 的主体）。
fn scalar_component_layout(ty: Type, pointer: clif::Type) -> Vec<(clif::Type, i32)> {
    let (element, length) = match ty {
        ty if ty.is_integer() || ty.is_float() || ty == Type::Char => (ty.as_scalar().unwrap(), 1),
        Type::Bool => (ScalarType::Bool, 1),
        Type::String => (ScalarType::String, 1),
        Type::Array { element, length } => (element, length),
        Type::Unit => unreachable!("Unit has no runtime value"),
        Type::Struct(_) | Type::Enum(_) => unreachable!("user types handled by component_layout"),
        _ => unreachable!("all scalar types are covered"),
    };
    let pointer_bytes = pointer.bytes() as i32;
    let (component_types, stride): (&[clif::Type], i32) = match element {
        scalar
            if scalar.as_type().is_integer()
                || scalar.as_type().is_float()
                || scalar == ScalarType::Char =>
        {
            let ty = clif_type(scalar.as_type(), pointer);
            let stride = ty.bytes().max(1) as i32;
            // Stack array elements use their natural scalar width.
            return (0..length)
                .map(|index| (ty, index as i32 * stride))
                .collect();
        }
        ScalarType::Bool => (&[types::I8], 1),
        ScalarType::String => (&[pointer, pointer], pointer_bytes * 2),
        _ => unreachable!("all scalar types are covered"),
    };
    let mut layout = Vec::with_capacity(component_types.len() * length);
    for index in 0..length {
        for (component, component_type) in component_types.iter().enumerate() {
            let offset = index as i32 * stride + component as i32 * pointer_bytes;
            layout.push((*component_type, offset));
        }
    }
    layout
}

/// 计算一个类型的栈布局大小（字节数，含末尾对齐填充）。
fn size_of_type(ty: Type, types: &[TypeDef], pointer: clif::Type) -> i32 {
    layout::layout_of(&ty, types, pointer.bytes()).size as i32
}

/// 计算结构体字段在扁平分量布局中的字节偏移（与 `component_layout` 的拼接一致）。
fn field_offset_in_struct(
    fields: &[StructField],
    target: usize,
    types: &[TypeDef],
    pointer: clif::Type,
) -> i32 {
    layout::field_offset(fields, target, types, pointer.bytes()) as i32
}

fn scalar_alignment(ty: Type, types: &[TypeDef], pointer: clif::Type) -> u8 {
    // Cranelift 的栈槽对齐是 2 的幂指数（log2），而非字节数。
    layout::layout_of(&ty, types, pointer.bytes())
        .align
        .trailing_zeros() as u8
}

fn clif_type(ty: Type, pointer: clif::Type) -> clif::Type {
    match ty {
        Type::I8 | Type::U8 | Type::Bool => types::I8,
        Type::I16 | Type::U16 => types::I16,
        Type::I32 | Type::U32 | Type::Char => types::I32,
        Type::I64 | Type::U64 | Type::Usize | Type::Isize => types::I64,
        Type::F32 => types::F32,
        Type::F64 => types::F64,
        Type::String | Type::Ptr { .. } | Type::Slice { .. } | Type::Null => pointer,
        Type::Unit | Type::Array { .. } | Type::Struct(_) | Type::Enum(_) => {
            unreachable!("type has no single runtime value")
        }
    }
}
