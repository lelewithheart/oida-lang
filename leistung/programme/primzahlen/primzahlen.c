// Primzahlen unter 2 Millionen zählen, durch Probeteilen. Misst Schleifen und Teilen mit Rest.

#include <stdint.h>
#include <stdio.h>

static int ist_prim(int64_t n) {
    for (int64_t t = 2; t * t <= n; t++) {
        if (n % t == 0) {
            return 0;
        }
    }
    return 1;
}

int main(void) {
    int64_t anzahl = 0;
    for (int64_t n = 2; n < 2000000; n++) {
        if (ist_prim(n)) {
            anzahl++;
        }
    }
    printf("%lld\n", (long long)anzahl);
    return 0;
}
