// Hunderttausend Teilchen in einer Liste, 300 Runden lang bewegt. Misst Listen von Strukturen.

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

typedef struct {
    double x, y, vx, vy;
} Teilchen;

int main(void) {
    int64_t anzahl = 100000;
    Teilchen *alle = malloc(anzahl * sizeof(Teilchen));
    for (int64_t i = 0; i < anzahl; i++) {
        alle[i] = (Teilchen){(double)(i * 37 % 1000), (double)(i * 91 % 1000), (double)(i % 7 - 3), (double)(i % 5 - 2)};
    }

    for (int64_t runde = 1; runde <= 300; runde++) {
        for (int64_t i = 0; i < anzahl; i++) {
            alle[i].x += alle[i].vx * 0.5;
            alle[i].y += alle[i].vy * 0.5;
            if (alle[i].x < 0 || alle[i].x > 1000) {
                alle[i].vx = -alle[i].vx;
            }
            if (alle[i].y < 0 || alle[i].y > 1000) {
                alle[i].vy = -alle[i].vy;
            }
        }
    }

    double summe = 0.0;
    for (int64_t i = 0; i < anzahl; i++) {
        summe += alle[i].x + alle[i].y;
    }
    printf("%lld\n", (long long)summe);
    free(alle);
    return 0;
}
