// Längste Collatz-Folge für Startwerte unter 1 Million. Misst Schleifen, Verzweigungen und Rechnen mit ganzen Zahlen.

using System;

long bester = 0;
long laengste = 0;
for (long start = 1; start < 1000000; start++)
{
    long n = start;
    long schritte = 0;
    while (n != 1)
    {
        if (n % 2 == 0)
        {
            n = n / 2;
        }
        else
        {
            n = 3 * n + 1;
        }
        schritte++;
    }
    if (schritte > laengste)
    {
        laengste = schritte;
        bester = start;
    }
}
Console.WriteLine(bester + " " + laengste);
