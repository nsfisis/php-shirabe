use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct Process;

impl Process {
    pub const ERR: &'static str = "err";
    pub const OUT: &'static str = "out";

    pub fn new(
        command: Vec<String>,
        cwd: Option<String>,
        env: Option<IndexMap<String, String>>,
        input: Option<String>,
        timeout: Option<f64>,
    ) -> Self {
        todo!()
    }

    pub fn from_shell_commandline(
        command: &str,
        cwd: Option<&str>,
        env: Option<IndexMap<String, String>>,
        input: Option<String>,
        timeout: Option<f64>,
    ) -> Self {
        todo!()
    }

    pub fn set_timeout(&mut self, timeout: Option<f64>) -> &mut Self {
        todo!()
    }

    pub fn set_env(&mut self, env: IndexMap<String, String>) -> &mut Self {
        todo!()
    }

    pub fn set_input(&mut self, input: Option<String>) -> &mut Self {
        todo!()
    }

    pub fn run(&mut self, callback: Option<Box<dyn FnMut(&str, &str)>>) -> i64 {
        todo!()
    }

    pub fn must_run(
        &mut self,
        callback: Option<Box<dyn FnMut(&str, &str)>>,
    ) -> anyhow::Result<&mut Self> {
        todo!()
    }

    pub fn start(&mut self, callback: Option<Box<dyn FnMut(&str, &str)>>) {
        todo!()
    }

    pub fn wait(&mut self, callback: Option<Box<dyn FnMut(&str, &str)>>) -> i64 {
        todo!()
    }

    pub fn stop(&mut self, timeout: f64, signal: Option<i64>) -> Option<i64> {
        todo!()
    }

    pub fn is_running(&self) -> bool {
        todo!()
    }

    pub fn is_successful(&self) -> bool {
        todo!()
    }

    pub fn is_started(&self) -> bool {
        todo!()
    }

    pub fn is_terminated(&self) -> bool {
        todo!()
    }

    pub fn get_output(&self) -> String {
        todo!()
    }

    pub fn get_error_output(&self) -> String {
        todo!()
    }

    pub fn get_exit_code(&self) -> Option<i64> {
        todo!()
    }

    pub fn get_exit_code_text(&self) -> Option<String> {
        todo!()
    }

    pub fn get_command_line(&self) -> String {
        todo!()
    }

    pub fn check_timeout(&self) -> anyhow::Result<()> {
        todo!()
    }

    pub fn get_timeout(&self) -> Option<f64> {
        todo!()
    }

    pub fn set_working_directory(&mut self, cwd: &str) -> &mut Self {
        todo!()
    }
}
