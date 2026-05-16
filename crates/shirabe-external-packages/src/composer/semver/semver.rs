#[derive(Debug)]
pub struct Semver;

impl Semver {
    pub fn sort(versions: Vec<String>) -> anyhow::Result<Vec<String>> {
        todo!()
    }

    pub fn rsort(versions: Vec<String>) -> Vec<String> {
        todo!()
    }

    pub fn satisfies(version: &str, constraint: &str) -> bool {
        todo!()
    }
}
