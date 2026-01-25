pub mod config;
pub use config::*;
pub mod cli;
pub mod error;
pub use error::*;
pub mod schema;

fn main() {
    println!("Hello, world!");
}
