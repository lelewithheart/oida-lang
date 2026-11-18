//! Der OIDA-Compiler.
//!
//! Die Phasen, in dieser Reihenfolge:
//!
//! 1. [`woerterbuch`]: welche Dialekt-Wörter für welches Schlüsselwort stehen
//! 2. [`lexer`]: Quelltext -> Tokens
//! 3. [`parser`]: Tokens -> Syntaxbaum ([`ast`])
//! 4. [`typen`]: Typprüfung mit Typerkennung
//! 5. [`irgen`]: Syntaxbaum -> LLVM-IR als Text
//! 6. [`treiber`]: ruft `clang` auf und macht daraus die `.exe`
//!
//! Daneben gibt es [`editor`]: die Schnittstelle für die VS-Code-Erweiterung.

pub mod ast;
pub mod editor;
pub mod fehler;
pub mod irgen;
pub mod lexer;
pub mod parser;
pub mod treiber;
pub mod typen;
pub mod woerterbuch;
