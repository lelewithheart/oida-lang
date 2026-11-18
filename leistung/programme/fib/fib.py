# Fibonacci-Zahl 40, rekursiv. Misst Funktionsaufrufe (rund 330 Millionen).

def fib(n):
    if n < 2:
        return n
    return fib(n - 1) + fib(n - 2)


print(fib(40))
