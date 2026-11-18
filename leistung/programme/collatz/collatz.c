// Längste Collatz-Folge für Startwerte unter 1 Million. Misst Schleifen, Verzweigungen und Rechnen mit ganzen Zahlen.

#include <stdint.h>
#include <stdio.h>

int main(void) {
    int64_t bester = 0;
    int64_t laengste = 0;
    for (int64_t start = 1; start < 1000000; start++) {
        int64_t n = start;
        int64_t schritte = 0;
        while (n != 1) {
            if (n % 2 == 0) {
                n = n / 2;
            } else {
                n = 3 * n + 1;
            }
            schritte++;
        }
        if (schritte > laengste) {
            laengste = schritte;
            bester = start;
        }
    }
    printf("%lld %lld\n", (long long)bester, (long long)laengste);
    return 0;
}
