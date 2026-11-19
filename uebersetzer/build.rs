//! Baut die Laufzeitbibliothek (`laufzeit/`) und merkt sich, wo sie liegt.
//! Der Compiler packt sie mit `include_bytes!` in die eigene `oida.exe`,
//! damit man später nur diese eine Datei braucht.

use std::env;
use std::path::PathBuf;
use std::process::Command;

/// Für welches Ziel OIDA Programme erzeugt: (wie Rust es nennt, wie clang es nennt).
/// Unter Windows passt das zu llvm-mingw; sonst ist es das System, für das der
/// Compiler selbst gebaut wird (unter Linux heißt es bei beiden gleich).
fn ziel() -> (String, String) {
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        ("x86_64-pc-windows-gnullvm".to_string(), "x86_64-w64-windows-gnu".to_string())
    } else {
        let ziel = env::var("TARGET").expect("cargo sagt, wofür gebaut wird");
        (ziel.clone(), ziel)
    }
}

fn main() {
    let (ziel, ir_ziel) = ziel();
    let ziel = ziel.as_str();
    println!("cargo:rustc-env=OIDA_IR_ZIEL={ir_ziel}");
    let hier = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let wurzel = hier.parent().unwrap().to_path_buf();
    let laufzeit = wurzel.join("laufzeit");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", laufzeit.join("src").display());
    println!("cargo:rerun-if-changed={}", laufzeit.join("Cargo.toml").display());
    println!("cargo:rerun-if-changed={}", wurzel.join("Cargo.toml").display());

    // Eigener Bauordner, damit sich das innere cargo nicht mit dem äußeren
    // um die Sperre auf target/ streitet.
    let bauordner = PathBuf::from(env::var("OUT_DIR").unwrap()).join("laufzeit-bau");
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());

    let status = Command::new(cargo)
        .args(["build", "--profile", "laufzeit", "--target", ziel, "--manifest-path"])
        .arg(laufzeit.join("Cargo.toml"))
        .arg("--target-dir")
        .arg(&bauordner)
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env_remove("RUSTFLAGS")
        .status()
        .expect("cargo für die Laufzeitbibliothek lässt sich nicht starten");
    if !status.success() {
        panic!(
            "Die Laufzeitbibliothek lässt sich nicht bauen. Ist das Ziel da? rustup target add {ziel}"
        );
    }

    let bibliothek = bauordner.join(ziel).join("laufzeit").join("liboida_laufzeit.a");
    if !bibliothek.exists() {
        panic!("Laufzeitbibliothek nicht gefunden: {}", bibliothek.display());
    }
    println!("cargo:rustc-env=OIDA_LAUFZEIT_BIBLIOTHEK={}", bibliothek.display());

    // Eine Kennung für genau diesen Stand der Bibliothek (FNV-1a über den Inhalt).
    // Unter diesem Namen legt der Compiler sie später im Temp-Ordner ab.
    let inhalt = std::fs::read(&bibliothek).expect("Laufzeitbibliothek lässt sich nicht lesen");
    let mut kennung: u64 = 0xcbf29ce484222325;
    for byte in &inhalt {
        kennung ^= u64::from(*byte);
        kennung = kennung.wrapping_mul(0x100000001b3);
    }
    println!("cargo:rustc-env=OIDA_LAUFZEIT_KENNUNG={kennung:016x}");
}
