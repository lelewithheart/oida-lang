// Längste Collatz-Folge für Startwerte unter 1 Million. Misst Schleifen, Verzweigungen und Rechnen mit ganzen Zahlen.

fn main() {
    let mut bester: i64 = 0;
    let mut laengste: i64 = 0;
    let mut start: i64 = 1;
    while start < 1_000_000 {
        let mut n = start;
        let mut schritte: i64 = 0;
        while n != 1 {
            if n % 2 == 0 {
                n /= 2;
            } else {
                n = 3 * n + 1;
            }
            schritte += 1;
        }
        if schritte > laengste {
            laengste = schritte;
            bester = start;
        }
        start += 1;
    }
    println!("{bester} {laengste}");
}
