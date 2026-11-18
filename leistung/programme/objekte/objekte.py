# Hunderttausend Objekte aus drei Klassen, 200 Runden lang bewegt. Misst Klassen und überschriebene Methoden.


class Wesen:
    def __init__(self, x, y, tempo):
        self.x = x
        self.y = y
        self.tempo = tempo

    def beweg(self):
        self.x = (self.x + self.tempo) % 1000

    def wert(self):
        return self.x + self.y


class Hupfa(Wesen):
    def __init__(self, x, y, tempo):
        super().__init__(x, y, tempo)
        self.hoch = 0

    def beweg(self):
        super().beweg()
        self.hoch = (self.hoch + 7) % 50
        self.y = (self.y + self.hoch) % 1000


class Kreisla(Wesen):
    def beweg(self):
        oid = self.x
        self.x = (self.y + self.tempo) % 1000
        self.y = oid

    def wert(self):
        return self.x * 2 + self.y


alle = []
for i in range(100000):
    if i % 3 == 0:
        alle.append(Wesen(i * 37 % 1000, i * 91 % 1000, i % 7 + 1))
    elif i % 3 == 1:
        alle.append(Hupfa(i * 37 % 1000, i * 91 % 1000, i % 7 + 1))
    else:
        alle.append(Kreisla(i * 37 % 1000, i * 91 % 1000, i % 7 + 1))

summe = 0
for runde in range(1, 201):
    for w in alle:
        w.beweg()
        summe = (summe + w.wert()) % 1000000007
print(summe)
