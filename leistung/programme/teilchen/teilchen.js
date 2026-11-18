// Hunderttausend Teilchen in einer Liste, 300 Runden lang bewegt. Misst Listen von Strukturen.

const alle = [];
for (let i = 0; i < 100000; i++) {
    alle.push({ x: i * 37 % 1000, y: i * 91 % 1000, vx: i % 7 - 3, vy: i % 5 - 2 });
}

for (let runde = 1; runde <= 300; runde++) {
    for (const t of alle) {
        t.x += t.vx * 0.5;
        t.y += t.vy * 0.5;
        if (t.x < 0 || t.x > 1000) {
            t.vx = -t.vx;
        }
        if (t.y < 0 || t.y > 1000) {
            t.vy = -t.vy;
        }
    }
}

let summe = 0.0;
for (const t of alle) {
    summe += t.x + t.y;
}
console.log(Math.trunc(summe));
