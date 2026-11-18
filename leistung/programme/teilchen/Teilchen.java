// Hunderttausend Teilchen in einer Liste, 300 Runden lang bewegt. Misst Listen von Strukturen.

import java.util.ArrayList;

public class Teilchen {
    double x, y, vx, vy;

    Teilchen(double x, double y, double vx, double vy) {
        this.x = x;
        this.y = y;
        this.vx = vx;
        this.vy = vy;
    }

    public static void main(String[] argumente) {
        ArrayList<Teilchen> alle = new ArrayList<>();
        for (long i = 0; i < 100000; i++) {
            alle.add(new Teilchen(i * 37 % 1000, i * 91 % 1000, i % 7 - 3, i % 5 - 2));
        }

        for (int runde = 1; runde <= 300; runde++) {
            for (Teilchen t : alle) {
                t.x += t.vx * 0.5;
                t.y += t.vy * 0.5;
                if (t.x < 0 || t.x > 1000) {
                    t.vx = -t.vx;
                }
                if (t.y < 0 || t.y > 1000) {
                    t.vy = -t.vy;
                }
            }
        }

        double summe = 0.0;
        for (Teilchen t : alle) {
            summe += t.x + t.y;
        }
        System.out.println((long) summe);
    }
}
