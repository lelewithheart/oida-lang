// Eine Million kurze Texte zusammenbauen und vergleichen. Misst Texte und den Speicher dafür.

let kleiner = 0;
let gleich = 0;
for (let i = 1; i <= 1000000; i++) {
    const s = "nr" + i + "!";
    if (s < "nr5") {
        kleiner++;
    }
    if (s === "nr777!") {
        gleich++;
    }
}
console.log(kleiner + " " + gleich);
