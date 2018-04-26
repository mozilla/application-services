extern crate libc;

#[no_mangle]
pub extern fn version() -> String {
    extern {
        fn cjose_version() -> *const c_char;
    }

    use std::os::raw::c_char;
    use std::ffi::CStr;

    let c_buf: *const c_char = unsafe { cjose_version() };
    let c_str: &CStr = unsafe { CStr::from_ptr(c_buf) };

    let str_slice: &str = c_str.to_str().unwrap();

    str_slice.to_owned()
}

pub mod JWK {
    use libc::size_t;
    use std::os::raw::c_char;
    use std::ffi::CStr;
    //use libc::c_char;
    use std::str;

    extern {
        fn cjose_version() -> *const c_char;
    }

    pub fn asKey(u: usize) {
        let c_buf: *const c_char = unsafe { cjose_version() };
        let c_str: &CStr = unsafe { CStr::from_ptr(c_buf) };
        let str_slice: &str = c_str.to_str().unwrap();
        let str_buf: String = str_slice.to_owned();  // if necessary

        //let ver = unsafe { cjose_version() };
        println!("Version {:?}", str_slice);
        //let x = unsafe { snappy_max_compressed_length(u) };
        //println!("max compressed length of a 100 byte buffer: {}", x);
    }

}

//pub mod JWE {
//    use libc::size_t;
//    #[link(name = "snappy")]
//    extern {
//        fn snappy_max_compressed_length(source_length: size_t) -> size_t;
//    }
//
//    pub fn createDecrypt(u: usize, k: usize) {
//        let x = unsafe { snappy_max_compressed_length(u) };
//        //println!("max compressed length of a 100 byte buffer: {}", x);
//    }
//}

/// Expose the JNI interface for android below
#[cfg(target_os="android")]
#[allow(non_snake_case)]
pub mod android {
    extern crate jni;

    use super::*;
    use self::jni::JNIEnv;
    use self::jni::objects::{JClass, JString};
    use self::jni::sys::{jstring};
    use std::ffi::{CString, CStr};

    #[no_mangle]
    //pub unsafe extern fn Java_com_example_vladikoff_testjosec_RustJose_version(env: JNIEnv, _: JClass, java_pattern: JString) -> jstring {
    pub unsafe extern fn Java_com_reactlibrary2_RustJose_version(env: JNIEnv, _: JClass, java_pattern: JString) -> jstring {
        let ver = version();
        let output = env.new_string(ver).expect("Couldn't create java string!");

        output.into_inner()
    }
}