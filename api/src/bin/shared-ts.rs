//! Writes one of the web app's generated modules on stdout, from maison-api's tables:
//! `paces` (web/src/lib/paces.ts), `codes` (web/src/lib/codes.ts).

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("paces") => print!("{}", maison_api::paces::typescript()),
        Some("codes") => print!("{}", maison_api::codes::typescript()),
        _ => {
            eprintln!("usage: shared-ts paces|codes");
            std::process::exit(2);
        }
    }
}
