// Mandelbrot-Menge auf 1000 x 1000 Punkten, höchstens 300 Schritte je Punkt. Misst Rechnen mit Kommazahlen.

fn main() {
    let mut summe: i64 = 0;
    for py in 0..1000i64 {
        let y0 = -1.5 + py as f64 * 3.0 / 1000.0;
        for px in 0..1000i64 {
            let x0 = -2.0 + px as f64 * 3.0 / 1000.0;
            let mut x = 0.0f64;
            let mut y = 0.0f64;
            let mut i: i64 = 0;
            while i < 300 && x * x + y * y <= 4.0 {
                let xt = x * x - y * y + x0;
                y = 2.0 * x * y + y0;
                x = xt;
                i += 1;
            }
            summe += i;
        }
    }
    println!("{summe}");
}
