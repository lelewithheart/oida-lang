// Primzahlen unter 2 Millionen zählen, durch Probeteilen. Misst Schleifen und Teilen mit Rest.

using System;

long anzahl = 0;
for (long n = 2; n < 2000000; n++)
{
    if (IstPrim(n))
    {
        anzahl++;
    }
}
Console.WriteLine(anzahl);

static bool IstPrim(long n)
{
    for (long t = 2; t * t <= n; t++)
    {
        if (n % t == 0)
        {
            return false;
        }
    }
    return true;
}
