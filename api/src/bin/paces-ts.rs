//! Writes the web app's paces module (web/src/lib/paces.ts) on stdout, from `maison_api::paces`.

fn main() {
    print!("{}", maison_api::paces::typescript());
}
