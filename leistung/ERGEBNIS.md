# Leistungsvergleich

Erzeugt mit `cargo run --release -p oida-messen` am 2026-11-18. Nicht von Hand ändern.

![Laufzeit je Programm und Sprache](ergebnis.svg)

Rechner: AMD Ryzen 5 8645HS w/ Radeon 760M Graphics, 14 GB Arbeitsspeicher, Microsoft Windows 11 Pro

| Sprache | Art | Version |
|---|---|---|
| OIDA | übersetzt (LLVM) | oida 0.1.0 |
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
| hallo | 23 ms | 26 ms | 26 ms | 129 ms | 147 ms | 100 ms | 189 ms |
| fib | 415 ms | 477 ms | 386 ms | 671 ms | 585 ms | 1,66 s | 19,61 s |
| primzahlen | 502 ms | 490 ms | 490 ms | 660 ms | 678 ms | 586 ms | 19,02 s |
| collatz | 199 ms | 212 ms | 205 ms | 398 ms | 455 ms | 2,91 s | 29,84 s |
| mandelbrot | 251 ms | 372 ms | 248 ms | 345 ms | 511 ms | 326 ms | 28,85 s |
| texte | 172 ms | 115 ms | 224 ms | 144 ms | 193 ms | 143 ms | 611 ms |
| ausgabe | 119 ms | 531 ms | 184 ms | 257 ms | 488 ms | 369 ms | 5,28 s |

## Überblick: Zeit im Verhältnis zu OIDA

1,00× heißt gleich schnell wie OIDA, 2× heißt doppelt so lang, 0,5× halb so lang. Die letzte Zeile ist das geometrische Mittel über alle Programme.

| Programm | OIDA | C | Rust | C# | Java | JavaScript | Python |
|---|---:|---:|---:|---:|---:|---:|---:|
| hallo | 1,00× | 1,13× | 1,12× | 5,62× | 6,39× | 4,37× | 8,26× |
| fib | 1,00× | 1,15× | 0,93× | 1,62× | 1,41× | 4,01× | 47× |
| primzahlen | 1,00× | 0,98× | 0,98× | 1,31× | 1,35× | 1,17× | 38× |
| collatz | 1,00× | 1,07× | 1,03× | 2,01× | 2,29× | 15× | 150× |
| mandelbrot | 1,00× | 1,48× | 0,99× | 1,37× | 2,03× | 1,30× | 115× |
| texte | 1,00× | 0,67× | 1,30× | 0,84× | 1,12× | 0,83× | 3,56× |
| ausgabe | 1,00× | 4,46× | 1,54× | 2,15× | 4,09× | 3,10× | 44× |
| **Mittel** | **1,00×** | **1,29×** | **1,11×** | **1,79×** | **2,21×** | **2,68×** | **33×** |

## Überblick: Speicher

Der höchste Arbeitsspeicher, den das Programm gebraucht hat. Kleiner ist besser.

![Arbeitsspeicher je Sprache](speicher.svg)

| Programm | OIDA | C | Rust | C# | Java | JavaScript | Python |
|---|---:|---:|---:|---:|---:|---:|---:|
| hallo | 4,9 MB | 4,8 MB | 4,9 MB | 23,5 MB | 56,7 MB | 34,6 MB | 23,6 MB |
| fib | 4,9 MB | 4,8 MB | 4,9 MB | 23,6 MB | 57,1 MB | 36,5 MB | 23,4 MB |
| primzahlen | 4,9 MB | 4,8 MB | 4,9 MB | 23,6 MB | 57,4 MB | 37,3 MB | 23,4 MB |
| collatz | 4,9 MB | 4,8 MB | 4,9 MB | 24,2 MB | 57,9 MB | 37,3 MB | 23,4 MB |
| mandelbrot | 4,9 MB | 4,8 MB | 4,9 MB | 23,7 MB | 57,9 MB | 38,6 MB | 23,4 MB |
| texte | 4,9 MB | 4,8 MB | 5,0 MB | 33,7 MB | 96,9 MB | 45,3 MB | 23,5 MB |
| ausgabe | 5,0 MB | 4,8 MB | 4,9 MB | 34,6 MB | 169,7 MB | 80,4 MB | 25,0 MB |

## Die Programme im Einzelnen

### hallo

Nur eine Zeile ausgeben. Misst, wie lange ein Programm zum Starten braucht.

| Sprache | Zeit | zu OIDA | Runden | Speicher | Bauen | Größe |
|---|---:|---:|---:|---:|---:|---:|
| OIDA | 23 ms | 1,00× | 7 | 4,9 MB | 389 ms | 293 KB |
| C | 26 ms | 1,13× | 7 | 4,8 MB | 369 ms | 60 KB |
| Rust | 26 ms | 1,12× | 7 | 4,9 MB | 535 ms | 412 KB |
| C# | 129 ms | 5,62× | 7 | 23,5 MB | 1,04 s | 4 KB |
| Java | 147 ms | 6,39× | 7 | 56,7 MB | 1,02 s | 0 KB |
| JavaScript | 100 ms | 4,37× | 7 | 34,6 MB | – | – |
| Python | 189 ms | 8,26× | 7 | 23,6 MB | – | – |

### fib

Fibonacci-Zahl 40, rekursiv. Misst Funktionsaufrufe (rund 330 Millionen).

| Sprache | Zeit | zu OIDA | Runden | Speicher | Bauen | Größe |
|---|---:|---:|---:|---:|---:|---:|
| OIDA | 415 ms | 1,00× | 7 | 4,9 MB | 408 ms | 292 KB |
| C | 477 ms | 1,15× | 7 | 4,8 MB | 364 ms | 61 KB |
| Rust | 386 ms | 0,93× | 7 | 4,9 MB | 524 ms | 414 KB |
| C# | 671 ms | 1,62× | 7 | 23,6 MB | 949 ms | 4 KB |
| Java | 585 ms | 1,41× | 7 | 57,1 MB | 839 ms | 1 KB |
| JavaScript | 1,66 s | 4,01× | 5 | 36,5 MB | – | – |
| Python | 19,61 s | 47× | 3 | 23,4 MB | – | – |

### primzahlen

Primzahlen unter 2 Millionen zählen, durch Probeteilen. Misst Schleifen und Teilen mit Rest.

| Sprache | Zeit | zu OIDA | Runden | Speicher | Bauen | Größe |
|---|---:|---:|---:|---:|---:|---:|
| OIDA | 502 ms | 1,00× | 7 | 4,9 MB | 432 ms | 292 KB |
| C | 490 ms | 0,98× | 7 | 4,8 MB | 359 ms | 61 KB |
| Rust | 490 ms | 0,98× | 7 | 4,9 MB | 512 ms | 414 KB |
| C# | 660 ms | 1,31× | 7 | 23,6 MB | 1,03 s | 4 KB |
| Java | 678 ms | 1,35× | 7 | 57,4 MB | 872 ms | 1 KB |
| JavaScript | 586 ms | 1,17× | 7 | 37,3 MB | – | – |
| Python | 19,02 s | 38× | 3 | 23,4 MB | – | – |

### collatz

Längste Collatz-Folge für Startwerte unter 1 Million. Misst Schleifen, Verzweigungen und Rechnen mit ganzen Zahlen.

| Sprache | Zeit | zu OIDA | Runden | Speicher | Bauen | Größe |
|---|---:|---:|---:|---:|---:|---:|
| OIDA | 199 ms | 1,00× | 7 | 4,9 MB | 442 ms | 292 KB |
| C | 212 ms | 1,07× | 7 | 4,8 MB | 378 ms | 61 KB |
| Rust | 205 ms | 1,03× | 7 | 4,9 MB | 606 ms | 414 KB |
| C# | 398 ms | 2,01× | 7 | 24,2 MB | 1,01 s | 4 KB |
| Java | 455 ms | 2,29× | 7 | 57,9 MB | 858 ms | 1 KB |
| JavaScript | 2,91 s | 15× | 5 | 37,3 MB | – | – |
| Python | 29,84 s | 150× | 1 | 23,4 MB | – | – |

### mandelbrot

Mandelbrot-Menge auf 1000 x 1000 Punkten. Höchstens 300 Schritte je Punkt; misst Rechnen mit Kommazahlen.

| Sprache | Zeit | zu OIDA | Runden | Speicher | Bauen | Größe |
|---|---:|---:|---:|---:|---:|---:|
| OIDA | 251 ms | 1,00× | 7 | 4,9 MB | 441 ms | 292 KB |
| C | 372 ms | 1,48× | 7 | 4,8 MB | 358 ms | 62 KB |
| Rust | 248 ms | 0,99× | 7 | 4,9 MB | 565 ms | 414 KB |
| C# | 345 ms | 1,37× | 7 | 23,7 MB | 1,08 s | 4 KB |
| Java | 511 ms | 2,03× | 7 | 57,9 MB | 921 ms | 1 KB |
| JavaScript | 326 ms | 1,30× | 7 | 38,6 MB | – | – |
| Python | 28,85 s | 115× | 1 | 23,4 MB | – | – |

### texte

Eine Million kurze Texte zusammenbauen und vergleichen. Misst Texte und den Speicher dafür.

| Sprache | Zeit | zu OIDA | Runden | Speicher | Bauen | Größe |
|---|---:|---:|---:|---:|---:|---:|
| OIDA | 172 ms | 1,00× | 7 | 4,9 MB | 343 ms | 296 KB |
| C | 115 ms | 0,67× | 7 | 4,8 MB | 357 ms | 63 KB |
| Rust | 224 ms | 1,30× | 7 | 5,0 MB | 502 ms | 416 KB |
| C# | 144 ms | 0,84× | 7 | 33,7 MB | 945 ms | 4 KB |
| Java | 193 ms | 1,12× | 7 | 96,9 MB | 823 ms | 1 KB |
| JavaScript | 143 ms | 0,83× | 7 | 45,3 MB | – | – |
| Python | 611 ms | 3,56× | 7 | 23,5 MB | – | – |

### ausgabe

Zwei Millionen Zeilen ausgeben. Misst die Ausgabe.

| Sprache | Zeit | zu OIDA | Runden | Speicher | Bauen | Größe |
|---|---:|---:|---:|---:|---:|---:|
| OIDA | 119 ms | 1,00× | 7 | 5,0 MB | 353 ms | 292 KB |
| C | 531 ms | 4,46× | 7 | 4,8 MB | 314 ms | 61 KB |
| Rust | 184 ms | 1,54× | 7 | 4,9 MB | 591 ms | 420 KB |
| C# | 257 ms | 2,15× | 7 | 34,6 MB | 916 ms | 4 KB |
| Java | 488 ms | 4,09× | 7 | 169,7 MB | 840 ms | 1 KB |
| JavaScript | 369 ms | 3,10× | 7 | 80,4 MB | – | – |
| Python | 5,28 s | 44× | 3 | 25,0 MB | – | – |

