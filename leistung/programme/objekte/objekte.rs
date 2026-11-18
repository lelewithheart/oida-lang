// Hunderttausend Objekte aus drei Klassen, 200 Runden lang bewegt. Misst Klassen und überschriebene Methoden.
//
// Rust hat keine Vererbung. Das Übliche stattdessen: ein Trait mit den Methoden
// und `Box<dyn ...>`; was alle gemeinsam haben, steckt in einer eigenen Struktur.

struct Grund {
    x: i64,
    y: i64,
    tempo: i64,
}

impl Grund {
    fn neu(i: i64) -> Grund {
        Grund { x: i * 37 % 1000, y: i * 91 % 1000, tempo: i % 7 + 1 }
    }

    fn beweg(&mut self) {
        self.x = (self.x + self.tempo) % 1000;
    }
}

trait Wesen {
    fn beweg(&mut self);
    fn wert(&self) -> i64;
}

struct Einfach {
    grund: Grund,
}

struct Hupfa {
    grund: Grund,
    hoch: i64,
}

struct Kreisla {
    grund: Grund,
}

impl Wesen for Einfach {
    fn beweg(&mut self) {
        self.grund.beweg();
    }

    fn wert(&self) -> i64 {
        self.grund.x + self.grund.y
    }
}

impl Wesen for Hupfa {
    fn beweg(&mut self) {
        self.grund.beweg();
        self.hoch = (self.hoch + 7) % 50;
        self.grund.y = (self.grund.y + self.hoch) % 1000;
    }

    fn wert(&self) -> i64 {
        self.grund.x + self.grund.y
    }
}

impl Wesen for Kreisla {
    fn beweg(&mut self) {
        let oid = self.grund.x;
        self.grund.x = (self.grund.y + self.grund.tempo) % 1000;
        self.grund.y = oid;
    }

    fn wert(&self) -> i64 {
        self.grund.x * 2 + self.grund.y
    }
}

fn main() {
    let mut alle: Vec<Box<dyn Wesen>> = Vec::new();
    for i in 0..100_000i64 {
        if i % 3 == 0 {
            alle.push(Box::new(Einfach { grund: Grund::neu(i) }));
        } else if i % 3 == 1 {
            alle.push(Box::new(Hupfa { grund: Grund::neu(i), hoch: 0 }));
        } else {
            alle.push(Box::new(Kreisla { grund: Grund::neu(i) }));
        }
    }

    let mut summe: i64 = 0;
    for _runde in 1..=200 {
        for w in alle.iter_mut() {
            w.beweg();
            summe = (summe + w.wert()) % 1_000_000_007;
        }
    }
    println!("{summe}");
}
