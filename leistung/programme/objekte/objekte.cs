// Hunderttausend Objekte aus drei Klassen, 200 Runden lang bewegt. Misst Klassen und überschriebene Methoden.

using System;
using System.Collections.Generic;

var alle = new List<Wesen>();
for (long i = 0; i < 100000; i++)
{
    if (i % 3 == 0)
    {
        alle.Add(new Wesen(i * 37 % 1000, i * 91 % 1000, i % 7 + 1));
    }
    else if (i % 3 == 1)
    {
        alle.Add(new Hupfa(i * 37 % 1000, i * 91 % 1000, i % 7 + 1));
    }
    else
    {
        alle.Add(new Kreisla(i * 37 % 1000, i * 91 % 1000, i % 7 + 1));
    }
}

long summe = 0;
for (int runde = 1; runde <= 200; runde++)
{
    foreach (var w in alle)
    {
        w.Beweg();
        summe = (summe + w.Wert()) % 1000000007;
    }
}
Console.WriteLine(summe);

class Wesen
{
    public long X, Y, Tempo;

    public Wesen(long x, long y, long tempo)
    {
        X = x;
        Y = y;
        Tempo = tempo;
    }

    public virtual void Beweg()
    {
        X = (X + Tempo) % 1000;
    }

    public virtual long Wert()
    {
        return X + Y;
    }
}

class Hupfa : Wesen
{
    public long Hoch;

    public Hupfa(long x, long y, long tempo) : base(x, y, tempo) { }

    public override void Beweg()
    {
        base.Beweg();
        Hoch = (Hoch + 7) % 50;
        Y = (Y + Hoch) % 1000;
    }
}

class Kreisla : Wesen
{
    public Kreisla(long x, long y, long tempo) : base(x, y, tempo) { }

    public override void Beweg()
    {
        long oid = X;
        X = (Y + Tempo) % 1000;
        Y = oid;
    }

    public override long Wert()
    {
        return X * 2 + Y;
    }
}
