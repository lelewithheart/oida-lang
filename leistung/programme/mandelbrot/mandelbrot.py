# Mandelbrot-Menge auf 1000 x 1000 Punkten, höchstens 300 Schritte je Punkt. Misst Rechnen mit Kommazahlen.

summe = 0
py = 0
while py < 1000:
    y0 = -1.5 + py * 3.0 / 1000.0
    px = 0
    while px < 1000:
        x0 = -2.0 + px * 3.0 / 1000.0
        x = 0.0
        y = 0.0
        i = 0
        while i < 300 and x * x + y * y <= 4.0:
            xt = x * x - y * y + x0
            y = 2.0 * x * y + y0
            x = xt
            i += 1
        summe += i
        px += 1
    py += 1
print(summe)
