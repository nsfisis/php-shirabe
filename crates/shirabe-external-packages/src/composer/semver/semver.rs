#[derive(Debug)]
pub struct Semver;

impl Semver {
    pub fn sort(_versions: Vec<String>) -> anyhow::Result<Vec<String>> {
        todo!()
    }

    pub fn rsort(_versions: Vec<String>) -> Vec<String> {
        todo!()
    }

    pub fn satisfies(_version: &str, _constraint: &str) -> bool {
        todo!()
    }
}
