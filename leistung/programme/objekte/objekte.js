// Hunderttausend Objekte aus drei Klassen, 200 Runden lang bewegt. Misst Klassen und überschriebene Methoden.

class Wesen {
    constructor(x, y, tempo) {
        this.x = x;
        this.y = y;
        this.tempo = tempo;
    }

    beweg() {
        this.x = (this.x + this.tempo) % 1000;
    }

    wert() {
        return this.x + this.y;
    }
}

class Hupfa extends Wesen {
    constructor(x, y, tempo) {
        super(x, y, tempo);
        this.hoch = 0;
    }

    beweg() {
        super.beweg();
        this.hoch = (this.hoch + 7) % 50;
        this.y = (this.y + this.hoch) % 1000;
    }
}

class Kreisla extends Wesen {
    beweg() {
        const oid = this.x;
        this.x = (this.y + this.tempo) % 1000;
        this.y = oid;
    }

    wert() {
        return this.x * 2 + this.y;
    }
}

const alle = [];
for (let i = 0; i < 100000; i++) {
    if (i % 3 === 0) {
        alle.push(new Wesen(i * 37 % 1000, i * 91 % 1000, i % 7 + 1));
    } else if (i % 3 === 1) {
        alle.push(new Hupfa(i * 37 % 1000, i * 91 % 1000, i % 7 + 1));
    } else {
        alle.push(new Kreisla(i * 37 % 1000, i * 91 % 1000, i % 7 + 1));
    }
}

let summe = 0;
for (let runde = 1; runde <= 200; runde++) {
    for (const w of alle) {
        w.beweg();
        summe = (summe + w.wert()) % 1000000007;
    }
}
console.log(summe);
