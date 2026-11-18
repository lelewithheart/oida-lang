// Primzahlen unter 2 Millionen zählen, durch Probeteilen. Misst Schleifen und Teilen mit Rest.

public class Primzahlen {
    static boolean istPrim(long n) {
        for (long t = 2; t * t <= n; t++) {
            if (n % t == 0) {
                return false;
            }
        }
        return true;
    }

    public static void main(String[] argumente) {
        long anzahl = 0;
        for (long n = 2; n < 2000000; n++) {
            if (istPrim(n)) {
                anzahl++;
            }
        }
        System.out.println(anzahl);
    }
}
