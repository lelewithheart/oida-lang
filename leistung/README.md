# Leistungsvergleich

Wie schnell ist OIDA im Vergleich zu anderen Sprachen? Hier liegen dieselben acht Programme
in OIDA, C, Rust, C#, Java, JavaScript und Python, dazu ein Werkzeug, das sie baut, laufen lässt und misst.

![Laufzeit je Programm und Sprache](ergebnis.svg)

![Arbeitsspeicher je Sprache](speicher.svg)

Alle Zahlen als Tabellen stehen in [ERGEBNIS.md](ERGEBNIS.md), die rohen Messwerte in [ergebnis.csv](ergebnis.csv).

## Messen

Im Projektordner:

```
cargo build --release
cargo run --release -p oida-messen
```

Das dauert rund acht Minuten (Python braucht am längsten) und schreibt `ERGEBNIS.md`,
`ergebnis.csv` und die beiden Grafiken neu. Währenddessen den Rechner in Ruhe lassen und das
Netzteil anstecken, sonst schwanken die Zeiten.

```
cargo run --release -p oida-messen -- fib collatz     nur diese Programme
cargo run --release -p oida-messen -- --ohne python   eine Sprache auslassen
cargo run --release -p oida-messen -- --nur-grafik    nicht messen, nur die Grafiken neu zeichnen
```

Sprachen, die am Rechner fehlen, werden ausgelassen. Gebraucht werden:

| Sprache    | Was da sein muss                                                            |
|------------|-----------------------------------------------------------------------------|
| C          | `clang` (aus llvm-mingw)                                                    |
| Rust       | `rustc`                                                                     |
| C#         | das .NET-SDK (`dotnet`); der Compiler wird direkt aufgerufen, ohne Projekt und ohne Netz |
| Java       | `javac` und `java` (im PATH, über `JAVA_HOME` oder das Java von DataGrip)   |
| JavaScript | `node`                                                                      |
| Python     | `python`                                                                    |

## Die Programme

| Programm     | Was es tut                                              | Was es misst                         |
|--------------|---------------------------------------------------------|--------------------------------------|
| `hallo`      | eine Zeile ausgeben                                     | wie lange das Starten dauert         |
| `fib`        | Fibonacci-Zahl 40, rekursiv                             | Funktionsaufrufe                     |
| `primzahlen` | Primzahlen unter 2 Millionen zählen (Probeteilen)       | Schleifen, Teilen mit Rest           |
| `collatz`    | längste Collatz-Folge unter 1 Million                   | Schleifen, Verzweigungen, ganze Zahlen |
| `mandelbrot` | Mandelbrot-Menge auf 1000 x 1000 Punkten                | Rechnen mit Kommazahlen              |
| `teilchen`   | 100 000 Teilchen in einer Liste, 300 Runden bewegt      | Listen von Strukturen                |
| `texte`      | eine Million kurze Texte bauen und vergleichen          | Texte und ihr Speicher               |
| `ausgabe`    | zwei Millionen Zeilen ausgeben                          | die Ausgabe                          |

Sie liegen in `programme/<name>/`, eines je Sprache.

## Wie gemessen wird

- **Zeit:** vom Starten bis zum Ende des Programms, also samt Hochfahren von .NET, Java, Node oder Python.
  Jedes Programm läuft mehrmals (schnelle 7-mal, sehr langsame nur einmal), es zählt der mittlere
  Wert (Median). So verfälscht ein einzelner Ausreißer nach oben oder unten nichts.
- **Speicher:** der höchste Arbeitsspeicher, den der Prozess gebraucht hat (fragt Windows).
- **Bauen und Größe:** wie lange das Übersetzen dauert und wie groß das Ergebnis wird.
- **Richtigkeit:** Jede Sprache muss dieselbe Ausgabe liefern (die `.erwartet`-Datei, bei `ausgabe`
  das, was OIDA ausgibt). Sonst zählt die Messung nicht.

Damit es fair bleibt:

- Überall derselbe Rechenweg, so geschrieben, wie man es in der Sprache üblicherweise schreibt.
  Keine Tricks, keine Sonderbibliotheken.
- Ganze Zahlen haben überall 64 Bit (JavaScript nimmt seine normalen Zahlen, Python seine beliebig großen).
- C wird mit `clang -O2` gebaut, Rust mit `opt-level=3` (wie `cargo build --release`), C# mit
  `-optimize+`, OIDA wie immer.
- Bei `ausgabe` verwenden Rust, C#, Java und JavaScript einen Puffer, weil man das dort bei viel Ausgabe so macht.
- Bei `teilchen` ist ein Teilchen in C, Rust und OIDA ein Wert in der Liste, in C#, Java,
  JavaScript und Python ein eigenes Objekt (eine Klasse), weil man es dort üblicherweise so schreibt.

## Was die Zahlen sagen

Stand der Messung vom 18.11.2026 (Ryzen 5 8645HS, Windows 11). Die genauen Werte ändern sich
von Lauf zu Lauf um ein paar Prozent.

**Beim Rechnen ist OIDA so schnell wie C und Rust.** Bei `fib`, `primzahlen`, `collatz` und
`mandelbrot` liegen OIDA und Rust innerhalb von 6 %. Das ist keine Überraschung: Alle drei
lassen LLVM den Maschinencode erzeugen, und die Typen stehen in OIDA beim Übersetzen fest.
C# braucht 1,3- bis 1,9-mal so lang, Java 1,3- bis 2,2-mal, JavaScript 1,2- bis 14-mal,
Python 39- bis 153-mal.

**Listen von Strukturen:** `teilchen` läuft in 108 ms. C (80 ms) und Rust (91 ms) sind etwas
schneller, weil OIDA bei jedem Zugriff prüft, ob es die Stelle in der Liste gibt. C#, Java und
JavaScript brauchen 2,3- bis 3-mal so lang wie OIDA. Der Grund steht in der Speicher-Spalte:
In OIDA liegen die Teilchen als Werte dicht hintereinander in der Liste (9 MB), dort ist jedes
ein eigenes Objekt (30 bis 73 MB).

**Starten und Speicher:** Ein OIDA-Programm startet in rund 25 ms und braucht 5 MB, wie C und
Rust. C# braucht 121 ms und 24 MB, Java 114 ms und 57 MB, Node 87 ms und 35 MB, Python 171 ms
und 24 MB, bevor überhaupt etwas gerechnet ist.

**Texte:** 174 ms und 5 MB. Das ist schneller als Rust und Java, etwa gleichauf mit C# und
JavaScript, und langsamer als C (112 ms), das hier ohne Heap auskommt.

**Ausgabe:** 124 ms, am schnellsten von allen. `SCHREIB` schreibt Zahlen direkt in einen Puffer.

**Im Mittel über alle acht Programme** (geometrisches Mittel, OIDA = 1): Rust 1,02, C 1,14,
C# 1,80, Java 2,18, JavaScript 2,61, Python 38.

### Was die Optimierung gebracht hat

Die erste Messung hatte zwei Schwächen gezeigt, die danach behoben wurden:

| Programm  | vorher                  | nachher           | Was geändert wurde                                   |
|-----------|-------------------------|-------------------|------------------------------------------------------|
| `texte`   | 230 bis 390 ms, 111 MB  | 174 ms, 5 MB      | Texte werden freigegeben, sobald sie niemand mehr braucht (Referenzzählung); `a + b + c` entsteht in einem Schritt |
| `ausgabe` | 365 ms                  | 124 ms            | Ausgabe ohne Sperre, Zahlen direkt in den Puffer     |
| `teilchen`| 132 ms                  | 108 ms            | LLVM erfährt, dass der Kopf einer Liste und ihre Elemente getrennter Speicher sind, und liest Länge und Datenzeiger nicht bei jedem Zugriff neu |

### Was man nicht falsch lesen soll

- **OIDA ist nicht schneller als C.** Dass C im Mittel hinter OIDA liegt, kommt von zwei Stellen,
  die nichts mit der Sprache zu tun haben: Bei `ausgabe` ist `printf` langsam (mit einem eigenen
  Puffer wäre C mindestens so schnell wie OIDA), und bei `mandelbrot` bremst eine Voreinstellung
  von clang (mit `-ffp-contract=off` ist das C-Programm genauso schnell, nachgemessen).
- **OIDA wurde auf genau diese Programme hin verbessert.** Die Optimierungen kamen, nachdem
  dieser Vergleich die Schwächen gezeigt hatte. Die Programme der anderen Sprachen sind normal
  geschrieben und nicht nachgeschärft. Der Vergleich ist also für OIDA eher freundlich.
- **Kleine Unterschiede sind Zufall.** Bis etwa 15 % zwischen OIDA, C und Rust hängen von
  Kleinigkeiten im erzeugten Code ab. Bei `fib` lief das OIDA-Programm in einem Messlauf sogar
  einmal in 260 ms statt 410 ms, ohne dass sich etwas geändert hatte.
- **Die Sprachen sind nicht gleich weit.** Die Programme hier kommen mit Zahlen, Texten,
  Strukturen, Listen, Schleifen und Funktionen aus, weil OIDA noch nicht mehr kann.

## Was der Vergleich nicht zeigt

Geschwindigkeit ist nur ein Teil davon, wie gut eine Sprache ist. Nicht gemessen ist alles,
was OIDA noch fehlt: Klassen, Dateien, mehrere Kerne, eine Standardbibliothek. Ehrlich
zusammengefasst: Was OIDA kann, kann es etwa so schnell wie C und Rust. Es kann nur noch nicht alles.

Offen ist auch, was die Referenzzählung bei großen Mengen von Texten und Listen in Listen
kostet; die Programme hier belasten sie kaum.
