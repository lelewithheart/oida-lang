// Mandelbrot-Menge auf 1000 x 1000 Punkten, höchstens 300 Schritte je Punkt. Misst Rechnen mit Kommazahlen.

let summe = 0;
for (let py = 0; py < 1000; py++) {
    const y0 = -1.5 + py * 3.0 / 1000.0;
    for (let px = 0; px < 1000; px++) {
        const x0 = -2.0 + px * 3.0 / 1000.0;
        let x = 0.0;
        let y = 0.0;
        let i = 0;
        while (i < 300 && x * x + y * y <= 4.0) {
            const xt = x * x - y * y + x0;
            y = 2.0 * x * y + y0;
            x = xt;
            i++;
        }
        summe += i;
    }
}
console.log(summe);
