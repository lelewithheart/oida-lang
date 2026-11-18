# Längste Collatz-Folge für Startwerte unter 1 Million. Misst Schleifen, Verzweigungen und Rechnen mit ganzen Zahlen.

bester = 0
laengste = 0
start = 1
while start < 1000000:
    n = start
    schritte = 0
    while n != 1:
        if n % 2 == 0:
            n = n // 2
        else:
            n = 3 * n + 1
        schritte += 1
    if schritte > laengste:
        laengste = schritte
        bester = start
    start += 1
print(bester, laengste)
