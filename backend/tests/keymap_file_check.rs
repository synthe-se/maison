//! Guards a hand-written keymap against the real parser before it is
//! installed. Run with KEYMAP_FILE=/path/to/ir-keymap.json.
#![cfg(feature = "live-runtime-tests")]

#[test]
fn candidate_keymap_parses() {
    let Ok(path) = std::env::var("KEYMAP_FILE") else {
        eprintln!("skipping: set KEYMAP_FILE");
        return;
    };
    let body = std::fs::read_to_string(&path).expect("readable keymap");
    let keymap = maison_backend::ir::parse_keymap(&body)
        .unwrap_or_else(|error| panic!("{path} is not a valid keymap: {error}"));
    for (code, binding) in &keymap {
        if let Some(reason) = maison_backend::ir::invalid_reason(&binding.actions) {
            panic!("binding {code} is invalid: {reason}");
        }
    }
    println!("{} bindings parsed and validated", keymap.len());
}
