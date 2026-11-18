// Primzahlen unter 2 Millionen zählen, durch Probeteilen. Misst Schleifen und Teilen mit Rest.

fn ist_prim(n: i64) -> bool {
    let mut t = 2;
    while t * t <= n {
        if n % t == 0 {
            return false;
        }
        t += 1;
    }
    true
}

fn main() {
    let mut anzahl: i64 = 0;
    let mut n: i64 = 2;
    while n < 2_000_000 {
        if ist_prim(n) {
            anzahl += 1;
        }
        n += 1;
    }
    println!("{anzahl}");
}
