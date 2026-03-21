use global_interning::{INTERNERS, Interned};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Hash, PartialEq, Eq, Serialize, Deserialize)]
struct Person {
    name: Interned<str>,
    #[serde(default)]
    parents: Interned<[Interned<Person>]>,
}

fn main() {
    let person: Interned<Person> = serde_json::from_value(json!({
        "name": "Alice",
        "parents": [
            {
                "name": "Alice",
                "parents": [
                    {
                        "name": "Alice"
                    },
                    {
                        "name": "Bob"
                    }
                ]
            },
            {
                "name": "Alice",
                "parents": [
                    {
                        "name": "Alice"
                    },
                    {
                        "name": "Bob"
                    }
                ]
            }
        ]
    }))
    .unwrap();

    println!("Deserialized:");
    println!("{person:#?}");

    let output = serde_json::to_string_pretty(&person).unwrap();

    println!();
    println!("Serialized:");
    println!("{output}");

    println!();
    INTERNERS.for_each_mut(|interner| {
        println!("Interned<{}>", interner.name());
        println!("  {} distinct value(s) interned", interner.len(),);
        println!(
            "  {} duplicated value(s) avoided in total",
            interner.sum_duplicates()
        );
        println!("  {} value(s) unused", interner.count_unused());
    });

    drop(person);
    let (total, passes) = INTERNERS.cleanup_all();
    println!("removed {total} values using {passes} passes");
}
