// Fibonacci-Zahl 40, rekursiv. Misst Funktionsaufrufe (rund 330 Millionen).

using System;

Console.WriteLine(Fib(40));

static long Fib(long n)
{
    if (n < 2)
    {
        return n;
    }
    return Fib(n - 1) + Fib(n - 2);
}
