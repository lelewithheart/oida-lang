# Hunderttausend Teilchen in einer Liste, 300 Runden lang bewegt. Misst Listen von Strukturen.


class Teilchen:
    def __init__(self, x, y, vx, vy):
        self.x = x
        self.y = y
        self.vx = vx
        self.vy = vy


alle = []
for i in range(100000):
    alle.append(Teilchen(float(i * 37 % 1000), float(i * 91 % 1000), float(i % 7 - 3), float(i % 5 - 2)))

for runde in range(1, 301):
    for t in alle:
        t.x += t.vx * 0.5
        t.y += t.vy * 0.5
        if t.x < 0 or t.x > 1000:
            t.vx = -t.vx
        if t.y < 0 or t.y > 1000:
            t.vy = -t.vy

summe = 0.0
for t in alle:
    summe += t.x + t.y
print(int(summe))
