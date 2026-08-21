//! Native dynamic-library support from `src/nvim/os/dl.c`.

type LibraryHandle = *mut std::ffi::c_void;

struct DynamicLibrary {
    handle: LibraryHandle,
}

impl DynamicLibrary {
    fn open(name: &[u8]) -> Option<Self> {
        let name = std::ffi::CString::new(name).ok()?;

        #[cfg(unix)]
        let handle =
            unsafe { libc::dlopen(name.as_ptr(), libc::RTLD_LAZY) };

        #[cfg(windows)]
        let handle = {
            #[link(name = "kernel32")]
            unsafe extern "system" {
                fn LoadLibraryA(name: *const u8) -> LibraryHandle;
            }
            unsafe { LoadLibraryA(name.as_ptr().cast()) }
        };

        #[cfg(not(any(unix, windows)))]
        let handle = std::ptr::null_mut();

        if handle.is_null() {
            None
        } else {
            Some(Self { handle })
        }
    }

    unsafe fn symbol(
        &self,
        name: &[u8],
    ) -> Option<*mut std::ffi::c_void> {
        let name = std::ffi::CString::new(name).ok()?;

        #[cfg(unix)]
        let symbol = unsafe { libc::dlsym(self.handle, name.as_ptr()) };

        #[cfg(windows)]
        let symbol = {
            #[link(name = "kernel32")]
            unsafe extern "system" {
                fn GetProcAddress(
                    module: LibraryHandle,
                    name: *const u8,
                ) -> *mut std::ffi::c_void;
            }
            unsafe { GetProcAddress(self.handle, name.as_ptr().cast()) }
        };

        #[cfg(not(any(unix, windows)))]
        let symbol = std::ptr::null_mut();

        (!symbol.is_null()).then_some(symbol)
    }
}

/// Result produced by [`os_libcall`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibCallResult {
    String(Option<Vec<u8>>),
    Integer(i32),
}

/// Load a native function and invoke one of Neovim's four supported
/// signatures (`os_libcall`).
///
/// `argv = Some` selects a string argument; `None` selects `argi`.
/// `want_string` selects a copied string result instead of an integer.
///
/// # Safety
/// The requested symbol must actually have the selected C signature
/// and must be safe to invoke with the supplied argument.
#[must_use]
pub unsafe fn os_libcall(
    libname: &[u8],
    funcname: &[u8],
    argv: Option<&[u8]>,
    argi: i32,
    want_string: bool,
) -> Option<LibCallResult> {
    let library = DynamicLibrary::open(libname)?;
    let function = unsafe { library.symbol(funcname) }?;
    let string_argument = match argv {
        Some(argument) => Some(std::ffi::CString::new(argument).ok()?),
        None => None,
    };

    if want_string {
        let result = if let Some(argument) = &string_argument {
            let function: unsafe extern "C" fn(
                *const std::ffi::c_char,
            ) -> *const std::ffi::c_char =
                unsafe { std::mem::transmute(function) };
            unsafe { function(argument.as_ptr()) }
        } else {
            let function: unsafe extern "C" fn(
                i32,
            ) -> *const std::ffi::c_char =
                unsafe { std::mem::transmute(function) };
            unsafe { function(argi) }
        };
        let address = result as usize;
        let copied = if result.is_null()
            || address == 1
            || address == usize::MAX
        {
            None
        } else {
            Some(
                unsafe { std::ffi::CStr::from_ptr(result) }
                    .to_bytes()
                    .to_vec(),
            )
        };
        Some(LibCallResult::String(copied))
    } else {
        let result = if let Some(argument) = &string_argument {
            let function: unsafe extern "C" fn(
                *const std::ffi::c_char,
            ) -> i32 = unsafe { std::mem::transmute(function) };
            unsafe { function(argument.as_ptr()) }
        } else {
            let function: unsafe extern "C" fn(i32) -> i32 =
                unsafe { std::mem::transmute(function) };
            unsafe { function(argi) }
        };
        Some(LibCallResult::Integer(result))
    }
}

impl Drop for DynamicLibrary {
    fn drop(&mut self) {
        #[cfg(unix)]
        unsafe {
            libc::dlclose(self.handle);
        }

        #[cfg(windows)]
        {
            #[link(name = "kernel32")]
            unsafe extern "system" {
                #[allow(dead_code)]
                fn FreeLibrary(module: LibraryHandle) -> i32;
            }
            unsafe {
                FreeLibrary(self.handle);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c_runtime_name() -> &'static [u8] {
        #[cfg(windows)]
        {
            b"ucrtbase.dll"
        }
        #[cfg(target_os = "macos")]
        {
            b"/usr/lib/libSystem.B.dylib"
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            b"libc.so.6"
        }
    }

    #[test]
    fn dynamic_library_rejects_interior_nul_names() {
        assert!(DynamicLibrary::open(b"bad\0name").is_none());
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot load native libraries")]
    fn dynamic_library_loads_the_c_runtime_and_abs_symbol() {
        let library =
            DynamicLibrary::open(c_runtime_name()).expect("C runtime");
        assert!(unsafe { library.symbol(b"abs") }.is_some());
        assert!(unsafe { library.symbol(b"nero_missing_symbol") }.is_none());
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call native libraries")]
    fn os_libcall_invokes_an_integer_function() {
        assert_eq!(
            unsafe {
                os_libcall(
                    c_runtime_name(),
                    b"abs",
                    None,
                    -17,
                    false,
                )
            },
            Some(LibCallResult::Integer(17))
        );
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call native libraries")]
    fn os_libcall_copies_a_string_result() {
        let result = unsafe {
            os_libcall(
                c_runtime_name(),
                b"strerror",
                None,
                2,
                true,
            )
        };
        let Some(LibCallResult::String(Some(message))) = result else {
            panic!("strerror should return a message");
        };
        assert!(!message.is_empty());
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call native libraries")]
    fn os_libcall_accepts_a_string_argument_for_integer_results() {
        assert_eq!(
            unsafe {
                os_libcall(
                    c_runtime_name(),
                    b"atoi",
                    Some(b"1234"),
                    0,
                    false,
                )
            },
            Some(LibCallResult::Integer(1234))
        );
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call native libraries")]
    fn os_libcall_reports_library_load_failures() {
        assert_eq!(
            unsafe {
                os_libcall(
                    b"nero-library-that-does-not-exist",
                    b"abs",
                    None,
                    -1,
                    false,
                )
            },
            None
        );
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call native libraries")]
    fn os_libcall_reports_symbol_lookup_failures() {
        assert_eq!(
            unsafe {
                os_libcall(
                    c_runtime_name(),
                    b"nero_missing_symbol",
                    None,
                    -1,
                    false,
                )
            },
            None
        );
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call native libraries")]
    fn os_libcall_rejects_interior_nul_arguments() {
        assert_eq!(
            unsafe {
                os_libcall(
                    c_runtime_name(),
                    b"atoi",
                    Some(b"PA\0TH"),
                    0,
                    false,
                )
            },
            None
        );
    }
}
