// Mandelbrot-Menge auf 1000 x 1000 Punkten. Höchstens 300 Schritte je Punkt; misst Rechnen mit Kommazahlen.

using System;

long summe = 0;
for (long py = 0; py < 1000; py++)
{
    double y0 = -1.5 + py * 3.0 / 1000.0;
    for (long px = 0; px < 1000; px++)
    {
        double x0 = -2.0 + px * 3.0 / 1000.0;
        double x = 0.0;
        double y = 0.0;
        long i = 0;
        while (i < 300 && x * x + y * y <= 4.0)
        {
            double xt = x * x - y * y + x0;
            y = 2.0 * x * y + y0;
            x = xt;
            i++;
        }
        summe += i;
    }
}
Console.WriteLine(summe);
