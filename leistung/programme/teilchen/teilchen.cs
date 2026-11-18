// Hunderttausend Teilchen in einer Liste, 300 Runden lang bewegt. Misst Listen von Strukturen.

using System;
using System.Collections.Generic;

var alle = new List<Teilchen>();
for (long i = 0; i < 100000; i++)
{
    alle.Add(new Teilchen { X = i * 37 % 1000, Y = i * 91 % 1000, Vx = i % 7 - 3, Vy = i % 5 - 2 });
}

for (int runde = 1; runde <= 300; runde++)
{
    foreach (var t in alle)
    {
        t.X += t.Vx * 0.5;
        t.Y += t.Vy * 0.5;
        if (t.X < 0 || t.X > 1000)
        {
            t.Vx = -t.Vx;
        }
        if (t.Y < 0 || t.Y > 1000)
        {
            t.Vy = -t.Vy;
        }
    }
}

double summe = 0.0;
foreach (var t in alle)
{
    summe += t.X + t.Y;
}
Console.WriteLine((long)summe);

// In C# nimmt man dafür üblicherweise eine Klasse: Jedes Teilchen ist ein eigenes Objekt am Heap.
class Teilchen
{
    public double X, Y, Vx, Vy;
}
