//! Translated from `src/nvim/api/private/defs.h`.
//!
//! The original's `#include "api/private/defs.h.inline.generated.h"` is
//! neovim's C-only cross-translation-unit inline-declaration generator
//! (see `src/nvim/ascii_defs.rs` for the same note) - no Rust equivalent
//! needed.
//!
//! The `ArrayOf`/`DictOf`/`DictAs`/`Dict`/`Enum`/`DictHash`/`DictKey`/
//! `LuaRefOf`/`Union`/`Tuple` macros are pure codegen annotations that
//! decorate `.c` function *signatures* for `src/gen/gen_api_metadata.lua`;
//! they carry no runtime behavior. They have no standalone translation here
//!   - the type information they document is expressed directly in each
//!     translated function's real Rust signature/doc comment when that
//!     function is translated, instead of via a macro layer.

use crate::types_defs::{HandleT, LuaRef};

/// `Error.type` (`ErrorType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i8)]
pub enum ErrorType {
    None = -1,
    Exception = 0,
    Validation = 1,
}

/// Per msgpack-rpc spec (`MessageType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i8)]
pub enum MessageType {
    Unknown = -1,
    Request = 0,
    Response = 1,
    Notification = 2,
    RedrawEvent = 3,
}

/// Mask for all internal calls (`INTERNAL_CALL_MASK`).
pub const INTERNAL_CALL_MASK: u64 = 1u64 << 63;

/// Internal call from Vimscript code (`VIML_INTERNAL_CALL`).
pub const VIML_INTERNAL_CALL: u64 = INTERNAL_CALL_MASK;

/// Internal call from Lua code (`LUA_INTERNAL_CALL`).
pub const LUA_INTERNAL_CALL: u64 = VIML_INTERNAL_CALL + 1;

/// Check whether call is internal (`is_internal_call`).
///
/// * `channel_id` - Channel id.
///
/// Returns true if `channel_id` refers to an internal channel.
#[inline]
pub const fn is_internal_call(channel_id: u64) -> bool {
    (channel_id & INTERNAL_CALL_MASK) != 0
}

/// `Error`
#[derive(Debug, Clone)]
pub struct Error {
    pub r#type: ErrorType,
    pub msg: Option<std::string::String>,
}

impl Default for Error {
    /// `ERROR_INIT`
    fn default() -> Self {
        Error {
            r#type: ErrorType::None,
            msg: None,
        }
    }
}

impl Error {
    /// `ERROR_SET(e)`
    #[inline]
    pub fn is_set(&self) -> bool {
        self.r#type != ErrorType::None
    }
}

pub type Boolean = bool;
pub type Integer = i64;
pub type Float = f64;

/// Maximum value of an Integer (`API_INTEGER_MAX`).
pub const API_INTEGER_MAX: Integer = i64::MAX;
/// Minimum value of an Integer (`API_INTEGER_MIN`).
pub const API_INTEGER_MIN: Integer = i64::MIN;

/// `String` (from api/private/defs.h): a byte string with an explicit
/// length - unlike Rust's own `String`, it may contain invalid UTF-8 or
/// embedded NUL bytes, matching the original's `char *data; size_t size;`.
/// Named `NvimString` here purely to avoid shadowing `std::string::String`
/// throughout this crate; this is not a semantic change; `STRING_INIT`
/// corresponds to `NvimString::default()` (an empty vec, same as a NULL/0
/// data/size pair).
pub type NvimString = Vec<u8>;

// REMOTE_TYPE(Buffer); REMOTE_TYPE(Window); REMOTE_TYPE(Tabpage);
// `typedef handle_T <type>` - genuinely just aliases in the original (C
// gives no type-safety between them), so plain aliases are used here too
// rather than newtypes, to avoid silently adding type safety the original
// doesn't have.
pub type Buffer = HandleT;
pub type Window = HandleT;
pub type Tabpage = HandleT;

/// `struct object` / `Object`: a tagged union in the original
/// (`ObjectType type; union { ... } data;`). Rust's native sum type (`enum`)
/// is the direct, literal translation of a C tagged union - not a redesign.
#[derive(Debug, Clone)]
pub enum Object {
    Nil,
    Boolean(Boolean),
    Integer(Integer),
    Float(Float),
    String(NvimString),
    Array(Array),
    Dict(Dict),
    LuaRef(LuaRef),
    // EXT types, cannot be split or reordered, see EXT_OBJECT_TYPE_SHIFT.
    Buffer(Buffer),
    Window(Window),
    Tabpage(Tabpage),
}

impl Default for Object {
    /// `OBJECT_INIT`
    fn default() -> Self {
        Object::Nil
    }
}

impl Object {
    /// The original kept a separate `ObjectType` tag alongside the union;
    /// this recovers the same tag from a Rust `Object` value (there is
    /// nothing to keep out of sync since the enum discriminant *is* the
    /// tag).
    pub fn object_type(&self) -> ObjectType {
        match self {
            Object::Nil => ObjectType::Nil,
            Object::Boolean(_) => ObjectType::Boolean,
            Object::Integer(_) => ObjectType::Integer,
            Object::Float(_) => ObjectType::Float,
            Object::String(_) => ObjectType::String,
            Object::Array(_) => ObjectType::Array,
            Object::Dict(_) => ObjectType::Dict,
            Object::LuaRef(_) => ObjectType::LuaRef,
            Object::Buffer(_) => ObjectType::Buffer,
            Object::Window(_) => ObjectType::Window,
            Object::Tabpage(_) => ObjectType::Tabpage,
        }
    }
}

/// `typedef kvec_t(Object) Array;` - note this is a plain growable vector,
/// not a set: order and duplicates are preserved exactly like the original.
pub type Array = Vec<Object>;

/// `struct key_value_pair` / `KeyValuePair`.
#[derive(Debug, Clone)]
pub struct KeyValuePair {
    pub key: NvimString,
    pub value: Object,
}

/// `typedef kvec_t(KeyValuePair) Dict;` - note this is genuinely an
/// *ordered vector of pairs*, not a hash map, in the original (small dicts,
/// insertion order preserved, O(n) lookup) - preserved as such here rather
/// than "upgrading" to a `HashMap`/`IndexMap`, since that would change
/// iteration order and duplicate-key behavior.
pub type Dict = Vec<KeyValuePair>;

pub type StringArray = Vec<NvimString>;

/// `ObjectType`. Exact discriminant values are preserved: they matter for
/// the `EXT_OBJECT_TYPE_SHIFT`/`EXT_OBJECT_TYPE_MAX` msgpack EXT-type
/// arithmetic below (used by `api/private/converter.c`, not yet
/// translated - phase 12).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum ObjectType {
    Nil = 0,
    Boolean,
    Integer,
    Float,
    String,
    Array,
    Dict,
    LuaRef,
    // EXT types, cannot be split or reordered, see EXT_OBJECT_TYPE_SHIFT.
    Buffer,
    Window,
    Tabpage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i8)]
pub enum UnpackType {
    StringArray = -1,
}

/// Value by which objects represented as EXT type are shifted
/// (`EXT_OBJECT_TYPE_SHIFT`).
///
/// Subtracted when packing, added when unpacking. Used to allow moving the
/// buffer/window/tabpage block inside `ObjectType`. This block yet cannot be
/// split or reordered.
pub const EXT_OBJECT_TYPE_SHIFT: i32 = ObjectType::Buffer as i32;
pub const EXT_OBJECT_TYPE_MAX: i32 = ObjectType::Tabpage as i32 - EXT_OBJECT_TYPE_SHIFT;

pub type OptionalKeys = u64;
pub type HlGroupId = Integer;

/// This is the prefix of all keysets with optional keys (`OptKeySet`).
#[derive(Debug, Clone, Copy, Default)]
pub struct OptKeySet {
    pub is_set_: OptionalKeys,
}

#[derive(Debug, Clone, Copy)]
pub struct KeySetLink {
    pub str: *const std::os::raw::c_char,
    pub ptr_off: usize,
    /// `ObjectType` or `UnpackType`; `ObjectType::Nil` (0) means untyped -
    /// kept as a raw `i32` like the original rather than inventing a
    /// combined Rust enum, since the original deliberately overloads one
    /// field with two different enums.
    pub r#type: i32,
    pub opt_index: i32,
    pub is_hlgroup: bool,
}

pub type FieldHashfn = extern "C" fn(str: *const std::os::raw::c_char, len: usize) -> *const KeySetLink;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_call_mask_matches_c_macro() {
        // INTERNAL_CALL_MASK = ((uint64_t)1) << (sizeof(uint64_t) * 8 - 1)
        assert_eq!(INTERNAL_CALL_MASK, 1u64 << 63);
        assert!(is_internal_call(VIML_INTERNAL_CALL));
        assert!(is_internal_call(LUA_INTERNAL_CALL));
        assert_eq!(LUA_INTERNAL_CALL, VIML_INTERNAL_CALL + 1);
        assert!(!is_internal_call(42));
    }

    #[test]
    fn ext_object_type_shift_matches_original_layout() {
        // Nil=0, Boolean=1, Integer=2, Float=3, String=4, Array=5, Dict=6,
        // LuaRef=7, Buffer=8, Window=9, Tabpage=10 in the original enum.
        assert_eq!(ObjectType::Buffer as i32, 8);
        assert_eq!(ObjectType::Window as i32, 9);
        assert_eq!(ObjectType::Tabpage as i32, 10);
        assert_eq!(EXT_OBJECT_TYPE_SHIFT, 8);
        assert_eq!(EXT_OBJECT_TYPE_MAX, 2);
    }

    #[test]
    fn object_type_tag_round_trips() {
        assert_eq!(Object::Nil.object_type(), ObjectType::Nil);
        assert_eq!(Object::Integer(5).object_type(), ObjectType::Integer);
        assert_eq!(
            Object::String(b"hi".to_vec()).object_type(),
            ObjectType::String
        );
        assert_eq!(Object::Buffer(3).object_type(), ObjectType::Buffer);
    }

    #[test]
    fn error_default_is_unset() {
        let e = Error::default();
        assert!(!e.is_set());
        let e2 = Error {
            r#type: ErrorType::Validation,
            msg: Some("bad".into()),
        };
        assert!(e2.is_set());
    }
}
