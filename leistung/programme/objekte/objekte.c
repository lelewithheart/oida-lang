// Hunderttausend Objekte aus drei Klassen, 200 Runden lang bewegt. Misst Klassen und überschriebene Methoden.
//
// C hat keine Klassen. Das Übliche stattdessen: Jedes Objekt trägt einen Zeiger
// auf eine Tabelle mit seinen Funktionen.

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

typedef struct Wesen Wesen;

typedef struct {
    void (*beweg)(Wesen *);
    int64_t (*wert)(Wesen *);
} Klasse;

struct Wesen {
    const Klasse *klasse;
    int64_t x, y, tempo;
};

typedef struct {
    Wesen wesen;
    int64_t hoch;
} Hupfa;

static void wesen_beweg(Wesen *w) {
    w->x = (w->x + w->tempo) % 1000;
}

static int64_t wesen_wert(Wesen *w) {
    return w->x + w->y;
}

static void hupfa_beweg(Wesen *w) {
    Hupfa *h = (Hupfa *)w;
    wesen_beweg(w);
    h->hoch = (h->hoch + 7) % 50;
    w->y = (w->y + h->hoch) % 1000;
}

static void kreisla_beweg(Wesen *w) {
    int64_t oid = w->x;
    w->x = (w->y + w->tempo) % 1000;
    w->y = oid;
}

static int64_t kreisla_wert(Wesen *w) {
    return w->x * 2 + w->y;
}

static const Klasse WESEN = {wesen_beweg, wesen_wert};
static const Klasse HUPFA = {hupfa_beweg, wesen_wert};
static const Klasse KREISLA = {kreisla_beweg, kreisla_wert};

static Wesen *neu(const Klasse *klasse, size_t groesse, int64_t i) {
    Wesen *w = calloc(1, groesse);
    w->klasse = klasse;
    w->x = i * 37 % 1000;
    w->y = i * 91 % 1000;
    w->tempo = i % 7 + 1;
    return w;
}

int main(void) {
    int64_t anzahl = 100000;
    Wesen **alle = malloc(anzahl * sizeof(Wesen *));
    for (int64_t i = 0; i < anzahl; i++) {
        if (i % 3 == 0) {
            alle[i] = neu(&WESEN, sizeof(Wesen), i);
        } else if (i % 3 == 1) {
            alle[i] = neu(&HUPFA, sizeof(Hupfa), i);
        } else {
            alle[i] = neu(&KREISLA, sizeof(Wesen), i);
        }
    }

    int64_t summe = 0;
    for (int64_t runde = 1; runde <= 200; runde++) {
        for (int64_t i = 0; i < anzahl; i++) {
            Wesen *w = alle[i];
            w->klasse->beweg(w);
            summe = (summe + w->klasse->wert(w)) % 1000000007;
        }
    }
    printf("%lld\n", (long long)summe);

    for (int64_t i = 0; i < anzahl; i++) {
        free(alle[i]);
    }
    free(alle);
    return 0;
}
