//! Translated from `src/nvim/api/private/converter.c`.
//!
//! The `typval_T` to API [`Object`] direction is complete. The reverse
//! direction is translated separately below once its allocation/
//! ownership tests are in place.

use std::collections::HashSet;

use crate::api::private::defs::{KeyValuePair, Object};
use crate::eval::typval_defs::{BoolVarValue, TypvalT, TypvalValue};

fn vim_to_object_inner(
    value: &TypvalT,
    visiting: &mut HashSet<usize>,
) -> Object {
    match &value.value {
        TypvalValue::Unknown | TypvalValue::Special(_) => Object::Nil,
        TypvalValue::Bool(BoolVarValue::False) => Object::Boolean(false),
        TypvalValue::Bool(BoolVarValue::True) => Object::Boolean(true),
        TypvalValue::Number(value) => Object::Integer(*value),
        TypvalValue::Float(value) => Object::Float(*value),
        TypvalValue::String(value) => Object::String(value.clone().unwrap_or_default()),
        TypvalValue::Blob(blob) => {
            if blob.is_null() {
                return Object::String(Vec::new());
            }
            let blob = unsafe { &**blob };
            Object::String(blob.bv_ga.ga_data[..blob.bv_ga.ga_len as usize].to_vec())
        }
        TypvalValue::List(list) => {
            if list.is_null() {
                return Object::Array(Vec::new());
            }
            let address = *list as usize;
            if !visiting.insert(address) {
                return Object::Nil;
            }
            let mut result = Vec::with_capacity(unsafe { (**list).lv_len } as usize);
            let mut item = unsafe { (**list).lv_first };
            while !item.is_null() {
                result.push(vim_to_object_inner(unsafe { &(*item).li_tv }, visiting));
                item = unsafe { (*item).li_next };
            }
            visiting.remove(&address);
            Object::Array(result)
        }
        TypvalValue::Dict(dict) => {
            if dict.is_null() {
                return Object::Dict(Vec::new());
            }
            let address = *dict as usize;
            if !visiting.insert(address) {
                return Object::Nil;
            }
            let mut result = Vec::with_capacity(unsafe { (**dict).dv_index.len() });
            for item in unsafe { (**dict).dv_index.values() } {
                let item = unsafe { &**item };
                let key_len = item.di_key.len().saturating_sub(1);
                result.push(KeyValuePair {
                    key: item.di_key[..key_len].to_vec(),
                    value: vim_to_object_inner(&item.di_tv, visiting),
                });
            }
            visiting.remove(&address);
            Object::Dict(result)
        }
        TypvalValue::Func(_) | TypvalValue::Partial(_) => {
            // The original only exports Lua-backed functions as
            // LuaRef objects. The Lua host is not translated, so no
            // current function/partial can satisfy that branch.
            Object::Nil
        }
    }
}

/// Convert a Vimscript value to an API object (`vim_to_object`).
#[must_use]
pub fn vim_to_object(value: &TypvalT) -> Object {
    vim_to_object_inner(value, &mut HashSet::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::typval::{tv_dict_add_nr, tv_dict_alloc, tv_dict_unref, tv_list_alloc, tv_list_append_number, tv_list_unref};

    #[test]
    fn vim_to_object_converts_scalar_values() {
        assert!(matches!(
            vim_to_object(&TypvalT {
                value: TypvalValue::Bool(BoolVarValue::True),
                ..Default::default()
            }),
            Object::Boolean(true)
        ));
        assert!(matches!(
            vim_to_object(&TypvalT {
                value: TypvalValue::Number(42),
                ..Default::default()
            }),
            Object::Integer(42)
        ));
        assert!(matches!(
            vim_to_object(&TypvalT {
                value: TypvalValue::String(Some(b"text".to_vec())),
                ..Default::default()
            }),
            Object::String(value) if value == b"text"
        ));
        assert!(matches!(
            vim_to_object(&TypvalT {
                value: TypvalValue::Special(crate::eval::typval_defs::SpecialVarValue::Null),
                ..Default::default()
            }),
            Object::Nil
        ));
    }

    #[test]
    fn vim_to_object_converts_list_and_dict_containers() {
        let _lock = crate::globals::global_state_test_lock();
        let list = tv_list_alloc(2);
        unsafe {
            tv_list_append_number(list, 3);
            tv_list_append_number(list, 7);
        }
        let dict = tv_dict_alloc();
        assert_eq!(unsafe { tv_dict_add_nr(&mut *dict, b"items", 2) }, crate::vim_defs::OK);

        let list_object = vim_to_object(&TypvalT {
            value: TypvalValue::List(list),
            ..Default::default()
        });
        let dict_object = vim_to_object(&TypvalT {
            value: TypvalValue::Dict(dict),
            ..Default::default()
        });

        assert!(matches!(
            list_object,
            Object::Array(ref items)
                if matches!(items.as_slice(), [Object::Integer(3), Object::Integer(7)])
        ));
        assert!(matches!(
            dict_object,
            Object::Dict(ref items)
                if items.len() == 1
                    && items[0].key == b"items"
                    && matches!(items[0].value, Object::Integer(2))
        ));
        unsafe {
            tv_list_unref(list);
            tv_dict_unref(dict);
        }
    }

    #[test]
    fn vim_to_object_converts_recursive_items_to_nil() {
        let _lock = crate::globals::global_state_test_lock();
        let list = tv_list_alloc(1);
        unsafe {
            crate::eval::typval::tv_list_append_owned_tv(
                list,
                TypvalT {
                    value: TypvalValue::List(list),
                    ..Default::default()
                },
            );
        }
        let object = vim_to_object(&TypvalT {
            value: TypvalValue::List(list),
            ..Default::default()
        });
        assert!(matches!(
            object,
            Object::Array(ref items) if matches!(items.as_slice(), [Object::Nil])
        ));

        // Break the cycle before normal recursive cleanup.
        unsafe { (*(*list).lv_first).li_tv = TypvalT::default() };
        unsafe { tv_list_unref(list) };
    }
}
