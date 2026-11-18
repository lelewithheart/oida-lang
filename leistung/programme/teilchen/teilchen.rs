// Hunderttausend Teilchen in einer Liste, 300 Runden lang bewegt. Misst Listen von Strukturen.

struct Teilchen {
    x: f64,
    y: f64,
    vx: f64,
    vy: f64,
}

fn main() {
    let mut alle: Vec<Teilchen> = Vec::new();
    for i in 0..100_000i64 {
        alle.push(Teilchen {
            x: (i * 37 % 1000) as f64,
            y: (i * 91 % 1000) as f64,
            vx: (i % 7 - 3) as f64,
            vy: (i % 5 - 2) as f64,
        });
    }

    for _runde in 1..=300 {
        for t in alle.iter_mut() {
            t.x += t.vx * 0.5;
            t.y += t.vy * 0.5;
            if t.x < 0.0 || t.x > 1000.0 {
                t.vx = -t.vx;
            }
            if t.y < 0.0 || t.y > 1000.0 {
                t.vy = -t.vy;
            }
        }
    }

    let mut summe = 0.0f64;
    for t in &alle {
        summe += t.x + t.y;
    }
    println!("{}", summe as i64);
}
