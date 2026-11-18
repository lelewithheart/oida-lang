# Leistungsvergleich

Erzeugt mit `cargo run --release -p oida-messen` am 2026-11-18. Nicht von Hand ändern.

![Laufzeit je Programm und Sprache](ergebnis.svg)

Rechner: AMD Ryzen 5 8645HS w/ Radeon 760M Graphics, 14 GB Arbeitsspeicher, Microsoft Windows 11 Pro

| Sprache | Art | Version |
|---|---|---|
| OIDA | übersetzt (LLVM) | oida 0.3.0 |
| C | übersetzt (LLVM, clang -O2) | clang version 23.1.5 |
| Rust | übersetzt (LLVM, opt-level 3) | rustc 1.100.0 (0ef1bc386 2026-11-09) |
| C# | Bytecode + JIT (.NET) | .NET SDK 10.0.403 |
| Java | Bytecode + JIT | openjdk version "25.0.4.1" 2026-08-18 |
| JavaScript | JIT (Node) | v26.13.0 |
| Python | Interpreter | Python 3.13.14 |

## Überblick: Zeit

Kleiner ist besser. Der mittlere Wert (Median) von mehreren Runden, samt Starten des Programms.

| Programm | OIDA | C | Rust | C# | Java | JavaScript | Python |
|---|---:|---:|---:|---:|---:|---:|---:|
| hallo | 27 ms | 23 ms | 21 ms | 121 ms | 114 ms | 87 ms | 171 ms |
| fib | 437 ms | 496 ms | 410 ms | 666 ms | 680 ms | 1,71 s | 19,79 s |
| primzahlen | 510 ms | 496 ms | 497 ms | 665 ms | 671 ms | 606 ms | 19,92 s |
| collatz | 207 ms | 200 ms | 198 ms | 389 ms | 451 ms | 2,95 s | 31,70 s |
| mandelbrot | 243 ms | 372 ms | 249 ms | 335 ms | 497 ms | 323 ms | 28,84 s |
| teilchen | 108 ms | 80 ms | 91 ms | 244 ms | 323 ms | 300 ms | 13,66 s |
| texte | 174 ms | 112 ms | 217 ms | 168 ms | 193 ms | 152 ms | 647 ms |
| ausgabe | 124 ms | 538 ms | 196 ms | 270 ms | 483 ms | 376 ms | 4,99 s |

## Überblick: Zeit im Verhältnis zu OIDA

1,00× heißt gleich schnell wie OIDA, 2× heißt doppelt so lang, 0,5× halb so lang. Die letzte Zeile ist das geometrische Mittel über alle Programme.

| Programm | OIDA | C | Rust | C# | Java | JavaScript | Python |
|---|---:|---:|---:|---:|---:|---:|---:|
| hallo | 1,00× | 0,85× | 0,77× | 4,52× | 4,27× | 3,25× | 6,39× |
| fib | 1,00× | 1,13× | 0,94× | 1,52× | 1,55× | 3,91× | 45× |
| primzahlen | 1,00× | 0,97× | 0,97× | 1,30× | 1,32× | 1,19× | 39× |
| collatz | 1,00× | 0,96× | 0,96× | 1,88× | 2,18× | 14× | 153× |
| mandelbrot | 1,00× | 1,53× | 1,02× | 1,38× | 2,04× | 1,33× | 119× |
| teilchen | 1,00× | 0,74× | 0,84× | 2,26× | 2,99× | 2,79× | 127× |
| texte | 1,00× | 0,64× | 1,25× | 0,97× | 1,11× | 0,87× | 3,72× |
| ausgabe | 1,00× | 4,35× | 1,59× | 2,19× | 3,91× | 3,04× | 40× |
| **Mittel** | **1,00×** | **1,14×** | **1,02×** | **1,80×** | **2,18×** | **2,61×** | **38×** |

## Überblick: Speicher

Der höchste Arbeitsspeicher, den das Programm gebraucht hat. Kleiner ist besser.

![Arbeitsspeicher je Sprache](speicher.svg)

| Programm | OIDA | C | Rust | C# | Java | JavaScript | Python |
|---|---:|---:|---:|---:|---:|---:|---:|
| hallo | 4,9 MB | 4,8 MB | 4,9 MB | 23,5 MB | 56,7 MB | 34,5 MB | 23,5 MB |
| fib | 4,9 MB | 4,8 MB | 4,9 MB | 23,6 MB | 57,1 MB | 36,5 MB | 23,4 MB |
| primzahlen | 4,9 MB | 4,8 MB | 4,9 MB | 23,5 MB | 57,5 MB | 37,1 MB | 23,5 MB |
| collatz | 4,9 MB | 4,8 MB | 4,9 MB | 24,2 MB | 57,9 MB | 37,4 MB | 23,4 MB |
| mandelbrot | 4,9 MB | 4,8 MB | 4,9 MB | 23,6 MB | 57,9 MB | 38,4 MB | 23,4 MB |
| teilchen | 9,2 MB | 7,4 MB | 9,4 MB | 30,2 MB | 64,2 MB | 73,3 MB | 47,1 MB |
| texte | 5,0 MB | 4,8 MB | 5,0 MB | 33,5 MB | 97,3 MB | 45,0 MB | 23,5 MB |
| ausgabe | 5,0 MB | 4,8 MB | 4,9 MB | 34,4 MB | 170,3 MB | 80,3 MB | 25,0 MB |

## Die Programme im Einzelnen

### hallo

Nur eine Zeile ausgeben. Misst, wie lange ein Programm zum Starten braucht.

| Sprache | Zeit | zu OIDA | Runden | Speicher | Bauen | Größe |
|---|---:|---:|---:|---:|---:|---:|
| OIDA | 27 ms | 1,00× | 7 | 4,9 MB | 456 ms | 294 KB |
| C | 23 ms | 0,85× | 7 | 4,8 MB | 374 ms | 60 KB |
| Rust | 21 ms | 0,77× | 7 | 4,9 MB | 508 ms | 412 KB |
| C# | 121 ms | 4,52× | 7 | 23,5 MB | 1,27 s | 4 KB |
| Java | 114 ms | 4,27× | 7 | 56,7 MB | 971 ms | 0 KB |
| JavaScript | 87 ms | 3,25× | 7 | 34,5 MB | – | – |
| Python | 171 ms | 6,39× | 7 | 23,5 MB | – | – |

### fib

Fibonacci-Zahl 40, rekursiv. Misst Funktionsaufrufe (rund 330 Millionen).

| Sprache | Zeit | zu OIDA | Runden | Speicher | Bauen | Größe |
|---|---:|---:|---:|---:|---:|---:|
| OIDA | 437 ms | 1,00× | 7 | 4,9 MB | 399 ms | 294 KB |
| C | 496 ms | 1,13× | 7 | 4,8 MB | 392 ms | 61 KB |
| Rust | 410 ms | 0,94× | 7 | 4,9 MB | 560 ms | 414 KB |
| C# | 666 ms | 1,52× | 7 | 23,6 MB | 994 ms | 4 KB |
| Java | 680 ms | 1,55× | 7 | 57,1 MB | 879 ms | 1 KB |
| JavaScript | 1,71 s | 3,91× | 5 | 36,5 MB | – | – |
| Python | 19,79 s | 45× | 3 | 23,4 MB | – | – |

### primzahlen

Primzahlen unter 2 Millionen zählen, durch Probeteilen. Misst Schleifen und Teilen mit Rest.

| Sprache | Zeit | zu OIDA | Runden | Speicher | Bauen | Größe |
|---|---:|---:|---:|---:|---:|---:|
| OIDA | 510 ms | 1,00× | 7 | 4,9 MB | 395 ms | 294 KB |
| C | 496 ms | 0,97× | 7 | 4,8 MB | 354 ms | 61 KB |
| Rust | 497 ms | 0,97× | 7 | 4,9 MB | 594 ms | 414 KB |
| C# | 665 ms | 1,30× | 7 | 23,5 MB | 1,12 s | 4 KB |
| Java | 671 ms | 1,32× | 7 | 57,5 MB | 878 ms | 1 KB |
| JavaScript | 606 ms | 1,19× | 7 | 37,1 MB | – | – |
| Python | 19,92 s | 39× | 3 | 23,5 MB | – | – |

### collatz

Längste Collatz-Folge für Startwerte unter 1 Million. Misst Schleifen, Verzweigungen und Rechnen mit ganzen Zahlen.

| Sprache | Zeit | zu OIDA | Runden | Speicher | Bauen | Größe |
|---|---:|---:|---:|---:|---:|---:|
| OIDA | 207 ms | 1,00× | 7 | 4,9 MB | 400 ms | 294 KB |
| C | 200 ms | 0,96× | 7 | 4,8 MB | 384 ms | 61 KB |
| Rust | 198 ms | 0,96× | 7 | 4,9 MB | 578 ms | 414 KB |
| C# | 389 ms | 1,88× | 7 | 24,2 MB | 999 ms | 4 KB |
| Java | 451 ms | 2,18× | 7 | 57,9 MB | 822 ms | 1 KB |
| JavaScript | 2,95 s | 14× | 5 | 37,4 MB | – | – |
| Python | 31,70 s | 153× | 1 | 23,4 MB | – | – |

### mandelbrot

Mandelbrot-Menge auf 1000 x 1000 Punkten. Höchstens 300 Schritte je Punkt; misst Rechnen mit Kommazahlen.

| Sprache | Zeit | zu OIDA | Runden | Speicher | Bauen | Größe |
|---|---:|---:|---:|---:|---:|---:|
| OIDA | 243 ms | 1,00× | 7 | 4,9 MB | 379 ms | 294 KB |
| C | 372 ms | 1,53× | 7 | 4,8 MB | 348 ms | 62 KB |
| Rust | 249 ms | 1,02× | 7 | 4,9 MB | 547 ms | 414 KB |
| C# | 335 ms | 1,38× | 7 | 23,6 MB | 944 ms | 4 KB |
| Java | 497 ms | 2,04× | 7 | 57,9 MB | 812 ms | 1 KB |
| JavaScript | 323 ms | 1,33× | 7 | 38,4 MB | – | – |
| Python | 28,84 s | 119× | 1 | 23,4 MB | – | – |

### teilchen

Hunderttausend Teilchen in einer Liste, 300 Runden lang bewegt. Misst Listen von Strukturen.

| Sprache | Zeit | zu OIDA | Runden | Speicher | Bauen | Größe |
|---|---:|---:|---:|---:|---:|---:|
| OIDA | 108 ms | 1,00× | 7 | 9,2 MB | 452 ms | 300 KB |
| C | 80 ms | 0,74× | 7 | 7,4 MB | 437 ms | 62 KB |
| Rust | 91 ms | 0,84× | 7 | 9,4 MB | 743 ms | 414 KB |
| C# | 244 ms | 2,26× | 7 | 30,2 MB | 1,25 s | 4 KB |
| Java | 323 ms | 2,99× | 7 | 64,2 MB | 991 ms | 1 KB |
| JavaScript | 300 ms | 2,79× | 7 | 73,3 MB | – | – |
| Python | 13,66 s | 127× | 3 | 47,1 MB | – | – |

### texte

Eine Million kurze Texte zusammenbauen und vergleichen. Misst Texte und den Speicher dafür.

| Sprache | Zeit | zu OIDA | Runden | Speicher | Bauen | Größe |
|---|---:|---:|---:|---:|---:|---:|
| OIDA | 174 ms | 1,00× | 7 | 5,0 MB | 384 ms | 298 KB |
| C | 112 ms | 0,64× | 7 | 4,8 MB | 451 ms | 63 KB |
| Rust | 217 ms | 1,25× | 7 | 5,0 MB | 574 ms | 416 KB |
| C# | 168 ms | 0,97× | 7 | 33,5 MB | 1,08 s | 4 KB |
| Java | 193 ms | 1,11× | 7 | 97,3 MB | 881 ms | 1 KB |
| JavaScript | 152 ms | 0,87× | 7 | 45,0 MB | – | – |
| Python | 647 ms | 3,72× | 7 | 23,5 MB | – | – |

### ausgabe

Zwei Millionen Zeilen ausgeben. Misst die Ausgabe.

| Sprache | Zeit | zu OIDA | Runden | Speicher | Bauen | Größe |
|---|---:|---:|---:|---:|---:|---:|
| OIDA | 124 ms | 1,00× | 7 | 5,0 MB | 416 ms | 294 KB |
| C | 538 ms | 4,35× | 7 | 4,8 MB | 368 ms | 61 KB |
| Rust | 196 ms | 1,59× | 7 | 4,9 MB | 808 ms | 420 KB |
| C# | 270 ms | 2,19× | 7 | 34,4 MB | 1,06 s | 4 KB |
| Java | 483 ms | 3,91× | 7 | 170,3 MB | 904 ms | 1 KB |
| JavaScript | 376 ms | 3,04× | 7 | 80,3 MB | – | – |
| Python | 4,99 s | 40× | 3 | 25,0 MB | – | – |

