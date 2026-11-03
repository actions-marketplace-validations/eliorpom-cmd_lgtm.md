//! `pycheck <cases.json>`: computes the Python primitives on cases written by
//! `bench/parity.py`, which then compares the bits with CPython's own results.

use serde_json::{json, Value};

use crate::pymath::{py_hypot, py_mod, py_round, py_sum};
use crate::pyrand::Random;

/// The fixed sequence of calls `bench/parity.py` also runs on each seed.
fn rand_script(rng: &mut Random) -> Vec<Value> {
    let mut out = Vec::new();
    for _ in 0..5 {
        out.push(json!(rng.random()));
    }
    out.push(json!(rng.uniform(-3.0, 7.0)));
    for _ in 0..20 {
        out.push(json!(rng.randint(0, 10)));
    }
    out.push(json!(rng.randrange(5, 90)));
    out.push(json!(rng.randrange_to(100)));
    out.push(json!(rng.randrange_step(0, 100, 7)));
    out.push(json!(rng.randrange_step(50, -10, -3)));
    out.push(json!(rng.choice_index(3)));
    out.push(json!(rng.choice_index(1000)));
    out.push(json!(rng.choice_index(1)));
    out.push(json!(rng.randrange(0, 1 << 40)));
    let mut items: Vec<i64> = (0..10).collect();
    rng.shuffle(&mut items);
    out.push(json!(items));
    for _ in 0..3 {
        out.push(json!(rng.gauss(0.0, 85.0)));
    }
    out.push(json!(rng.choices_weighted_index(&[1.0, 2.0, 3.5])));
    out.push(json!(rng.choices_index(7)));
    out.push(json!(rng.random()));
    out
}

pub fn run(path: &std::path::Path) {
    let cases: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let rand: Vec<Value> = cases["seeds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|seed| {
            let mut rng = match seed {
                Value::String(text) => Random::new(text.as_str()),
                other => Random::new(other.as_i64().unwrap()),
            };
            json!(rand_script(&mut rng))
        })
        .collect();
    let hypot: Vec<f64> = cases["hypot"]
        .as_array()
        .unwrap()
        .iter()
        .map(|pair| py_hypot(pair[0].as_f64().unwrap(), pair[1].as_f64().unwrap()))
        .collect();
    let sums: Vec<f64> = cases["sum"]
        .as_array()
        .unwrap()
        .iter()
        .map(|list| py_sum(list.as_array().unwrap().iter().map(|v| v.as_f64().unwrap())))
        .collect();
    let rounds: Vec<i64> = cases["round"].as_array().unwrap().iter().map(|v| py_round(v.as_f64().unwrap())).collect();
    let mods: Vec<f64> = cases["mod"]
        .as_array()
        .unwrap()
        .iter()
        .map(|pair| py_mod(pair[0].as_f64().unwrap(), pair[1].as_f64().unwrap()))
        .collect();
    println!("{}", json!({"rand": rand, "hypot": hypot, "sum": sums, "round": rounds, "mod": mods}));
}
