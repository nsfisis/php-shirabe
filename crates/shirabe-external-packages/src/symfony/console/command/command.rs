/// Stub for \Symfony\Component\Console\Command\Command.
pub trait Command {
    fn get_name(&self) -> Option<String> {
        todo!()
    }

    fn set_name(&mut self, _name: &str) {
        todo!()
    }

    fn get_description(&self) -> String {
        todo!()
    }

    fn set_description(&mut self, _description: &str) {
        todo!()
    }
}
