//! macOS: статически собранный SDL написан частично на Objective-C и проверяет версию системы
//! через `@available`. Это вызов `__isPlatformVersionAtLeast` из служебной библиотеки clang
//! (compiler-rt), которую Rust сам не подключает. Подключаем её явно: путь берём у clang.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let Ok(out) = std::process::Command::new("clang").arg("--print-resource-dir").output() else {
        println!("cargo:warning=clang не найден — SDL может не слинковаться");
        return;
    };
    let dir = String::from_utf8_lossy(&out.stdout).trim().to_string();
    // Библиотека «толстая» (arm64 + x86_64), подходит и для универсальной сборки
    println!("cargo:rustc-link-search=native={dir}/lib/darwin");
    println!("cargo:rustc-link-lib=static=clang_rt.osx");
}
