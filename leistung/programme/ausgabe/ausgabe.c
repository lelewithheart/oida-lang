// Zwei Millionen Zeilen ausgeben. Misst die Ausgabe.

#include <stdint.h>
#include <stdio.h>

int main(void) {
    for (int64_t i = 1; i <= 2000000; i++) {
        printf("%lld %lld\n", (long long)i, (long long)(i * 3));
    }
    return 0;
}
