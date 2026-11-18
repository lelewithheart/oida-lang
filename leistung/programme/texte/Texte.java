// Eine Million kurze Texte zusammenbauen und vergleichen. Misst Texte und den Speicher dafür.

public class Texte {
    public static void main(String[] argumente) {
        long kleiner = 0;
        long gleich = 0;
        for (long i = 1; i <= 1000000; i++) {
            String s = "nr" + i + "!";
            if (s.compareTo("nr5") < 0) {
                kleiner++;
            }
            if (s.equals("nr777!")) {
                gleich++;
            }
        }
        System.out.println(kleiner + " " + gleich);
    }
}
