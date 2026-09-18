//! 统一的类型布局计算（M14-A/C）。
//!
//! M14 规格要求「集中实现 `layout_of(T)`」，禁止 codegen、分配器和 `mem.size_of`
//! 各算一套布局。本模块是唯一实现，前端（`lower` 的 `mem.size_of` / `mem.alloc`）
//! 与后端（`codegen` 的栈槽、字段偏移、切片步长）都调用这里的函数。
//!
//! 结构体按字段声明顺序排列，字段前补齐到其对齐，末尾补齐到最大字段对齐；
//! 这与 M13 的「紧密拼接」不同，M14 起以对齐布局为准。

use crate::ir::{ScalarType, StructField, Type, TypeDef};

/// 当前支持的三个官方目标（Linux x86_64、macOS ARM64、Windows x86_64）
/// 都是 64 位，指针宽度固定为 8。
pub const POINTER_BYTES: u32 = 8;

/// 宿主 C `long` 的字节数（Windows MSVC 为 4，Linux/macOS 为 8）。
///
/// `c_long` / `c_ulong` 是平台相关别名，不能用固定宽度代替。
pub fn host_c_long_bytes() -> u32 {
    std::mem::size_of::<std::os::raw::c_long>() as u32
}

/// 一个类型的字节大小与对齐。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    pub size: u32,
    pub align: u32,
}

/// 单个聚合类型允许的最大字节数。超过此大小的一律拒绝，避免布局在 `u32`
/// 中回绕成极小值而导致后端分配不足、写越界。
pub const MAX_AGGREGATE_BYTES: u64 = i32::MAX as u64;

/// 枚举 tag（判别值）的字节大小；当前三个 64 位目标统一为 i32。
pub const ENUM_TAG_BYTES: u32 = 4;

/// 枚举 payload 统一分量的字节大小；每个分量按 i64 存储。
pub const ENUM_PAYLOAD_COMPONENT_BYTES: u32 = 8;

/// 枚举的统一字节布局：tag 在偏移 0，payload 分量从 8 字节边界开始，
/// 每个分量 8 字节；完全没有 payload 时 size=4、align=4。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EnumLayout {
    pub tag_offset: u32,
    pub payload_offset: u32,
    pub payload_components: u32,
    pub size: u64,
    pub align: u32,
}

fn align_up(value: u32, align: u32) -> u32 {
    debug_assert!(align.is_power_of_two());
    (value as u64)
        .div_ceil(align as u64)
        .saturating_mul(align as u64)
        .min(u32::MAX as u64) as u32
}

fn align_up_u64(value: u64, align: u32) -> u64 {
    debug_assert!(align.is_power_of_two());
    let align = u64::from(align);
    value.div_ceil(align) * align
}

/// 数组元素的分量步长与尾部字节。布尔元素与其它标量一致使用自然宽度，
/// 这样 `size_of_type(bool)`、`mem.alloc<bool>` 与数组寻址的步长保持一致。
fn array_stride_tail(element: ScalarType, pointer_bytes: u32) -> (u32, u32) {
    let bytes = scalar_bytes(element, pointer_bytes);
    (bytes, bytes)
}

/// 数组类型的精确字节数（以 `u64` 计算，不会溢出）。
pub fn array_byte_size(element: ScalarType, length: usize, pointer_bytes: u32) -> u64 {
    if length == 0 {
        return 0;
    }
    let (stride, tail) = array_stride_tail(element, pointer_bytes);
    (length as u64 - 1)
        .saturating_mul(stride as u64)
        .saturating_add(tail as u64)
}

/// 标量（含 `string` 视图）的字节大小。
fn scalar_bytes(scalar: ScalarType, pointer_bytes: u32) -> u32 {
    match scalar {
        ScalarType::I8 | ScalarType::U8 | ScalarType::Bool => 1,
        ScalarType::I16 | ScalarType::U16 => 2,
        ScalarType::I32 | ScalarType::U32 | ScalarType::F32 | ScalarType::Char => 4,
        ScalarType::I64
        | ScalarType::U64
        | ScalarType::Usize
        | ScalarType::Isize
        | ScalarType::F64 => 8,
        ScalarType::String => pointer_bytes * 2,
    }
}

/// 计算类型布局。`pointer_bytes` 是目标指针宽度（当前 8）。
pub fn layout_of(ty: &Type, types: &[TypeDef], pointer_bytes: u32) -> Layout {
    match ty {
        Type::Unit => Layout { size: 0, align: 1 },
        Type::Null => Layout {
            size: pointer_bytes,
            align: pointer_bytes,
        },
        Type::Ptr { .. } => Layout {
            size: pointer_bytes,
            align: pointer_bytes,
        },
        Type::Slice { .. } => Layout {
            size: pointer_bytes * 2,
            align: pointer_bytes,
        },
        Type::Array { element, length } => {
            let size =
                array_byte_size(*element, *length, pointer_bytes).min(u32::MAX as u64) as u32;
            let align = match element {
                ScalarType::String => pointer_bytes,
                other => scalar_bytes(*other, pointer_bytes).max(1),
            };
            Layout { size, align }
        }
        Type::Struct(id) => struct_layout_of(types, *id, pointer_bytes).1,
        Type::Enum(id) => {
            let layout = enum_layout_of(types, *id, pointer_bytes);
            debug_assert!(
                layout.size <= MAX_AGGREGATE_BYTES,
                "enum layout must be rejected during type instantiation, not saturated here"
            );
            Layout {
                // 类型实例化用 `checked_size_of` 拒绝超过预算的类型；对未经验证的
                // IR 取 u32 上界，避免回绕成可分配的小尺寸。
                size: layout.size.min(u64::from(u32::MAX)) as u32,
                align: layout.align,
            }
        }
        ty => match ty.as_scalar() {
            Some(ScalarType::String) => Layout {
                size: pointer_bytes * 2,
                align: pointer_bytes,
            },
            Some(scalar) => {
                let bytes = scalar_bytes(scalar, pointer_bytes);
                Layout {
                    size: bytes,
                    align: bytes,
                }
            }
            None => Layout { size: 0, align: 1 },
        },
    }
}

/// 枚举的统一字节布局描述：tag 在偏移 0；有 payload 分量时 payload 从
/// `align_up(tag, 8) = 8` 开始，每个统一分量 8 字节，整体 align=8；
/// 完全没有 payload 时 size=4、align=4。
pub fn enum_layout_of(types: &[TypeDef], id: crate::ir::TypeId, pointer_bytes: u32) -> EnumLayout {
    let payload_components = enum_payload_components(types, id, pointer_bytes);
    let payload_offset = align_up(ENUM_TAG_BYTES, ENUM_PAYLOAD_COMPONENT_BYTES);
    if payload_components == 0 {
        EnumLayout {
            tag_offset: 0,
            payload_offset,
            payload_components,
            size: u64::from(ENUM_TAG_BYTES),
            align: ENUM_TAG_BYTES,
        }
    } else {
        EnumLayout {
            tag_offset: 0,
            payload_offset,
            payload_components,
            size: enum_payload_size_bytes(payload_components),
            align: ENUM_PAYLOAD_COMPONENT_BYTES,
        }
    }
}

/// 有 payload 枚举的精确字节大小（`u64`，不饱和）；`8 + n * 8`。
pub fn enum_payload_size_bytes(payload_components: u32) -> u64 {
    u64::from(ENUM_PAYLOAD_COMPONENT_BYTES) * (u64::from(payload_components) + 1)
}

/// 精确计算类型布局（`u64`，不做饱和），供类型实例化检查聚合预算。
/// 规则与 `layout_of` 相同；超过 `MAX_AGGREGATE_BYTES` 时调用方应给出诊断。
pub fn checked_layout_of(ty: &Type, types: &[TypeDef], pointer_bytes: u32) -> (u64, u32) {
    match ty {
        Type::Unit => (0, 1),
        Type::Null | Type::Ptr { .. } => (u64::from(pointer_bytes), pointer_bytes),
        Type::Slice { .. } | Type::String => (u64::from(pointer_bytes) * 2, pointer_bytes),
        Type::Array { element, length } => {
            let align = match element {
                ScalarType::String => pointer_bytes,
                other => scalar_bytes(*other, pointer_bytes).max(1),
            };
            (array_byte_size(*element, *length, pointer_bytes), align)
        }
        Type::Struct(id) => {
            let TypeDef::Struct { fields, .. } = &types[id.0] else {
                return (0, 1);
            };
            let mut offset = 0u64;
            let mut max_align = 1u32;
            for field in fields {
                let (size, align) = checked_layout_of(&field.ty, types, pointer_bytes);
                let align = align.max(1);
                offset = align_up_u64(offset, align);
                offset += size;
                max_align = max_align.max(align);
            }
            (align_up_u64(offset, max_align), max_align)
        }
        Type::Enum(id) => {
            let layout = enum_layout_of(types, *id, pointer_bytes);
            (layout.size, layout.align)
        }
        _ => match ty.as_scalar() {
            Some(ScalarType::String) => (u64::from(pointer_bytes) * 2, pointer_bytes),
            Some(scalar) => {
                let bytes = scalar_bytes(scalar, pointer_bytes);
                (u64::from(bytes), bytes)
            }
            None => (0, 1),
        },
    }
}

/// 精确类型大小（字节，`u64`）。
pub fn checked_size_of(ty: &Type, types: &[TypeDef], pointer_bytes: u32) -> u64 {
    checked_layout_of(ty, types, pointer_bytes).0
}

/// 结构体每个字段的字节偏移（与 `layout_of` 一致）以及整体布局。
pub fn struct_layout_of(
    types: &[TypeDef],
    id: crate::ir::TypeId,
    pointer_bytes: u32,
) -> (Vec<u32>, Layout) {
    let TypeDef::Struct { fields, .. } = &types[id.0] else {
        unreachable!("struct_layout_of resolves a struct");
    };
    let mut offsets = Vec::with_capacity(fields.len());
    let mut offset = 0u32;
    let mut max_align = 1u32;
    for field in fields {
        let layout = layout_of(&field.ty, types, pointer_bytes);
        let align = layout.align.max(1);
        offset = align_up(offset, align);
        offsets.push(offset);
        offset = offset.saturating_add(layout.size);
        max_align = max_align.max(align);
    }
    let size = align_up(offset, max_align);
    (
        offsets,
        Layout {
            size,
            align: max_align,
        },
    )
}

/// 结构体字段的字节偏移（按字段下标）。
pub fn field_offset(
    fields: &[StructField],
    target: usize,
    types: &[TypeDef],
    pointer_bytes: u32,
) -> u32 {
    let mut offset = 0u32;
    for (index, field) in fields.iter().enumerate() {
        let layout = layout_of(&field.ty, types, pointer_bytes);
        offset = align_up(offset, layout.align.max(1));
        if index == target {
            return offset;
        }
        offset = offset.saturating_add(layout.size);
    }
    unreachable!("field index was validated during lowering")
}

/// 扁平化为运行时分量后的分量个数（与 `codegen::component_layout` 一致）。
pub fn component_count(ty: &Type, types: &[TypeDef], pointer_bytes: u32) -> u32 {
    match ty {
        Type::Unit => 0,
        Type::Ptr { .. } => 1,
        Type::Slice { .. } | Type::String => 2,
        Type::Array { element, length } => {
            let element = component_count(&element.as_type(), types, pointer_bytes);
            (element as u64)
                .saturating_mul(*length as u64)
                .min(u32::MAX as u64) as u32
        }
        Type::Struct(id) => match &types[id.0] {
            TypeDef::Struct { fields, .. } => fields
                .iter()
                .map(|field| component_count(&field.ty, types, pointer_bytes))
                .sum(),
            _ => 0,
        },
        Type::Enum(id) => 1 + enum_payload_components(types, *id, pointer_bytes),
        _ => 1,
    }
}

/// 枚举 payload 的统一分量数（取最大 variant）。
pub fn enum_payload_components(
    types: &[TypeDef],
    id: crate::ir::TypeId,
    pointer_bytes: u32,
) -> u32 {
    match &types[id.0] {
        TypeDef::Enum { variants } => variants
            .iter()
            .map(|variant| {
                variant
                    .fields
                    .iter()
                    .map(|field| component_count(field, types, pointer_bytes))
                    .sum::<u32>()
            })
            .max()
            .unwrap_or(0),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{EnumVariant, StructField, TypeId};

    fn enum_types(variants: Vec<(&str, Vec<Type>)>) -> (Vec<TypeDef>, TypeId) {
        let variants = variants
            .into_iter()
            .map(|(name, fields)| EnumVariant {
                name: name.to_string(),
                fields,
            })
            .collect();
        (vec![TypeDef::Enum { variants }], TypeId(0))
    }

    fn field(name: &str, ty: Type) -> StructField {
        StructField {
            name: name.to_string(),
            ty,
            public: true,
        }
    }

    #[test]
    fn enum_without_payload_is_four_four() {
        let (types, id) = enum_types(vec![("A", vec![]), ("B", vec![])]);
        let layout = enum_layout_of(&types, id, POINTER_BYTES);
        assert_eq!(layout.tag_offset, 0);
        assert_eq!(layout.payload_offset, 8);
        assert_eq!(layout.payload_components, 0);
        assert_eq!((layout.size, layout.align), (4, 4));
        assert_eq!(
            layout_of(&Type::Enum(id), &types, POINTER_BYTES),
            Layout { size: 4, align: 4 }
        );
    }

    #[test]
    fn enum_payload_starts_at_eight_and_size_is_padded() {
        let (types, id) = enum_types(vec![("V", vec![Type::I64]), ("E", vec![])]);
        let layout = enum_layout_of(&types, id, POINTER_BYTES);
        assert_eq!(
            (
                layout.tag_offset,
                layout.payload_offset,
                layout.payload_components,
                layout.size,
                layout.align
            ),
            (0, 8, 1, 16, 8)
        );
        assert_eq!(layout.size % u64::from(layout.align), 0);

        let (types, id) = enum_types(vec![("Triple", vec![Type::F64, Type::F64, Type::F64])]);
        let layout = enum_layout_of(&types, id, POINTER_BYTES);
        assert_eq!(
            (layout.payload_components, layout.size, layout.align),
            (3, 32, 8)
        );
        assert_eq!(
            layout_of(&Type::Enum(id), &types, POINTER_BYTES),
            Layout { size: 32, align: 8 }
        );
    }

    #[test]
    fn enum_payload_size_is_exact_for_max_components() {
        assert_eq!(enum_payload_size_bytes(0), 8);
        assert_eq!(enum_payload_size_bytes(1), 16);
        assert_eq!(
            enum_payload_size_bytes(u32::MAX),
            u64::from(ENUM_PAYLOAD_COMPONENT_BYTES) * (u64::from(u32::MAX) + 1)
        );
    }

    #[test]
    fn enum_in_struct_uses_aligned_field_offsets() {
        let types = vec![
            TypeDef::Struct {
                fields: vec![
                    field("head", Type::I32),
                    field("shape", Type::Enum(TypeId(1))),
                    field("tail", Type::I32),
                ],
                extern_c: false,
            },
            TypeDef::Enum {
                variants: vec![EnumVariant {
                    name: "Circle".to_string(),
                    fields: vec![Type::F64],
                }],
            },
        ];
        let (offsets, layout) = struct_layout_of(&types, TypeId(0), POINTER_BYTES);
        assert_eq!(offsets, vec![0, 8, 24]);
        assert_eq!((layout.size, layout.align), (32, 8));
        assert_eq!(
            checked_size_of(&Type::Struct(TypeId(0)), &types, POINTER_BYTES),
            u64::from(layout.size)
        );
    }
}
