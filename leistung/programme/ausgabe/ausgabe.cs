// Zwei Millionen Zeilen ausgeben. Misst die Ausgabe.

using System;
using System.IO;
using System.Text;

// Console.WriteLine leert nach jeder Zeile; für viel Ausgabe nimmt man in C# einen Puffer.
using var aus = new StreamWriter(Console.OpenStandardOutput(), new UTF8Encoding(false), 1 << 16);
for (long i = 1; i <= 2000000; i++)
{
    aus.WriteLine(i + " " + (i * 3));
}
