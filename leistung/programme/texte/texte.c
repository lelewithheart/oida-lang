// Eine Million kurze Texte zusammenbauen und vergleichen. Misst Texte und den Speicher dafür.

#include <stdint.h>
#include <stdio.h>
#include <string.h>

int main(void) {
    int64_t kleiner = 0;
    int64_t gleich = 0;
    char s[32];
    for (int64_t i = 1; i <= 1000000; i++) {
        snprintf(s, sizeof s, "nr%lld!", (long long)i);
        if (strcmp(s, "nr5") < 0) {
            kleiner++;
        }
        if (strcmp(s, "nr777!") == 0) {
            gleich++;
        }
    }
    printf("%lld %lld\n", (long long)kleiner, (long long)gleich);
    return 0;
}
