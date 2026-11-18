// Eine Million kurze Texte zusammenbauen und vergleichen. Misst Texte und den Speicher dafür.

fn main() {
    let mut kleiner: i64 = 0;
    let mut gleich: i64 = 0;
    for i in 1..=1_000_000i64 {
        let s = format!("nr{i}!");
        if s.as_str() < "nr5" {
            kleiner += 1;
        }
        if s == "nr777!" {
            gleich += 1;
        }
    }
    println!("{kleiner} {gleich}");
}
