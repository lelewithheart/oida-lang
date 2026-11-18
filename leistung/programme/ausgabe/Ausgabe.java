// Zwei Millionen Zeilen ausgeben. Misst die Ausgabe.

import java.io.BufferedOutputStream;
import java.io.PrintStream;

public class Ausgabe {
    public static void main(String[] argumente) {
        // System.out leert nach jeder Zeile; für viel Ausgabe nimmt man in Java einen Puffer.
        PrintStream aus = new PrintStream(new BufferedOutputStream(System.out, 1 << 16), false);
        for (long i = 1; i <= 2000000; i++) {
            aus.println(i + " " + (i * 3));
        }
        aus.flush();
    }
}
