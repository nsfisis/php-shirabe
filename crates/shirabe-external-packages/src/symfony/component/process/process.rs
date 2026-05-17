use indexmap::IndexMap;

#[derive(Debug)]
pub struct Process;

impl Process {
    pub const ERR: &'static str = "err";
    pub const OUT: &'static str = "out";

    pub fn new(
        _command: Vec<String>,
        _cwd: Option<String>,
        _env: Option<IndexMap<String, String>>,
        _input: Option<String>,
        _timeout: Option<f64>,
    ) -> Self {
        todo!()
    }

    pub fn from_shell_commandline(
        _command: &str,
        _cwd: Option<&str>,
        _env: Option<IndexMap<String, String>>,
        _input: Option<String>,
        _timeout: Option<f64>,
    ) -> Self {
        todo!()
    }

    pub fn set_timeout(&mut self, _timeout: Option<f64>) -> &mut Self {
        todo!()
    }

    pub fn set_env(&mut self, _env: IndexMap<String, String>) -> &mut Self {
        todo!()
    }

    pub fn set_input(&mut self, _input: Option<String>) -> &mut Self {
        todo!()
    }

    pub fn run(&mut self, _callback: Option<Box<dyn FnMut(&str, &str)>>) -> i64 {
        todo!()
    }

    pub fn must_run(
        &mut self,
        _callback: Option<Box<dyn FnMut(&str, &str)>>,
    ) -> anyhow::Result<&mut Self> {
        todo!()
    }

    pub fn start(&mut self, _callback: Option<Box<dyn FnMut(&str, &str)>>) {
        todo!()
    }

    pub fn wait(&mut self, _callback: Option<Box<dyn FnMut(&str, &str)>>) -> i64 {
        todo!()
    }

    pub fn stop(&mut self, _timeout: f64, _signal: Option<i64>) -> Option<i64> {
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

    pub fn set_working_directory(&mut self, _cwd: &str) -> &mut Self {
        todo!()
    }
}
