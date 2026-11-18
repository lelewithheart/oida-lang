// Längste Collatz-Folge für Startwerte unter 1 Million. Misst Schleifen, Verzweigungen und Rechnen mit ganzen Zahlen.

let bester = 0;
let laengste = 0;
for (let start = 1; start < 1000000; start++) {
    let n = start;
    let schritte = 0;
    while (n !== 1) {
        if (n % 2 === 0) {
            n = n / 2;
        } else {
            n = 3 * n + 1;
        }
        schritte++;
    }
    if (schritte > laengste) {
        laengste = schritte;
        bester = start;
    }
}
console.log(bester + " " + laengste);
