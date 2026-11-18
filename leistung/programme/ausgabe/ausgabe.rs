// Zwei Millionen Zeilen ausgeben. Misst die Ausgabe.

use std::io::{BufWriter, Write};

fn main() {
    // println! leert nach jeder Zeile; für viel Ausgabe nimmt man in Rust einen Puffer.
    let mut aus = BufWriter::new(std::io::stdout().lock());
    for i in 1..=2_000_000i64 {
        writeln!(aus, "{} {}", i, i * 3).unwrap();
    }
}
