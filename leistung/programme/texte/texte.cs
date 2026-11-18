// Eine Million kurze Texte zusammenbauen und vergleichen. Misst Texte und den Speicher dafür.

using System;

long kleiner = 0;
long gleich = 0;
for (long i = 1; i <= 1000000; i++)
{
    string s = "nr" + i + "!";
    if (string.CompareOrdinal(s, "nr5") < 0)
    {
        kleiner++;
    }
    if (s == "nr777!")
    {
        gleich++;
    }
}
Console.WriteLine(kleiner + " " + gleich);
