// Zwei Millionen Zeilen ausgeben. Misst die Ausgabe.

// console.log je Zeile ist langsam; für viel Ausgabe sammelt man in Node die Zeilen.
let zeilen = [];
for (let i = 1; i <= 2000000; i++) {
    zeilen.push(i + " " + (i * 3));
    if (zeilen.length === 10000) {
        process.stdout.write(zeilen.join("\n") + "\n");
        zeilen = [];
    }
}
if (zeilen.length > 0) {
    process.stdout.write(zeilen.join("\n") + "\n");
}
