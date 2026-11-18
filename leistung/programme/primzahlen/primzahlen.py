# Primzahlen unter 2 Millionen zählen, durch Probeteilen. Misst Schleifen und Teilen mit Rest.

def ist_prim(n):
    t = 2
    while t * t <= n:
        if n % t == 0:
            return False
        t += 1
    return True


anzahl = 0
n = 2
while n < 2000000:
    if ist_prim(n):
        anzahl += 1
    n += 1
print(anzahl)
