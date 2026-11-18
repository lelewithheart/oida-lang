// Hunderttausend Objekte aus drei Klassen, 200 Runden lang bewegt. Misst Klassen und überschriebene Methoden.

import java.util.ArrayList;

public class Objekte {
    static class Wesen {
        long x, y, tempo;

        Wesen(long x, long y, long tempo) {
            this.x = x;
            this.y = y;
            this.tempo = tempo;
        }

        void beweg() {
            x = (x + tempo) % 1000;
        }

        long wert() {
            return x + y;
        }
    }

    static class Hupfa extends Wesen {
        long hoch;

        Hupfa(long x, long y, long tempo) {
            super(x, y, tempo);
        }

        @Override
        void beweg() {
            super.beweg();
            hoch = (hoch + 7) % 50;
            y = (y + hoch) % 1000;
        }
    }

    static class Kreisla extends Wesen {
        Kreisla(long x, long y, long tempo) {
            super(x, y, tempo);
        }

        @Override
        void beweg() {
            long oid = x;
            x = (y + tempo) % 1000;
            y = oid;
        }

        @Override
        long wert() {
            return x * 2 + y;
        }
    }

    public static void main(String[] argumente) {
        ArrayList<Wesen> alle = new ArrayList<>();
        for (long i = 0; i < 100000; i++) {
            if (i % 3 == 0) {
                alle.add(new Wesen(i * 37 % 1000, i * 91 % 1000, i % 7 + 1));
            } else if (i % 3 == 1) {
                alle.add(new Hupfa(i * 37 % 1000, i * 91 % 1000, i % 7 + 1));
            } else {
                alle.add(new Kreisla(i * 37 % 1000, i * 91 % 1000, i % 7 + 1));
            }
        }

        long summe = 0;
        for (int runde = 1; runde <= 200; runde++) {
            for (Wesen w : alle) {
                w.beweg();
                summe = (summe + w.wert()) % 1000000007;
            }
        }
        System.out.println(summe);
    }
}
