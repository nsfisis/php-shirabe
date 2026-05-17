#[derive(Debug)]
pub struct Linter;

impl Linter {
    pub fn lint(_file: &str, _php_versions: &[String]) -> anyhow::Result<()> {
        todo!()
    }
}
