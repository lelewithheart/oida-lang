// Mandelbrot-Menge auf 1000 x 1000 Punkten, höchstens 300 Schritte je Punkt. Misst Rechnen mit Kommazahlen.

#include <stdint.h>
#include <stdio.h>

int main(void) {
    int64_t summe = 0;
    for (int64_t py = 0; py < 1000; py++) {
        double y0 = -1.5 + (double)py * 3.0 / 1000.0;
        for (int64_t px = 0; px < 1000; px++) {
            double x0 = -2.0 + (double)px * 3.0 / 1000.0;
            double x = 0.0;
            double y = 0.0;
            int64_t i = 0;
            while (i < 300 && x * x + y * y <= 4.0) {
                double xt = x * x - y * y + x0;
                y = 2.0 * x * y + y0;
                x = xt;
                i++;
            }
            summe += i;
        }
    }
    printf("%lld\n", (long long)summe);
    return 0;
}
