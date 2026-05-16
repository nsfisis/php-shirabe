pub trait OutputFormatterInterface {
    fn is_decorated(&self) -> bool;
    fn set_decorated(&mut self, decorated: bool);
    fn format(&self, message: &str) -> String;
}
