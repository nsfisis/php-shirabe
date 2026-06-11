use crate::symfony::console::command::command::Command;

pub trait CommandLoaderInterface: std::fmt::Debug {
    /// Loads a command.
    ///
    /// @throws CommandNotFoundException
    fn get(&self, name: &str) -> Box<dyn Command>;

    /// Checks if a command exists.
    fn has(&self, name: &str) -> bool;

    fn get_names(&self) -> Vec<String>;
}
