# Leistungsvergleich

Wie schnell ist OIDA im Vergleich zu anderen Sprachen? Hier liegen dieselben sieben Programme
in OIDA, C, Rust, Java, JavaScript und Python, dazu ein Werkzeug, das sie baut, laufen lässt und misst.

Die Zahlen stehen in [ERGEBNIS.md](ERGEBNIS.md).

## Messen

Im Projektordner:

```
cargo build --release
cargo run --release -p oida-messen
```

Das dauert ein paar Minuten (Python braucht am längsten) und schreibt `ERGEBNIS.md` neu.
Währenddessen den Rechner in Ruhe lassen und das Netzteil anstecken, sonst schwanken die Zeiten.

```
cargo run --release -p oida-messen -- fib collatz     nur diese Programme
cargo run --release -p oida-messen -- --ohne python   eine Sprache auslassen
```

Sprachen, die am Rechner fehlen, werden ausgelassen. Gebraucht werden: `clang` (llvm-mingw),
`rustc`, `javac`/`java` (im PATH, über `JAVA_HOME` oder das Java von DataGrip), `node`, `python`.

## Die Programme

| Programm     | Was es tut                                              | Was es misst                         |
|--------------|---------------------------------------------------------|--------------------------------------|
| `hallo`      | eine Zeile ausgeben                                     | wie lange das Starten dauert         |
| `fib`        | Fibonacci-Zahl 40, rekursiv                             | Funktionsaufrufe                     |
| `primzahlen` | Primzahlen unter 2 Millionen zählen (Probeteilen)       | Schleifen, Teilen mit Rest           |
| `collatz`    | längste Collatz-Folge unter 1 Million                   | Schleifen, Verzweigungen, ganze Zahlen |
| `mandelbrot` | Mandelbrot-Menge auf 1000 x 1000 Punkten                | Rechnen mit Kommazahlen              |
| `texte`      | eine Million kurze Texte bauen und vergleichen          | Texte und ihr Speicher               |
| `ausgabe`    | zwei Millionen Zeilen ausgeben                          | die Ausgabe                          |

Sie liegen in `programme/<name>/`, eines je Sprache.

## Wie gemessen wird

- **Zeit:** vom Starten bis zum Ende des Programms, also samt Hochfahren von Java, Node oder Python.
  Jedes Programm läuft mehrmals (schnelle 7-mal, sehr langsame nur einmal), es zählt die schnellste Runde.
- **Speicher:** der höchste Arbeitsspeicher, den der Prozess gebraucht hat (fragt Windows).
- **Bauen und Größe:** wie lange das Übersetzen dauert und wie groß die `.exe` wird.
- **Richtigkeit:** Jede Sprache muss dieselbe Ausgabe liefern (die `.erwartet`-Datei, bei `ausgabe`
  das, was OIDA ausgibt). Sonst zählt die Messung nicht.

Damit es fair bleibt:

- Überall derselbe Rechenweg, so geschrieben, wie man es in der Sprache üblicherweise schreibt.
  Keine Tricks, keine Sonderbibliotheken.
- Ganze Zahlen haben überall 64 Bit (JavaScript nimmt seine normalen Zahlen, Python seine beliebig großen).
- C wird mit `clang -O2` gebaut, Rust mit `opt-level=3` (wie `cargo build --release`), OIDA wie immer.
- Bei `ausgabe` verwenden Rust, Java und JavaScript einen Puffer, weil man das dort bei viel Ausgabe so macht.

## Was die Zahlen sagen

Stand der Messung vom 2026-11-18 (Ryzen 5 8645HS, Windows 11). Die genauen Werte ändern sich
von Lauf zu Lauf um ein paar Prozent.

**Beim Rechnen ist OIDA so schnell wie C und Rust.** Bei `fib`, `primzahlen`, `collatz` und
`mandelbrot` liegen die drei innerhalb weniger Prozent. Das ist keine Überraschung: Alle drei
lassen LLVM den Maschinencode erzeugen, und die Typen stehen in OIDA beim Übersetzen fest.
Java braucht 1,4- bis 2,2-mal so lang, JavaScript 1,2- bis 15-mal, Python 38- bis 336-mal.

**Starten und Speicher:** Ein OIDA-Programm startet in 20 ms und braucht 5 MB, genau wie C und
Rust. Java braucht 127 ms und 57 MB, Node 88 ms und 35 MB, Python 192 ms und 24 MB, bevor
überhaupt etwas gerechnet ist.

**Bei Texten ist OIDA schwach.** `texte` läuft in C viermal so schnell, in Rust, Java und
JavaScript doppelt bis dreimal so schnell, und OIDA braucht dabei 111 MB statt 5 MB. Der Grund:
Jeder neue Text holt sich Speicher, und v0.1 gibt ihn nie zurück. Das ist die größte Baustelle
und steht als Referenzzählung im Fahrplan.

**Die Ausgabe ist mittel.** `ausgabe` läuft so schnell wie in JavaScript und schneller als in C
und Java, aber Rust ist doppelt so schnell. OIDA sperrt die Ausgabe bei jedem Wert einzeln und
macht aus jeder Zahl erst einen Text; das lässt sich verbessern.

**Im Mittel über alle sieben Programme** (geometrisches Mittel, OIDA = 1): C 0,95, Rust 0,83,
Java 1,66, JavaScript 2,05, Python 29. Dass Rust vorne liegt, kommt von `texte` und `ausgabe`.

### Zwei Stellen, die man nicht falsch lesen soll

- **OIDA ist nicht schneller als C.** Bei `mandelbrot` braucht C hier 1,5-mal so lang, bei `fib`
  1,16-mal. Das liegt an Voreinstellungen von clang, nicht an der Sprache: Mit
  `-ffp-contract=off` ist das C-Programm bei `mandelbrot` genauso schnell wie OIDA und Rust
  (nachgemessen). Unterschiede bis etwa 15 % zwischen OIDA, C und Rust sind solche Zufälle.
- **Die Sprachen sind nicht gleich weit.** Die Programme hier kommen mit Zahlen, Texten,
  Schleifen und Funktionen aus, weil OIDA v0.1 nicht mehr kann.

## Was der Vergleich nicht zeigt

Geschwindigkeit ist nur ein Teil davon, wie gut eine Sprache ist. Nicht gemessen ist alles,
was OIDA noch fehlt: Listen, Strukturen, Speicher freigeben, Dateien, Eingabe, mehrere Kerne,
eine Standardbibliothek. Die üblichen Vergleichsprogramme (Sortieren, Matrizen, Bäume) lassen
sich in OIDA v0.1 gar nicht schreiben. Ehrlich zusammengefasst: Was OIDA kann, kann es so
schnell wie C. Es kann nur noch wenig.

Spannend wird der Vergleich wieder, wenn Listen und die Referenzzählung da sind: Dann zeigt
sich, was das Zählen kostet.
