pub trait Querier {
    fn query(&self, query: &str) -> String;
}
