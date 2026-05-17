#[derive(Debug)]
pub struct SplFileInfo;

impl SplFileInfo {
    pub fn new(_path: &str) -> Self {
        todo!()
    }

    pub fn get_pathname(&self) -> String {
        todo!()
    }

    pub fn get_path(&self) -> String {
        todo!()
    }

    pub fn get_filename(&self) -> String {
        todo!()
    }

    pub fn get_basename(&self, _suffix: Option<&str>) -> String {
        todo!()
    }

    pub fn get_extension(&self) -> String {
        todo!()
    }

    pub fn get_relative_path_name(&self) -> String {
        todo!()
    }

    pub fn get_relative_path(&self) -> String {
        todo!()
    }

    pub fn is_dir(&self) -> bool {
        todo!()
    }

    pub fn is_file(&self) -> bool {
        todo!()
    }

    pub fn is_link(&self) -> bool {
        todo!()
    }

    pub fn get_real_path(&self) -> Option<String> {
        todo!()
    }

    pub fn get_size(&self) -> i64 {
        todo!()
    }
}
