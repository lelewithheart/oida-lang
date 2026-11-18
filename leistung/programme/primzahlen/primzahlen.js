// Primzahlen unter 2 Millionen zählen, durch Probeteilen. Misst Schleifen und Teilen mit Rest.

function istPrim(n) {
    for (let t = 2; t * t <= n; t++) {
        if (n % t === 0) {
            return false;
        }
    }
    return true;
}

let anzahl = 0;
for (let n = 2; n < 2000000; n++) {
    if (istPrim(n)) {
        anzahl++;
    }
}
console.log(anzahl);
