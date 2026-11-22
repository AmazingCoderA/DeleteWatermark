//! Минимальная cdylib-обёртка.
//! Нужна только для того, чтобы cargo-ndk находил артефакт .so при
//! кросс-сборке под Android; вся логика программы живёт в бинарнике.

#[no_mangle]
pub extern "C" fn delete_watermark_android_stub() -> i32 {
    0
}
